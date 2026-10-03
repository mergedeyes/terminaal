//! Folder sync: pairs of a folder here and one on the server
//! ([`SyncPair`], a host's `[[host.sync]]`), kept alike over an SFTP
//! session ([`super::session`]).
//!
//! - **When**: a pair syncs whenever its session attaches -- the files tab
//!   opening, or after a reconnect -- and on "Sync now". A `live` pair runs
//!   in a session of its own in the background as long as a terminal is
//!   logged in to the host (`app.rs`), and also syncs a moment after
//!   something changed in its local folder (inotify, [`LIVE_SETTLE`]).
//!   Changes on the server only show up with the next sync.
//! - **What changed**: a [`Record`] per pair (in `$XDG_STATE_HOME/terminaal/
//!   sync/`) holds each file's size and modification times on both sides as
//!   of the last sync. A side whose file differs from it changed. Copies get
//!   their source's modification time, so after a sync both sides agree
//!   even without a record.
//! - **Direction** ([`SyncDirection`]): two-way copies whatever changed to
//!   the other side; upload and download only from one side to the other.
//! - **Conflicts**: changed on both sides (or, one-way, the target changed
//!   too) and not the same content -- nothing is lost: the older copy (or
//!   the target's own) is kept next to it as `name (conflict).ext`, the
//!   other one takes the name. Same size on both sides is checked by
//!   SHA-256 first ([`HASHES_COMMAND`]), so an identical file with another
//!   time is no conflict.
//! - **Deletions** only travel where the pair says ([`SyncPair::delete_remote`],
//!   [`SyncPair::delete_local`]), only for files that were synced before and
//!   haven't changed on the other side since, and never from a side whose
//!   folder is empty (an unmounted disk mustn't empty the server).
//!   Otherwise the deleted file comes back from the other side.
//! - Symlinks, names that aren't UTF-8 or contain a line break, and our own
//!   `.name.part` files are left out, like the pair's `exclude` patterns.
//!   A folder that can't be read is left alone as a whole.
//! - A pair syncs in one place at a time: its record is locked (`flock`)
//!   for the run, another tab or Terminaal finds it [`SyncState::Busy`].

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::edit::{ServerHash, Watcher, parse_hash};
use super::protocol::{self, Attrs};
use super::session::{Channels, Command, Fatal, SftpClient, file_name, join, parent};
use crate::i18n::t;

/// Quiet time after a local change before a live pair syncs.
const LIVE_SETTLE: Duration = Duration::from_secs(1);
/// Longest a live pair waits while its folder keeps changing.
const LIVE_MAX_DELAY: Duration = Duration::from_secs(10);
/// A pair that was busy elsewhere is tried again after this.
const BUSY_RETRY: Duration = Duration::from_secs(30);
/// While copying, the record is saved at most this often.
const SAVE_EVERY: Duration = Duration::from_secs(5);
/// What a live pair's folders are watched for.
const WATCH_MASK: u32 =
    libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_MOVED_FROM | libc::IN_DELETE | libc::IN_CREATE;

/// Hashes the files named on stdin (one per line) on the server: a line
/// `terminaal-hash <hash>` per file, the hash `-` if there's none (no
/// `sha256sum`/`shasum`, unreadable). The marker tells these lines from
/// whatever the login's startup files print (bash reads `.bashrc` even for
/// a command). Like [`super::edit::HASH_COMMAND`], no quotes or backslashes
/// the login shell might read differently; the paths never pass through a
/// shell.
pub const HASHES_COMMAND: &str = "sh -c 'while IFS= read -r p; do if command -v sha256sum >/dev/null 2>&1; then h=$(sha256sum 2>/dev/null < \"$p\"); elif command -v shasum >/dev/null 2>&1; then h=$(shasum -a 256 2>/dev/null < \"$p\"); else h=; fi; echo \"terminaal-hash ${h:--}\"; done'";

/// The hashes in [`HASHES_COMMAND`]'s output, in order; other lines are
/// someone else's.
fn parse_hashes(output: &[u8]) -> Vec<ServerHash> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| line.trim_end().strip_prefix("terminaal-hash "))
        .map(|rest| parse_hash(rest.as_bytes()))
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncDirection {
    /// Changes go whichever way they were made.
    #[default]
    Both,
    /// The folder here is the original; the server's follows it.
    Upload,
    /// The server's folder is the original.
    Download,
}

impl SyncDirection {
    pub const ALL: [Self; 3] = [Self::Both, Self::Upload, Self::Download];

    /// Copies go from here to the server.
    pub fn up(self) -> bool {
        self != Self::Download
    }

    /// Copies go from the server to here.
    pub fn down(self) -> bool {
        self != Self::Upload
    }

    fn both(self) -> bool {
        self == Self::Both
    }

    pub fn arrow(self) -> &'static str {
        match self {
            Self::Both => "⇄",
            Self::Upload => "→",
            Self::Download => "←",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Both => t!("sync-direction-both"),
            Self::Upload => t!("sync-direction-upload"),
            Self::Download => t!("sync-direction-download"),
        }
    }
}

/// A folder here and one on the server, kept alike.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPair {
    /// The folder here; `~/` allowed.
    pub local: String,
    /// The folder on the server: absolute, or relative to the login's home.
    pub remote: String,
    #[serde(default)]
    pub direction: SyncDirection,
    /// Files deleted here are deleted on the server too (two-way, upload).
    #[serde(default, skip_serializing_if = "is_false")]
    pub delete_remote: bool,
    /// Files deleted on the server are deleted here too (two-way, download).
    #[serde(default, skip_serializing_if = "is_false")]
    pub delete_local: bool,
    /// Synced in the background while a terminal is logged in to the host,
    /// local changes as they happen. Otherwise when the files tab opens.
    #[serde(default, skip_serializing_if = "is_false")]
    pub live: bool,
    /// Left out: names, or paths relative to the folder if they have a `/`;
    /// `*` and `?` as wildcards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl SyncPair {
    pub fn local_path(&self) -> PathBuf {
        crate::ssh::expand_tilde(self.local.trim())
    }

    /// Absolute on the server; a relative folder (or `~/…`) is in `home`.
    pub fn remote_path(&self, home: &str) -> String {
        let remote = self.remote.trim();
        let relative = if remote == "~" { Some("") } else { remote.strip_prefix("~/") };
        let path = match relative {
            Some(rest) => join(home, rest),
            None if remote.starts_with('/') => remote.to_string(),
            None => join(home, remote),
        };
        match path.trim_end_matches('/') {
            "" => "/".to_string(),
            trimmed => trimmed.to_string(),
        }
    }

    /// Something to sync, as far as that's known without the server.
    pub fn check(&self) -> Result<(), String> {
        if self.local.trim().is_empty() || self.remote.trim().is_empty() {
            return Err(t!("sync-missing-folder"));
        }
        if !self.local_path().is_absolute() {
            return Err(t!("sync-local-relative", path = self.local.trim()));
        }
        Ok(())
    }

    fn deletes_remote(&self) -> bool {
        self.delete_remote && self.direction.up()
    }

    fn deletes_local(&self) -> bool {
        self.delete_local && self.direction.down()
    }
}

/// `*` (any run of characters) and `?` (one) against all of `text`.
fn glob(pattern: &str, text: &str) -> bool {
    let (pattern, text): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// A download's or upload's part file: `.name.part`.
fn is_part_file(name: &str) -> bool {
    name.len() > ".part".len() + 1 && name.starts_with('.') && name.ends_with(".part")
}

/// `rel` (whose last part is `name`) is left out of the sync.
pub fn excluded(rel: &str, name: &str, patterns: &[String]) -> bool {
    is_part_file(name)
        || name.contains('\n')
        || patterns.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).any(|pattern| {
            if pattern.contains('/') { glob(pattern.trim_matches('/'), rel) } else { glob(pattern, name) }
        })
}

/// What one side has at a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    File { size: u64, mtime: Option<u64>, mode: u32 },
    Dir,
}

/// One side's tree: paths relative to the pair's folder, `/` between parts.
#[derive(Debug, Default)]
pub struct Scan {
    pub nodes: BTreeMap<String, Node>,
    /// Folders that couldn't be read: nothing below them is touched.
    pub unreadable: Vec<String>,
    pub errors: Vec<String>,
}

fn secs(time: io::Result<SystemTime>) -> Option<u64> {
    time.ok()?.duration_since(UNIX_EPOCH).ok().map(|since| since.as_secs())
}

fn child(rel: &str, name: &str) -> String {
    if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") }
}

/// The local folder `root`; only an unreadable `root` itself is an error.
pub fn scan_local(root: &Path, exclude: &[String]) -> io::Result<Scan> {
    let mut scan = Scan::default();
    let mut dirs = vec![String::new()];
    while let Some(rel) = dirs.pop() {
        let dir = if rel.is_empty() { root.to_path_buf() } else { root.join(&rel) };
        let read = match std::fs::read_dir(&dir) {
            Ok(read) => read,
            Err(err) if rel.is_empty() => return Err(err),
            Err(err) => {
                scan.errors.push(format!("{}: {err}", crate::ssh::display_path(&dir)));
                scan.unreadable.push(rel);
                continue;
            }
        };
        for entry in read.flatten() {
            // Server paths are strings: other names can't be synced.
            let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
            let path = child(&rel, &name);
            if excluded(&path, &name, exclude) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
            if meta.is_dir() {
                scan.nodes.insert(path.clone(), Node::Dir);
                dirs.push(path);
            } else if meta.is_file() {
                let mode = meta.permissions().mode() & 0o7777;
                scan.nodes.insert(path, Node::File { size: meta.len(), mtime: secs(meta.modified()), mode });
            }
        }
    }
    Ok(scan)
}

/// The server folder `root`; an error only for `root` itself or a broken
/// connection.
pub fn scan_remote(client: &mut SftpClient, root: &str, exclude: &[String]) -> protocol::Result<Scan> {
    let mut scan = Scan::default();
    let mut dirs = vec![String::new()];
    while let Some(rel) = dirs.pop() {
        let dir = if rel.is_empty() { root.to_string() } else { join(root, &rel) };
        let entries = match client.list(&dir) {
            Ok(entries) => entries,
            Err(err) if rel.is_empty() || err.is_fatal() => return Err(err),
            Err(err) => {
                scan.errors.push(format!("{dir}: {err}"));
                scan.unreadable.push(rel);
                continue;
            }
        };
        for entry in entries {
            let path = child(&rel, &entry.name);
            if entry.name.contains('/') || excluded(&path, &entry.name, exclude) {
                continue;
            }
            if entry.attrs.is_dir() {
                scan.nodes.insert(path.clone(), Node::Dir);
                dirs.push(path);
            } else if entry.attrs.is_file() {
                let mode = entry.attrs.permissions.unwrap_or(0o644) & 0o7777;
                let mtime = entry.attrs.mtime().map(u64::from);
                scan.nodes.insert(path, Node::File { size: entry.attrs.size.unwrap_or(0), mtime, mode });
            }
        }
    }
    Ok(scan)
}

/// A path as of the last sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rec {
    /// Size, and each side's modification time.
    File { size: u64, local: Option<u64>, remote: Option<u64> },
    Dir,
}

/// Everything that was alike on both sides after the last sync.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub entries: BTreeMap<String, Rec>,
}

impl Record {
    /// `f<TAB>size<TAB>local<TAB>remote<TAB>path` or `d<TAB>path` per line,
    /// `-` for an unknown time; `#` lines are comments. Bad lines are
    /// skipped -- the worst that does is a conflict copy.
    pub fn parse(text: &str) -> Self {
        let time = |field: &str| field.parse::<u64>().ok();
        let mut entries = BTreeMap::new();
        for line in text.lines().filter(|line| !line.starts_with('#')) {
            let fields: Vec<&str> = line.splitn(5, '\t').collect();
            match fields.as_slice() {
                ["d", path] if !path.is_empty() => drop(entries.insert(path.to_string(), Rec::Dir)),
                ["f", size, local, remote, path] if !path.is_empty() => {
                    if let Ok(size) = size.parse() {
                        entries.insert(path.to_string(), Rec::File { size, local: time(local), remote: time(remote) });
                    }
                }
                _ => {}
            }
        }
        Self { entries }
    }

    pub fn to_text(&self, header: &str) -> String {
        let time = |value: Option<u64>| value.map_or("-".to_string(), |value| value.to_string());
        let mut text = format!("# {}\n", header.replace('\n', " "));
        for (path, rec) in &self.entries {
            match rec {
                Rec::Dir => text.push_str(&format!("d\t{path}\n")),
                Rec::File { size, local, remote } => {
                    text.push_str(&format!("f\t{size}\t{}\t{}\t{path}\n", time(*local), time(*remote)));
                }
            }
        }
        text
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Local,
    Remote,
}

/// One thing a sync does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Upload(String),
    Download(String),
    /// Synced before, deleted on one side since, and deletions don't go
    /// that way: copied back `to` that side (and the user told so).
    Restore { path: String, to: Side },
    DeleteLocal(String),
    DeleteRemote(String),
    /// A folder, only if it's empty by then.
    RmdirLocal(String),
    RmdirRemote(String),
    MkdirLocal(String),
    MkdirRemote(String),
    /// Changed on both sides (two-way): the older copy is kept under a
    /// conflict name on both sides, the newer one takes the name.
    KeepBoth { path: String, newer: Side },
    /// One-way, the target changed too: its copy is moved aside under a
    /// conflict name, the source's takes the name.
    MoveAside { path: String, target: Side },
    Record(String, Rec),
    Forget(String),
    /// A file on one side, a folder on the other: left alone.
    Mismatch(String),
}

impl Step {
    /// Runs before or after which: records, deletions (deepest folders
    /// first), new folders (outermost first), conflicts, then copies.
    fn order(&self) -> (u8, isize) {
        let depth = |path: &str| path.matches('/').count() as isize;
        match self {
            Self::Record(..) | Self::Forget(_) | Self::Mismatch(_) => (0, 0),
            Self::DeleteLocal(_) | Self::DeleteRemote(_) => (1, 0),
            Self::RmdirLocal(path) | Self::RmdirRemote(path) => (2, -depth(path)),
            Self::MkdirLocal(path) | Self::MkdirRemote(path) => (3, depth(path)),
            Self::KeepBoth { .. } | Self::MoveAside { .. } => (4, 0),
            Self::Upload(_) | Self::Download(_) | Self::Restore { .. } => (5, 0),
        }
    }
}

/// The side's file differs from the record (or there's none).
fn changed(size: u64, mtime: Option<u64>, rec: Option<&Rec>, side: Side) -> bool {
    match rec {
        Some(Rec::File { size: was, local, remote }) => {
            size != *was || mtime != if side == Side::Local { *local } else { *remote }
        }
        _ => true,
    }
}

/// What to do to make both sides alike, from their trees and the record.
pub fn plan(pair: &SyncPair, local: &Scan, remote: &Scan, record: &Record) -> Vec<Step> {
    let direction = pair.direction;
    // Deletions only count from a side that has anything at all.
    let delete_local = pair.deletes_local() && !remote.nodes.is_empty();
    let delete_remote = pair.deletes_remote() && !local.nodes.is_empty();
    let mut skip: Vec<String> = local.unreadable.iter().chain(&remote.unreadable).cloned().collect();
    let paths: BTreeSet<&String> = local.nodes.keys().chain(remote.nodes.keys()).chain(record.entries.keys()).collect();
    let mut steps = Vec::new();
    for path in paths {
        if skip.iter().any(|above| path.len() > above.len() && path.starts_with(above.as_str()) && path.as_bytes()[above.len()] == b'/') {
            continue;
        }
        let rec = record.entries.get(path);
        let path = path.clone();
        match (local.nodes.get(&path), remote.nodes.get(&path)) {
            (Some(&Node::File { size: l_size, mtime: l_time, .. }), Some(&Node::File { size: r_size, mtime: r_time, .. })) => {
                if l_size == r_size && l_time == r_time {
                    let synced = Rec::File { size: l_size, local: l_time, remote: r_time };
                    if rec != Some(&synced) {
                        steps.push(Step::Record(path, synced));
                    }
                    continue;
                }
                let local_changed = changed(l_size, l_time, rec, Side::Local);
                let remote_changed = changed(r_size, r_time, rec, Side::Remote);
                let newer = if l_time.unwrap_or(0) >= r_time.unwrap_or(0) { Side::Local } else { Side::Remote };
                steps.extend(match (direction, local_changed, remote_changed) {
                    // Both as recorded, just not alike (a time the server
                    // wouldn't take): nothing to do.
                    (_, false, false) => None,
                    (SyncDirection::Both, true, false) | (SyncDirection::Upload, _, false) => Some(Step::Upload(path)),
                    (SyncDirection::Both, false, true) | (SyncDirection::Download, false, _) => Some(Step::Download(path)),
                    (SyncDirection::Both, true, true) => Some(Step::KeepBoth { path, newer }),
                    (SyncDirection::Upload, _, true) => Some(Step::MoveAside { path, target: Side::Remote }),
                    (SyncDirection::Download, true, _) => Some(Step::MoveAside { path, target: Side::Local }),
                });
            }
            (Some(Node::File { .. }), Some(Node::Dir)) | (Some(Node::Dir), Some(Node::File { .. })) => {
                skip.push(path.clone());
                steps.push(Step::Mismatch(path));
            }
            (Some(Node::Dir), Some(Node::Dir)) => {
                if rec != Some(&Rec::Dir) {
                    steps.push(Step::Record(path, Rec::Dir));
                }
            }
            (Some(&Node::File { size, mtime, .. }), None) => {
                let was_synced = matches!(rec, Some(Rec::File { .. }));
                let changed = changed(size, mtime, rec, Side::Local);
                if was_synced && delete_local && !changed {
                    steps.push(Step::DeleteLocal(path));
                } else if was_synced && direction.both() && !pair.delete_local {
                    steps.push(Step::Restore { path, to: Side::Remote });
                } else if direction.up() {
                    steps.push(Step::Upload(path));
                } else if was_synced && changed {
                    // Download only, changed here: no longer the server's.
                    steps.push(Step::Forget(path));
                }
            }
            (None, Some(&Node::File { size, mtime, .. })) => {
                let was_synced = matches!(rec, Some(Rec::File { .. }));
                let changed = changed(size, mtime, rec, Side::Remote);
                if was_synced && delete_remote && !changed {
                    steps.push(Step::DeleteRemote(path));
                } else if was_synced && direction.both() && !pair.delete_remote {
                    steps.push(Step::Restore { path, to: Side::Local });
                } else if direction.down() {
                    steps.push(Step::Download(path));
                } else if was_synced && changed {
                    steps.push(Step::Forget(path));
                }
            }
            (Some(Node::Dir), None) => {
                if rec == Some(&Rec::Dir) && delete_local {
                    steps.push(Step::RmdirLocal(path));
                } else if direction.up() {
                    steps.push(Step::MkdirRemote(path));
                }
            }
            (None, Some(Node::Dir)) => {
                if rec == Some(&Rec::Dir) && delete_remote {
                    steps.push(Step::RmdirRemote(path));
                } else if direction.down() {
                    steps.push(Step::MkdirLocal(path));
                }
            }
            (None, None) => steps.push(Step::Forget(path)),
        }
    }
    steps.sort_by_key(Step::order);
    steps
}

/// `notes (conflict).txt`, `notes (conflict 2).txt`, ... for `dir/notes.txt`.
fn conflict_names(path: &str) -> impl Iterator<Item = String> + '_ {
    let (dir, name) = match path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, path),
    };
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    (1..).map(move |n| {
        let name = if n == 1 { format!("{stem} (conflict){ext}") } else { format!("{stem} (conflict {n}){ext}") };
        match dir {
            Some(dir) => format!("{dir}/{name}"),
            None => name,
        }
    })
}

fn local_hash(path: &Path) -> Option<[u8; 32]> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return Some(hasher.finalize().into()),
            Ok(n) => hasher.update(&buf[..n]),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// Conflicts between files of the same size whose content is the same are
/// none: recorded as synced instead.
fn drop_false_conflicts(steps: &mut [Step], dirs: (&Path, &str), scans: (&Scan, &Scan), channels: &dyn Channels) {
    let same_size = |path: &str| match (scans.0.nodes.get(path), scans.1.nodes.get(path)) {
        (Some(&Node::File { size: l, mtime: l_time, .. }), Some(&Node::File { size: r, mtime: r_time, .. })) if l == r => {
            Some(Rec::File { size: l, local: l_time, remote: r_time })
        }
        _ => None,
    };
    let candidates: Vec<(usize, String, Rec)> = steps
        .iter()
        .enumerate()
        .filter_map(|(i, step)| match step {
            Step::KeepBoth { path, .. } | Step::MoveAside { path, .. } => Some((i, path.clone(), same_size(path)?)),
            _ => None,
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    let input: String = candidates.iter().map(|(_, path, _)| format!("{}\n", join(dirs.1, path))).collect();
    let hashes = match channels.exec(HASHES_COMMAND, input.as_bytes()) {
        Ok(output) => parse_hashes(&output),
        Err(err) => return log::info!("sync: no hashes from the server: {err}"),
    };
    if hashes.len() != candidates.len() {
        return log::info!("sync: {} hashes for {} files", hashes.len(), candidates.len());
    }
    for ((i, path, rec), hash) in candidates.into_iter().zip(hashes) {
        if let ServerHash::Hash(hash) = hash
            && local_hash(&dirs.0.join(&path)) == Some(hash)
        {
            steps[i] = Step::Record(path, rec);
        }
    }
}

/// `path` and the folders above it on the server, as far as missing.
fn mkdir_all(client: &mut SftpClient, path: &str) -> protocol::Result<()> {
    let mut missing = Vec::new();
    let mut at = path.to_string();
    loop {
        match client.lstat(&at) {
            Ok(_) => break,
            Err(err) if err.not_found() && at != "/" => {
                missing.push(at.clone());
                at = parent(&at);
            }
            Err(err) => return Err(err),
        }
    }
    for dir in missing.into_iter().rev() {
        client.mkdir(&dir, Attrs::mode(0o755))?;
    }
    Ok(())
}

fn set_mtime(path: &Path, mtime: u64) -> io::Result<()> {
    File::options().write(true).open(path)?.set_modified(UNIX_EPOCH + Duration::from_secs(mtime))
}

/// What a pair is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncState {
    /// Not synced yet: waits for the connection.
    Waiting,
    Scanning,
    /// Files copied so far, of all.
    Copying { done: usize, total: usize },
    Done { at: SystemTime, copied: usize, deleted: usize },
    /// Syncing in another tab or Terminaal right now.
    Busy,
    Stopped,
    Failed(String),
}

/// A pair as the files tab shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncStatus {
    /// Its place in the list the session was given.
    pub index: usize,
    pub local: String,
    pub remote: String,
    pub direction: SyncDirection,
    pub live: bool,
    pub state: SyncState,
    /// Conflicts and errors of the last run.
    pub notes: Vec<String>,
}

impl SyncStatus {
    pub fn running(&self) -> bool {
        matches!(self.state, SyncState::Scanning | SyncState::Copying { .. })
    }
}

/// A copy a sync needs; the session queues it with its transfers.
pub enum SyncTransfer {
    /// Into the hidden `temp` next to the target, renamed over it once done.
    Upload { local: PathBuf, temp: String, size: u64, link: SyncLink },
    Download { remote: String, local: PathBuf, size: u64, link: SyncLink },
}

/// Which pair a transfer belongs to, and what's left once it's through.
#[derive(Clone, Debug)]
pub struct SyncLink {
    pub pair: usize,
    generation: u64,
    path: String,
    finish: Finish,
}

#[derive(Clone, Debug)]
enum Finish {
    Upload { temp: String, target: String, mode: u32 },
    /// `mode` only for a file that's new here.
    Download { local: PathBuf, mode: Option<u32> },
}

struct Pair {
    config: SyncPair,
    state: SyncState,
    notes: Vec<String>,
    /// Sync once this has come.
    due: Option<Instant>,
    /// The first local change since the last sync (live pairs).
    first_change: Option<Instant>,
    /// Asked for while running: once more afterwards.
    again: bool,
    run: Option<Run>,
}

/// A sync under way: scanned, its copies queued.
struct Run {
    /// The `flock` on the record's lock file.
    _lock: File,
    record: Record,
    record_path: PathBuf,
    header: String,
    pending: usize,
    total: usize,
    copied: usize,
    deleted: usize,
    saved: Instant,
    dirty: bool,
}

impl Run {
    fn save(&mut self) {
        if !self.dirty {
            return;
        }
        if let Err(err) = crate::ssh::write_atomically(&self.record_path, &self.record.to_text(&self.header)) {
            log::warn!("can't save sync record {}: {err}", self.record_path.display());
        }
        self.dirty = false;
        self.saved = Instant::now();
    }
}

/// Exclusive lock for a pair's runs; `None`: someone else has it.
fn lock(path: &Path) -> io::Result<Option<File>> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = File::options().create(true).truncate(false).write(true).open(path)?;
    // SAFETY: a plain syscall on our descriptor.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let err = io::Error::last_os_error();
        return if err.kind() == io::ErrorKind::WouldBlock { Ok(None) } else { Err(err) };
    }
    Ok(Some(file))
}

/// Names a pair's record: login and both folders.
fn record_key(login: &str, local: &Path, remote: &str) -> String {
    let digest = Sha256::new()
        .chain_update(login.as_bytes())
        .chain_update([0])
        .chain_update(local.as_os_str().as_encoded_bytes())
        .chain_update([0])
        .chain_update(remote.as_bytes())
        .finalize();
    digest.iter().take(10).map(|byte| format!("{byte:02x}")).collect()
}

/// The pairs of one session, and what they're doing.
pub struct Syncs {
    /// `user@host`, part of each record's name.
    login: String,
    pairs: Vec<Pair>,
    /// Counts [`Syncs::set`]s: transfers of an older set are ignored.
    generation: u64,
    /// Where records go; `None`: nowhere (no home), syncs fail.
    dir: Option<PathBuf>,
    tx: Sender<Command>,
    watcher: Option<Watcher>,
    /// Runs that ended so far: the files tab rereads its local side.
    pub finished: u64,
}

impl Syncs {
    pub fn new(dir: Option<PathBuf>, tx: Sender<Command>) -> Self {
        Self { login: String::new(), pairs: Vec::new(), generation: 0, dir, tx, watcher: None, finished: 0 }
    }

    /// Sync these pairs from now on. The session dropped the transfers of
    /// the old ones first.
    pub fn set(&mut self, login: String, pairs: Vec<SyncPair>) {
        self.close();
        self.generation += 1;
        self.login = login;
        self.watcher = None;
        self.pairs = pairs
            .into_iter()
            .map(|config| Pair {
                config,
                state: SyncState::Waiting,
                notes: Vec::new(),
                due: None,
                first_change: None,
                again: false,
                run: None,
            })
            .collect();
    }

    /// Sync this pair (`None`: all) as soon as possible.
    pub fn request(&mut self, which: Option<usize>) {
        for (i, pair) in self.pairs.iter_mut().enumerate() {
            if which.is_some_and(|which| which != i) {
                continue;
            }
            if pair.run.is_some() {
                pair.again = true;
            } else {
                pair.due = Some(Instant::now());
            }
        }
    }

    /// The watcher saw `path` change: its live pair syncs a moment later.
    pub fn local_changed(&mut self, path: &Path) {
        for pair in self.pairs.iter_mut().filter(|pair| pair.config.live) {
            let Ok(rel) = path.strip_prefix(pair.config.local_path()) else { continue };
            let Some(rel) = rel.to_str() else { continue };
            let mut prefix = String::new();
            let left_out = rel.split('/').any(|name| {
                prefix = child(&prefix, name);
                excluded(&prefix, name, &pair.config.exclude)
            });
            if left_out {
                continue;
            }
            if pair.run.is_some() {
                pair.again = true;
                continue;
            }
            let now = Instant::now();
            let first = *pair.first_change.get_or_insert(now);
            pair.due = Some((now + LIVE_SETTLE).min(first + LIVE_MAX_DELAY));
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.pairs.iter().filter(|pair| pair.run.is_none()).filter_map(|pair| pair.due).min()
    }

    /// A pair whose time has come and that isn't running.
    pub fn next_due(&self) -> Option<usize> {
        let now = Instant::now();
        self.pairs.iter().position(|pair| pair.run.is_none() && pair.due.is_some_and(|due| due <= now))
    }

    /// About to scan pair `index`.
    pub fn scanning(&mut self, index: usize) {
        let pair = &mut self.pairs[index];
        pair.state = SyncState::Scanning;
        pair.notes.clear();
        pair.due = None;
        pair.first_change = None;
        pair.again = false;
    }

    /// Scan both sides of pair `index`, do what's quick (records, deletions,
    /// folders, conflict renames) and hand back the copies to queue. A
    /// broken connection is [`Fatal`]; anything else ends up in the pair's
    /// state.
    pub fn run(
        &mut self,
        index: usize,
        client: &mut SftpClient,
        channels: &dyn Channels,
        home: &str,
    ) -> Result<Vec<SyncTransfer>, Fatal> {
        let config = self.pairs[index].config.clone();
        match self.start(index, &config, client, channels, home) {
            Ok(Ok(transfers)) => Ok(transfers),
            Ok(Err(err)) => {
                let pair = &mut self.pairs[index];
                pair.run = None;
                pair.state = SyncState::Failed(err);
                Ok(Vec::new())
            }
            Err(fatal) => {
                // Tried again with the next connection.
                let pair = &mut self.pairs[index];
                pair.run = None;
                pair.state = SyncState::Waiting;
                pair.due = Some(Instant::now());
                Err(fatal)
            }
        }
    }

    fn start(
        &mut self,
        index: usize,
        config: &SyncPair,
        client: &mut SftpClient,
        channels: &dyn Channels,
        home: &str,
    ) -> Result<Result<Vec<SyncTransfer>, String>, Fatal> {
        let fatal = |err: protocol::Error| if err.is_fatal() { Err(Fatal(err.to_string())) } else { Ok(err) };
        if let Err(err) = config.check() {
            return Ok(Err(err));
        }
        let Some(dir) = self.dir.clone() else { return Ok(Err(t!("common-home-unset"))) };
        let (local, remote) = (config.local_path(), config.remote_path(home));
        let key = record_key(&self.login, &local, &remote);
        let lock = match lock(&dir.join(format!("{key}.lock"))) {
            Ok(Some(lock)) => lock,
            Ok(None) => {
                let pair = &mut self.pairs[index];
                pair.state = SyncState::Busy;
                pair.due = Some(Instant::now() + BUSY_RETRY);
                return Ok(Ok(Vec::new()));
            }
            Err(err) => return Ok(Err(t!("sync-record-failed", err = err.to_string()))),
        };
        let record_path = dir.join(format!("{key}.tsv"));
        let record = match std::fs::read_to_string(&record_path) {
            Ok(text) => Record::parse(&text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Record::default(),
            Err(err) => return Ok(Err(t!("sync-record-failed", err = err.to_string()))),
        };
        // A missing folder is only made for a pair that never synced: with
        // a record, it may just be an unmounted disk.
        let fresh = record.entries.is_empty();
        let shown = crate::ssh::display_path(&local);
        match std::fs::metadata(&local) {
            Ok(meta) if meta.is_dir() => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound && fresh => {
                if let Err(err) = std::fs::create_dir_all(&local) {
                    return Ok(Err(t!("sync-local-failed", path = &shown, err = err.to_string())));
                }
            }
            Ok(_) => return Ok(Err(t!("sync-not-a-folder", path = &shown))),
            Err(err) => return Ok(Err(t!("sync-local-failed", path = &shown, err = err.to_string()))),
        }
        match client.stat(&remote) {
            Ok(attrs) if attrs.is_dir() => {}
            Ok(_) => return Ok(Err(t!("sync-not-a-folder", path = &remote))),
            Err(err) if err.not_found() && fresh => {
                if let Err(err) = mkdir_all(client, &remote) {
                    return Ok(Err(t!("sync-remote-failed", path = &remote, err = fatal(err)?.to_string())));
                }
            }
            Err(err) => return Ok(Err(t!("sync-remote-failed", path = &remote, err = fatal(err)?.to_string()))),
        }
        let local_scan = match scan_local(&local, &config.exclude) {
            Ok(scan) => scan,
            Err(err) => return Ok(Err(t!("sync-local-failed", path = &shown, err = err.to_string()))),
        };
        let remote_scan = match scan_remote(client, &remote, &config.exclude) {
            Ok(scan) => scan,
            Err(err) => return Ok(Err(t!("sync-remote-failed", path = &remote, err = fatal(err)?.to_string()))),
        };
        if config.live {
            self.watch(&local, &local_scan);
        }
        let mut steps = plan(config, &local_scan, &remote_scan, &record);
        drop_false_conflicts(&mut steps, (&local, &remote), (&local_scan, &remote_scan), channels);

        let header = t!("sync-record-header", login = &self.login, local = &shown, remote = &remote);
        let mut run = Run {
            _lock: lock,
            record,
            record_path,
            header,
            pending: 0,
            total: 0,
            copied: 0,
            deleted: 0,
            saved: Instant::now(),
            dirty: false,
        };
        let mut notes: Vec<String> = local_scan.errors.iter().chain(&remote_scan.errors).cloned().collect();
        let mut exec = Exec {
            client,
            local: &local,
            remote: &remote,
            scans: (&local_scan, &remote_scan),
            run: &mut run,
            notes: &mut notes,
            transfers: Vec::new(),
            made: HashSet::new(),
            link: (index, self.generation),
            restored: (0, 0),
        };
        let mut broken = None;
        for step in steps {
            if let Err(fatal) = exec.step(step) {
                broken = Some(fatal);
                break;
            }
        }
        let (transfers, restored) = (exec.transfers, exec.restored);
        if restored.0 > 0 {
            notes.push(t!("sync-restored-local", count = restored.0));
        }
        if restored.1 > 0 {
            notes.push(t!("sync-restored-remote", count = restored.1));
        }
        if let Some(fatal) = broken {
            // Keep what was done; the next connection syncs again.
            run.save();
            return Err(fatal);
        }
        let pair = &mut self.pairs[index];
        pair.notes = notes;
        run.pending = transfers.len();
        run.total = transfers.len();
        pair.run = Some(run);
        if transfers.is_empty() {
            self.end(index);
        } else {
            self.pairs[index].state = SyncState::Copying { done: 0, total: transfers.len() };
        }
        Ok(Ok(transfers))
    }

    fn watch(&mut self, root: &Path, scan: &Scan) {
        if self.watcher.is_none() {
            let tx = self.tx.clone();
            match Watcher::with("sync-watch", WATCH_MASK, move |path| tx.send(Command::SyncLocalChanged(path)).is_ok()) {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(err) => return log::warn!("can't watch synced folders: {err}"),
            }
        }
        let Some(watcher) = &self.watcher else { return };
        let dirs = scan.nodes.iter().filter(|(_, node)| **node == Node::Dir).map(|(path, _)| root.join(path));
        for dir in std::iter::once(root.to_path_buf()).chain(dirs) {
            if let Err(err) = watcher.add(&dir) {
                // Most likely out of inotify watches: the rest won't go either.
                return log::warn!("can't watch {}: {err}", dir.display());
            }
        }
    }

    /// A copy of a pair is through (or failed: `outcome`). `source`: size
    /// and time of what was copied, as it started. `true` once that was
    /// the pair's last copy.
    pub fn finished(
        &mut self,
        link: &SyncLink,
        source: (u64, Option<u64>),
        outcome: Result<(), String>,
        client: Option<&mut SftpClient>,
    ) -> Result<bool, Fatal> {
        if link.generation != self.generation {
            return Ok(false);
        }
        let Some(pair) = self.pairs.get_mut(link.pair) else { return Ok(false) };
        let Some(run) = pair.run.as_mut() else { return Ok(false) };
        let (size, mtime) = source;
        let outcome = match (outcome, &link.finish, client) {
            (Err(err), ..) => Err(err),
            (Ok(()), Finish::Upload { temp, target, mode }, Some(client)) => {
                let time = mtime.and_then(|mtime| u32::try_from(mtime).ok());
                let attrs = Attrs { permissions: Some(*mode), atime_mtime: time.map(|time| (time, time)), ..Attrs::default() };
                match client.setstat(temp, attrs) {
                    Err(err) if err.is_fatal() => return Err(err.into()),
                    Err(err) => log::info!("sync: can't set the time of {target}: {err}"),
                    Ok(()) => {}
                }
                let renamed = if client.has_extension("posix-rename@openssh.com") {
                    client.posix_rename(temp, target)
                } else {
                    // Plain SFTP renames only onto a free name.
                    match client.remove(target) {
                        Err(err) if !err.not_found() => Err(err),
                        _ => client.rename(temp, target),
                    }
                };
                match renamed {
                    Ok(()) => {
                        let remote = client.lstat(target).ok().and_then(|attrs| attrs.mtime()).map(u64::from);
                        run.record.entries.insert(link.path.clone(), Rec::File { size, local: mtime, remote });
                        Ok(())
                    }
                    Err(err) if err.is_fatal() => return Err(err.into()),
                    Err(err) => {
                        let _ = client.remove(temp);
                        Err(err.to_string())
                    }
                }
            }
            (Ok(()), Finish::Download { local, mode }, _) => {
                if let Some(mode) = mode
                    && let Err(err) = std::fs::set_permissions(local, std::fs::Permissions::from_mode(*mode & 0o777))
                {
                    log::info!("sync: can't set the mode of {}: {err}", local.display());
                }
                if let Some(mtime) = mtime
                    && let Err(err) = set_mtime(local, mtime)
                {
                    log::info!("sync: can't set the time of {}: {err}", local.display());
                }
                let local_time = secs(std::fs::metadata(local).and_then(|meta| meta.modified()));
                run.record.entries.insert(link.path.clone(), Rec::File { size, local: local_time, remote: mtime });
                Ok(())
            }
            (Ok(()), Finish::Upload { .. }, None) => Err(t!("files-offline")),
        };
        match outcome {
            Ok(()) => {
                run.copied += 1;
                run.dirty = true;
            }
            Err(err) => pair.notes.push(t!("sync-copy-failed", path = &link.path, err = err)),
        }
        run.pending -= 1;
        if run.pending == 0 {
            self.end(link.pair);
            return Ok(true);
        }
        pair.state = SyncState::Copying { done: run.total - run.pending, total: run.total };
        if run.saved.elapsed() >= SAVE_EVERY {
            run.save();
        }
        Ok(false)
    }

    fn end(&mut self, index: usize) {
        let pair = &mut self.pairs[index];
        let Some(mut run) = pair.run.take() else { return };
        run.save();
        pair.state = SyncState::Done { at: SystemTime::now(), copied: run.copied, deleted: run.deleted };
        if std::mem::take(&mut pair.again) {
            pair.due = Some(Instant::now());
        }
        self.finished += 1;
    }

    /// Stop pair `index`; the session dropped its copies first.
    pub fn stop(&mut self, index: usize) {
        let Some(pair) = self.pairs.get_mut(index) else { return };
        if let Some(mut run) = pair.run.take() {
            run.save();
            pair.state = SyncState::Stopped;
            self.finished += 1;
        }
        pair.due = None;
        pair.again = false;
    }

    /// Save what's been synced: the session ends.
    pub fn close(&mut self) {
        for pair in &mut self.pairs {
            if let Some(mut run) = pair.run.take() {
                run.save();
            }
        }
    }

    pub fn statuses(&self) -> Vec<SyncStatus> {
        self.pairs
            .iter()
            .enumerate()
            .map(|(index, pair)| SyncStatus {
                index,
                local: crate::ssh::display_path(&pair.config.local_path()),
                remote: pair.config.remote.trim().to_string(),
                direction: pair.config.direction,
                live: pair.config.live,
                state: pair.state.clone(),
                notes: pair.notes.clone(),
            })
            .collect()
    }
}

/// Carries out the steps of one run.
struct Exec<'a> {
    client: &'a mut SftpClient,
    local: &'a Path,
    remote: &'a str,
    scans: (&'a Scan, &'a Scan),
    run: &'a mut Run,
    notes: &'a mut Vec<String>,
    transfers: Vec<SyncTransfer>,
    /// Server folders known to be there.
    made: HashSet<String>,
    /// Pair index and generation for the transfers' links.
    link: (usize, u64),
    /// Files deleted here, and on the server, that are copied back.
    restored: (usize, usize),
}

impl Exec<'_> {
    /// A server error that isn't the connection's, as text.
    fn soft(err: protocol::Error) -> Result<String, Fatal> {
        if err.is_fatal() { Err(err.into()) } else { Ok(err.to_string()) }
    }

    fn note(&mut self, path: &str, err: String) {
        self.notes.push(t!("sync-copy-failed", path = path, err = err));
    }

    fn forget(&mut self, path: &str) {
        self.run.record.entries.remove(path);
        self.run.dirty = true;
    }

    fn step(&mut self, step: Step) -> Result<(), Fatal> {
        match step {
            Step::Record(path, rec) => {
                self.run.record.entries.insert(path, rec);
                self.run.dirty = true;
            }
            Step::Forget(path) => self.forget(&path),
            Step::Mismatch(path) => self.notes.push(t!("sync-type-differs", path = &path)),
            Step::DeleteLocal(path) => match std::fs::remove_file(self.local.join(&path)) {
                Ok(()) => {
                    self.forget(&path);
                    self.run.deleted += 1;
                }
                Err(err) => self.note(&path, err.to_string()),
            },
            Step::DeleteRemote(path) => match self.client.remove(&join(self.remote, &path)) {
                Ok(()) => {
                    self.forget(&path);
                    self.run.deleted += 1;
                }
                Err(err) => {
                    let err = Self::soft(err)?;
                    self.note(&path, err);
                }
            },
            // Not empty (something new, or left out): stays.
            Step::RmdirLocal(path) => {
                if std::fs::remove_dir(self.local.join(&path)).is_ok() {
                    self.forget(&path);
                }
            }
            Step::RmdirRemote(path) => match self.client.rmdir(&join(self.remote, &path)) {
                Ok(()) => self.forget(&path),
                Err(err) => drop(Self::soft(err)?),
            },
            Step::MkdirLocal(path) => match std::fs::create_dir_all(self.local.join(&path)) {
                Ok(()) => {
                    self.run.record.entries.insert(path, Rec::Dir);
                    self.run.dirty = true;
                }
                Err(err) => self.note(&path, err.to_string()),
            },
            Step::MkdirRemote(path) => match self.remote_dir(&path)? {
                Ok(()) => {
                    self.run.record.entries.insert(path, Rec::Dir);
                    self.run.dirty = true;
                }
                Err(err) => self.note(&path, err),
            },
            Step::KeepBoth { path, newer } => {
                let Some(copy) = self.move_aside(&path, if newer == Side::Local { Side::Remote } else { Side::Local })? else {
                    return Ok(());
                };
                self.notes.push(t!("sync-conflict-both", path = &path, copy = &copy));
                // The newer one takes the name, the older one its copy over.
                match newer {
                    Side::Local => {
                        self.upload(&path, &path)?;
                        self.download(&copy, &path)?;
                    }
                    Side::Remote => {
                        self.download(&path, &path)?;
                        self.upload(&copy, &path)?;
                    }
                }
            }
            Step::MoveAside { path, target } => {
                let Some(copy) = self.move_aside(&path, target)? else { return Ok(()) };
                let side = match target {
                    Side::Local => t!("sync-side-local"),
                    Side::Remote => t!("sync-side-remote"),
                };
                self.notes.push(t!("sync-conflict-target", path = &path, copy = &copy, side = side));
                match target {
                    Side::Remote => self.upload(&path, &path)?,
                    Side::Local => self.download(&path, &path)?,
                }
            }
            Step::Upload(path) => self.upload(&path, &path)?,
            Step::Download(path) => self.download(&path, &path)?,
            Step::Restore { path, to: Side::Remote } => {
                self.restored.1 += 1;
                self.upload(&path, &path)?;
            }
            Step::Restore { path, to: Side::Local } => {
                self.restored.0 += 1;
                self.download(&path, &path)?;
            }
        }
        Ok(())
    }

    /// The server folder `path` (relative), made if missing.
    fn remote_dir(&mut self, path: &str) -> Result<Result<(), String>, Fatal> {
        if path.is_empty() || self.made.contains(path) || self.scans.1.nodes.get(path) == Some(&Node::Dir) {
            return Ok(Ok(()));
        }
        match mkdir_all(self.client, &join(self.remote, path)) {
            Ok(()) => {
                self.made.insert(path.to_string());
                Ok(Ok(()))
            }
            Err(err) => Ok(Err(Self::soft(err)?)),
        }
    }

    /// Rename `side`'s copy of `path` to a conflict name free on both sides.
    fn move_aside(&mut self, path: &str, side: Side) -> Result<Option<String>, Fatal> {
        let mut names = conflict_names(path);
        let copy = loop {
            let name = names.next().expect("conflict names never run out");
            let taken = self.scans.0.nodes.contains_key(&name)
                || self.scans.1.nodes.contains_key(&name)
                || std::fs::symlink_metadata(self.local.join(&name)).is_ok();
            if taken {
                continue;
            }
            match self.client.lstat(&join(self.remote, &name)) {
                Err(err) if err.not_found() => break name,
                Err(err) if err.is_fatal() => return Err(err.into()),
                _ => {}
            }
        };
        let moved = match side {
            Side::Local => std::fs::rename(self.local.join(path), self.local.join(&copy)).map_err(|err| err.to_string()),
            Side::Remote => match self.client.rename(&join(self.remote, path), &join(self.remote, &copy)) {
                Ok(()) => Ok(()),
                Err(err) => Err(Self::soft(err)?),
            },
        };
        match moved {
            Ok(()) => {
                self.forget(path);
                Ok(Some(copy))
            }
            Err(err) => {
                self.note(path, err);
                Ok(None)
            }
        }
    }

    fn link(&self, path: &str, finish: Finish) -> SyncLink {
        SyncLink { pair: self.link.0, generation: self.link.1, path: path.to_string(), finish }
    }

    /// The local file `path` to the server as `to`; `node` is what was
    /// scanned at `node_of` (the name before a conflict rename).
    fn upload(&mut self, to: &str, node_of: &str) -> Result<(), Fatal> {
        let Some(&Node::File { size, mode, .. }) = self.scans.0.nodes.get(node_of) else { return Ok(()) };
        let dir = match to.rsplit_once('/') {
            Some((dir, _)) => dir,
            None => "",
        };
        if let Err(err) = self.remote_dir(dir)? {
            self.note(to, err);
            return Ok(());
        }
        let target = join(self.remote, to);
        // A file there keeps its mode; a new one gets ours.
        let mode = match self.scans.1.nodes.get(to) {
            Some(&Node::File { mode, .. }) => mode,
            _ => mode,
        };
        let temp = join(&parent(&target), &format!(".{}.part", file_name(&target)));
        // Left over from an interrupted sync: the transfer creates it anew.
        if let Err(err) = self.client.remove(&temp)
            && err.is_fatal()
        {
            return Err(err.into());
        }
        let local = self.local.join(to);
        let link = self.link(to, Finish::Upload { temp: temp.clone(), target, mode });
        self.transfers.push(SyncTransfer::Upload { local, temp, size, link });
        Ok(())
    }

    /// The server's file `to` here; `node_of` as for [`Exec::upload`].
    fn download(&mut self, to: &str, node_of: &str) -> Result<(), Fatal> {
        let Some(&Node::File { size, mode, .. }) = self.scans.1.nodes.get(node_of) else { return Ok(()) };
        let local = self.local.join(to);
        if let Some(dir) = local.parent()
            && let Err(err) = std::fs::create_dir_all(dir)
        {
            self.note(to, err.to_string());
            return Ok(());
        }
        let new = !matches!(self.scans.0.nodes.get(to), Some(Node::File { .. }));
        let link = self.link(to, Finish::Download { local: local.clone(), mode: new.then_some(mode) });
        self.transfers.push(SyncTransfer::Download { remote: join(self.remote, to), local, size, link });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::tests::scratch;
    use super::super::session::tests::{Session, session, wait_for};
    use super::super::session::{Connection, Remote, State};
    use super::*;

    /// Write `content` with modification time `secs`.
    fn put(path: &Path, content: &str, secs: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        File::options().write(true).open(path).unwrap().set_modified(UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
    }

    fn mtime(path: &Path) -> u64 {
        secs(std::fs::metadata(path).and_then(|meta| meta.modified())).unwrap()
    }

    /// Ask for a sync and wait until it's over; its state.
    fn sync(remote: &Remote, what: &str) -> State {
        let before = remote.state().syncs_finished;
        remote.send(Command::SyncNow(None));
        wait_for(remote, what, |s| s.syncs_finished > before && s.syncs.iter().all(|sync| !sync.running()))
    }

    fn pair_of(local: &Path, remote: &Path) -> SyncPair {
        SyncPair { local: local.to_str().unwrap().into(), remote: remote.to_str().unwrap().into(), ..SyncPair::default() }
    }

    fn start(name: &str) -> Option<(Remote, PathBuf, PathBuf, PathBuf)> {
        let Session { remote, .. } = session(name)?;
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch(&format!("{name}-dirs"));
        let (here, there) = (root.join("here"), root.join("there"));
        std::fs::create_dir_all(&here).unwrap();
        std::fs::create_dir_all(&there).unwrap();
        Some((remote, root, here, there))
    }

    #[test]
    fn two_way_sync_copies_both_ways_keeps_conflicts_and_deletes_only_when_asked() {
        let Some((remote, root, here, there)) = start("sync-two-way") else { return };
        put(&here.join("a.txt"), "a from here", 1_700_000_000);
        put(&here.join("sub/b.txt"), "b", 1_700_000_000);
        put(&there.join("c.txt"), "c from there", 1_700_000_000);
        // Same content, other times: no conflict, thanks to the hash.
        put(&here.join("same.txt"), "same", 1_700_000_000);
        put(&there.join("same.txt"), "same", 1_700_000_500);
        std::fs::write(here.join(".x.txt.part"), "left out").unwrap();

        let before = remote.state().syncs_finished;
        remote.send(Command::SetSyncs { login: "test".into(), pairs: vec![pair_of(&here, &there)] });
        let state = wait_for(&remote, "first sync", |s| s.syncs_finished > before && !s.syncs[0].running());
        assert!(matches!(state.syncs[0].state, SyncState::Done { copied: 3, deleted: 0, .. }), "{:?}", state.syncs);
        assert_eq!(read(&there.join("a.txt")), "a from here");
        assert_eq!(read(&there.join("sub/b.txt")), "b");
        assert_eq!(read(&here.join("c.txt")), "c from there");
        assert_eq!(mtime(&there.join("a.txt")), 1_700_000_000, "copies keep their time");
        assert_eq!(mtime(&here.join("c.txt")), 1_700_000_000);
        assert!(!there.join(".x.txt.part").exists());
        assert!(!here.join("same (conflict).txt").exists() && !there.join("same (conflict).txt").exists());

        // One side changed: copied over the other.
        put(&here.join("a.txt"), "a changed here", 1_700_000_100);
        put(&there.join("c.txt"), "c changed there", 1_700_000_100);
        let state = sync(&remote, "changes");
        assert!(state.syncs[0].notes.is_empty(), "{:?}", state.syncs[0].notes);
        assert_eq!(read(&there.join("a.txt")), "a changed here");
        assert_eq!(read(&here.join("c.txt")), "c changed there");

        // Both changed: the newer takes the name, the older is kept on both sides.
        put(&here.join("a.txt"), "older edit", 1_700_000_200);
        put(&there.join("a.txt"), "newer edit", 1_700_000_300);
        let state = sync(&remote, "conflict");
        assert_eq!(state.syncs[0].notes.len(), 1, "{:?}", state.syncs[0].notes);
        for side in [&here, &there] {
            assert_eq!(read(&side.join("a.txt")), "newer edit");
            assert_eq!(read(&side.join("a (conflict).txt")), "older edit");
        }

        // Deleted here without deletions: it comes back, and that's said.
        std::fs::remove_file(here.join("sub/b.txt")).unwrap();
        let state = sync(&remote, "restore");
        assert_eq!(read(&here.join("sub/b.txt")), "b");
        assert_eq!(state.syncs[0].notes.len(), 1, "{:?}", state.syncs[0].notes);
        assert!(state.syncs[0].notes[0].contains("Hier Gelöschtes auf dem Server löschen"), "{:?}", state.syncs[0].notes);

        // With them: gone on the server too -- but not a file that changed there.
        let deleting = SyncPair { delete_remote: true, ..pair_of(&here, &there) };
        let before = remote.state().syncs_finished;
        remote.send(Command::SetSyncs { login: "test".into(), pairs: vec![deleting] });
        wait_for(&remote, "set again", |s| s.syncs_finished > before && !s.syncs[0].running());
        std::fs::remove_file(here.join("sub/b.txt")).unwrap();
        std::fs::remove_file(here.join("c.txt")).unwrap();
        put(&there.join("c.txt"), "c changed there again", 1_700_000_400);
        let state = sync(&remote, "deletions");
        assert!(matches!(state.syncs[0].state, SyncState::Done { deleted: 1, .. }), "{:?}", state.syncs);
        assert!(!there.join("sub/b.txt").exists());
        assert_eq!(read(&here.join("c.txt")), "c changed there again", "a change wins over a deletion");

        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn one_way_upload_moves_a_changed_target_aside() {
        let Some((remote, root, here, there)) = start("sync-upload") else { return };
        put(&here.join("site.html"), "v1", 1_700_000_000);
        put(&there.join("theirs.log"), "server's own", 1_700_000_000);
        let upload = SyncPair { direction: SyncDirection::Upload, ..pair_of(&here, &there) };
        let before = remote.state().syncs_finished;
        remote.send(Command::SetSyncs { login: "test".into(), pairs: vec![upload] });
        wait_for(&remote, "first sync", |s| s.syncs_finished > before && !s.syncs[0].running());
        assert_eq!(read(&there.join("site.html")), "v1");
        assert!(!here.join("theirs.log").exists(), "nothing comes down");

        put(&there.join("site.html"), "hotfix on the server", 1_700_000_050);
        put(&here.join("site.html"), "v2", 1_700_000_100);
        let state = sync(&remote, "moved aside");
        assert_eq!(read(&there.join("site.html")), "v2");
        assert_eq!(read(&there.join("site (conflict).html")), "hotfix on the server");
        assert!(!here.join("site (conflict).html").exists());
        assert_eq!(state.syncs[0].notes.len(), 1, "{:?}", state.syncs[0].notes);
        assert_eq!(read(&there.join("theirs.log")), "server's own");
        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn live_pairs_push_local_changes_and_busy_pairs_wait() {
        let Some((remote, root, here, there)) = start("sync-live") else { return };
        let live = SyncPair { live: true, exclude: vec!["*.tmp".into()], ..pair_of(&here, &there) };
        let before = remote.state().syncs_finished;
        remote.send(Command::SetSyncs { login: "test".into(), pairs: vec![live] });
        wait_for(&remote, "first sync", |s| s.syncs_finished > before && !s.syncs[0].running());

        std::fs::create_dir_all(here.join("new/deeper")).unwrap();
        std::fs::write(here.join("new/deeper/note.txt"), "hello").unwrap();
        std::fs::write(here.join("scratch.tmp"), "left out").unwrap();
        let start = Instant::now();
        while !there.join("new/deeper/note.txt").exists() {
            assert!(start.elapsed() < Duration::from_secs(20), "never pushed: {:#?}", remote.state().syncs);
            std::thread::sleep(Duration::from_millis(50));
        }
        wait_for(&remote, "settled", |s| !s.syncs[0].running());
        assert!(!there.join("scratch.tmp").exists());

        // Held elsewhere: busy, nothing copied.
        // Where `session` keeps the records.
        let records = std::env::temp_dir().join(format!("terminaal-sftp-{}-sync-live-edits/sync", std::process::id()));
        let key = record_key("test", &here, there.to_str().unwrap());
        let _held = lock(&records.join(format!("{key}.lock"))).unwrap().expect("free");
        std::fs::write(here.join("while-busy.txt"), "x").unwrap();
        remote.send(Command::SyncNow(None));
        wait_for(&remote, "busy", |s| s.syncs[0].state == SyncState::Busy);
        assert!(!there.join("while-busy.txt").exists());
        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn globs_match_names_and_paths() {
        assert!(glob("*.log", "build.log"));
        assert!(!glob("*.log", "build.log.1"));
        assert!(glob("node_modules", "node_modules"));
        assert!(glob("a?c", "abc"));
        assert!(glob("*a*b*", "xxaxxbxx"));
        assert!(!glob("*a*b", "xxaxxbxx"));
        let patterns = vec!["*.tmp".to_string(), "build/out".to_string(), " ".to_string()];
        assert!(excluded("x/y.tmp", "y.tmp", &patterns));
        assert!(excluded("build/out", "out", &patterns));
        assert!(!excluded("src/out", "out", &patterns));
        assert!(excluded("x/.y.txt.part", ".y.txt.part", &[]), "our own part files");
        assert!(!excluded("x/.part", ".part", &[]));
        assert!(!excluded("x", "x", &patterns));
    }

    #[test]
    fn remote_folders_resolve_against_home() {
        let pair = |remote: &str| SyncPair { remote: remote.into(), ..SyncPair::default() };
        assert_eq!(pair("site").remote_path("/home/u"), "/home/u/site");
        assert_eq!(pair("~/site/").remote_path("/home/u"), "/home/u/site");
        assert_eq!(pair("~").remote_path("/home/u"), "/home/u");
        assert_eq!(pair("/srv/www").remote_path("/home/u"), "/srv/www");
        assert_eq!(pair("/").remote_path("/home/u"), "/");
    }

    #[test]
    fn pairs_round_trip_through_toml() {
        let pair = SyncPair {
            local: "~/site".into(),
            remote: "/srv/www".into(),
            direction: SyncDirection::Upload,
            delete_remote: true,
            delete_local: false,
            live: true,
            exclude: vec![".git".into()],
        };
        let text = toml::to_string(&pair).unwrap();
        assert!(text.contains("direction = \"upload\""), "{text}");
        assert!(!text.contains("delete_local"), "{text}");
        assert_eq!(toml::from_str::<SyncPair>(&text).unwrap(), pair);
        let minimal: SyncPair = toml::from_str("local = \"/a\"\nremote = \"b\"").unwrap();
        assert_eq!(minimal.direction, SyncDirection::Both);
        assert!(!minimal.live);
        assert!(SyncPair { local: "rel".into(), remote: "b".into(), ..minimal.clone() }.check().is_err());
        assert!(SyncPair { remote: " ".into(), ..minimal.clone() }.check().is_err());
        assert!(minimal.check().is_ok());
    }

    #[test]
    fn hashes_are_told_from_other_output() {
        let hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let output = format!("Welcome to the server!\nterminaal-hash {hash}  -\nstty: not a tty\nterminaal-hash -\n");
        let hashes = parse_hashes(output.as_bytes());
        assert_eq!(hashes.len(), 2);
        assert!(matches!(hashes[0], ServerHash::Hash(_)));
        assert_eq!(hashes[1], ServerHash::Unknown);
        // The real thing, through a shell that talks first.
        let file = std::env::temp_dir().join(format!("terminaal-hashes-{}", std::process::id()));
        std::fs::write(&file, "").unwrap();
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("echo hello from bashrc; {HASHES_COMMAND}"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(format!("{}\n/nonexistent\n", file.display()).as_bytes())?;
                child.wait_with_output()
            })
            .unwrap();
        let hashes = parse_hashes(&output.stdout);
        assert_eq!(hashes, [ServerHash::Hash(local_hash(&file).unwrap()), ServerHash::Unknown], "{output:?}");
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn records_round_trip() {
        let mut record = Record::default();
        record.entries.insert("a b/c\td.txt".into(), Rec::File { size: 12, local: Some(5), remote: None });
        record.entries.insert("a b".into(), Rec::Dir);
        let text = record.to_text("user@host ~/x /srv/x");
        assert!(text.starts_with("# user@host"));
        assert_eq!(Record::parse(&text), record);
        assert_eq!(Record::parse("garbage\nf\tx\t1\t2\tp\nd\t\n").entries.len(), 0);
    }

    #[test]
    fn conflict_names_go_before_the_extension() {
        let names: Vec<String> = conflict_names("dir/notes.txt").take(2).collect();
        assert_eq!(names, ["dir/notes (conflict).txt", "dir/notes (conflict 2).txt"]);
        assert_eq!(conflict_names(".bashrc").next().unwrap(), ".bashrc (conflict)");
    }

    fn file(size: u64, mtime: u64) -> Node {
        Node::File { size, mtime: Some(mtime), mode: 0o644 }
    }

    fn scan(nodes: &[(&str, Node)]) -> Scan {
        Scan { nodes: nodes.iter().map(|(path, node)| (path.to_string(), *node)).collect(), ..Scan::default() }
    }

    fn synced(size: u64, mtime: u64) -> Rec {
        Rec::File { size, local: Some(mtime), remote: Some(mtime) }
    }

    fn record(entries: &[(&str, Rec)]) -> Record {
        Record { entries: entries.iter().map(|(path, rec)| (path.to_string(), *rec)).collect() }
    }

    fn pair(direction: SyncDirection) -> SyncPair {
        SyncPair { local: "/l".into(), remote: "r".into(), direction, ..SyncPair::default() }
    }

    #[test]
    fn two_way_copies_what_changed_and_keeps_both_on_conflict() {
        let local = scan(&[("same", file(1, 10)), ("mine", file(2, 20)), ("theirs", file(3, 10)), ("both", file(4, 30)), ("new-here", file(1, 1)), ("d", Node::Dir)]);
        let remote = scan(&[("same", file(1, 10)), ("mine", file(3, 10)), ("theirs", file(9, 40)), ("both", file(5, 31)), ("new-there", file(1, 1)), ("e", Node::Dir)]);
        let rec = record(&[("same", synced(1, 10)), ("mine", synced(3, 10)), ("theirs", synced(3, 10)), ("both", synced(1, 1)), ("gone", synced(1, 1))]);
        let steps = plan(&pair(SyncDirection::Both), &local, &remote, &rec);
        assert_eq!(
            steps,
            [
                Step::Forget("gone".into()),
                Step::MkdirRemote("d".into()),
                Step::MkdirLocal("e".into()),
                Step::KeepBoth { path: "both".into(), newer: Side::Remote },
                Step::Upload("mine".into()),
                Step::Upload("new-here".into()),
                Step::Download("new-there".into()),
                Step::Download("theirs".into()),
            ]
        );
    }

    #[test]
    fn deletions_travel_only_where_allowed() {
        // `kept` was synced and deleted on the server; `edited` too, but
        // changed here since.
        let local = scan(&[("kept", file(1, 1)), ("edited", file(2, 5)), ("dir", Node::Dir)]);
        let remote = scan(&[("other", file(1, 1))]);
        let rec = record(&[("kept", synced(1, 1)), ("edited", synced(1, 1)), ("dir", Rec::Dir), ("other", synced(1, 1))]);
        let no = plan(&pair(SyncDirection::Both), &local, &remote, &rec);
        assert!(no.contains(&Step::Restore { path: "kept".into(), to: Side::Remote }), "{no:?}");
        assert!(no.contains(&Step::Restore { path: "other".into(), to: Side::Local }), "{no:?}");
        assert!(no.contains(&Step::MkdirRemote("dir".into())));

        let yes = plan(&SyncPair { delete_local: true, ..pair(SyncDirection::Both) }, &local, &remote, &rec);
        assert!(yes.contains(&Step::DeleteLocal("kept".into())), "{yes:?}");
        assert!(yes.contains(&Step::Upload("edited".into())), "a change wins over a deletion: {yes:?}");
        assert!(yes.contains(&Step::RmdirLocal("dir".into())));

        // Upload only: deleting here never deletes there unless asked.
        let up = plan(&SyncPair { delete_local: true, ..pair(SyncDirection::Upload) }, &local, &remote, &rec);
        assert!(up.contains(&Step::Upload("kept".into())), "{up:?}");
        let local_gone = scan(&[("x", file(1, 1))]);
        let up = plan(&SyncPair { delete_remote: true, ..pair(SyncDirection::Upload) }, &local_gone, &remote, &rec);
        assert!(up.contains(&Step::DeleteRemote("other".into())), "{up:?}");
        let up = plan(&pair(SyncDirection::Upload), &local_gone, &remote, &rec);
        assert!(!up.iter().any(|step| matches!(step, Step::DeleteRemote(_))), "{up:?}");

        // An empty side never deletes the other.
        let empty = plan(&SyncPair { delete_remote: true, ..pair(SyncDirection::Both) }, &Scan::default(), &remote, &rec);
        assert!(empty.contains(&Step::Download("other".into())), "{empty:?}");
    }

    #[test]
    fn one_way_moves_a_changed_target_aside_and_leaves_foreign_files() {
        let local = scan(&[("a", file(1, 5)), ("b", file(2, 6))]);
        let remote = scan(&[("a", file(1, 1)), ("b", file(3, 9)), ("theirs", file(1, 1))]);
        let rec = record(&[("a", synced(1, 1)), ("b", synced(2, 2))]);
        let up = plan(&pair(SyncDirection::Upload), &local, &remote, &rec);
        assert_eq!(up, [Step::MoveAside { path: "b".into(), target: Side::Remote }, Step::Upload("a".into())]);
        let down = plan(&pair(SyncDirection::Download), &local, &remote, &rec);
        assert_eq!(
            down,
            [
                Step::MoveAside { path: "a".into(), target: Side::Local },
                Step::MoveAside { path: "b".into(), target: Side::Local },
                Step::Download("theirs".into()),
            ]
        );
    }

    #[test]
    fn unreadable_folders_and_type_clashes_are_left_alone() {
        let mut local = scan(&[("locked", Node::Dir), ("x", Node::Dir), ("x/inner", file(1, 1))]);
        local.unreadable.push("locked".into());
        let remote = scan(&[("locked", Node::Dir), ("locked/f", file(1, 1)), ("x", file(1, 1)), ("lockedness", file(1, 1))]);
        let rec = record(&[("x/inner", synced(1, 1))]);
        let steps = plan(&SyncPair { delete_remote: true, ..pair(SyncDirection::Both) }, &local, &remote, &rec);
        assert_eq!(steps, [Step::Record("locked".into(), Rec::Dir), Step::Mismatch("x".into()), Step::Download("lockedness".into())]);
    }
}
