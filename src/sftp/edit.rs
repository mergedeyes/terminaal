//! Editing a file of the server locally: download it into a private folder,
//! open it in an editor, and upload it again whenever it's saved.
//!
//! - **Local copies** live in `$XDG_RUNTIME_DIR/terminaal/edit/` (a folder
//!   of the user's own, in memory, gone at logout), one 0700 folder per file.
//!   An inotify thread watches those folders; a save counts once the file
//!   is written or renamed into place (editors do either) and things have
//!   been quiet for [`SETTLE`].
//! - **Before uploading**, the server's file is compared with what was
//!   synced last, by SHA-256: computed on the server ([`HASH_COMMAND`], one
//!   line back instead of the file), else by downloading files up to
//!   [`COMPARE_LIMIT`], else by size and modification time. If someone
//!   changed it meanwhile, the edit waits in `Conflict` until the user
//!   overwrites or takes theirs.
//! - **Uploading** writes a hidden file next to the original, gives it the
//!   original's mode and renames it over the original -- an interrupted
//!   upload never leaves half a file. Where that would change the file's
//!   owner (the new file would be ours) or the server can't rename over a
//!   file, it's written in place instead.
//! - **Without permission** (reading or writing), the edit offers sudo: a
//!   small script goes into `~/.cache/terminaal/sudo/<random>/` on the
//!   server, and the user runs ` sh <script>` in the terminal, where sudo
//!   asks for the password as usual. The script copies with `sudo cat` /
//!   `sudo cp` (the file keeps its owner and mode) and leaves its exit code
//!   in a `status` file, which this polls for. Saves of a file opened that
//!   way go through sudo again; the script compares the file's hash with
//!   the last synced one right before copying and stops with
//!   [`CONFLICT_EXIT`] if it changed.
//! - **Without a connection** local saves wait; they're uploaded -- after
//!   the same check -- once the session is attached again. Only the pasted command passes through the
//!   user's shell -- whatever it is -- the paths only ever through `sh`.

use std::collections::HashMap;
use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::protocol::{self, Attrs, Client, open};
use super::session::{Channels, Command, Fatal, file_name, join, parent};
use crate::i18n::t;

/// Quiet time after a local change before it's uploaded.
const SETTLE: Duration = Duration::from_millis(300);
/// How often the sudo script's status file is looked for.
const SUDO_POLL: Duration = Duration::from_millis(500);
/// Files up to this size are compared by content before an upload.
pub const COMPARE_LIMIT: u64 = 1024 * 1024;
/// Bigger files aren't opened for editing.
pub const EDIT_LIMIT: u64 = 32 * 1024 * 1024;
/// The sudo script's exit code when the file changed since the last sync.
pub const CONFLICT_EXIT: i32 = 99;

/// Hashes the file named on stdin on the server, through whatever shell the
/// login has -- the command itself has no quotes or backslashes that one
/// might read differently, the path never goes through it. Prints the
/// `sha256sum` line, `no-tool` without one, or an error.
pub const HASH_COMMAND: &str = "sh -c 'IFS= read -r p; if command -v sha256sum >/dev/null 2>&1; then sha256sum -- \"$p\"; elif command -v shasum >/dev/null 2>&1; then shasum -a 256 -- \"$p\"; else echo no-tool; fi'";

/// What the server said about a file's hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerHash {
    Hash([u8; 32]),
    /// Neither `sha256sum` nor `shasum` there.
    NoTool,
    /// It couldn't say (no permission, gone, no connection).
    Unknown,
}

/// Read [`HASH_COMMAND`]'s output.
pub fn parse_hash(output: &[u8]) -> ServerHash {
    let text = String::from_utf8_lossy(output);
    let text = text.trim();
    if text == "no-tool" {
        return ServerHash::NoTool;
    }
    let Some(hex) = text.split_whitespace().next().filter(|hex| hex.len() == 64) else { return ServerHash::Unknown };
    let mut hash = [0u8; 32];
    for (i, byte) in hash.iter_mut().enumerate() {
        match u8::from_str_radix(&hex[2 * i..2 * i + 2], 16) {
            Ok(value) => *byte = value,
            Err(_) => return ServerHash::Unknown,
        }
    }
    ServerHash::Hash(hash)
}

fn hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAction {
    /// Upload the local copy over the server's changed file.
    Overwrite,
    /// Drop the local changes: download the server's file again.
    TakeTheirs,
    /// Read, or save, through sudo in the terminal.
    UseSudo,
    /// The user ran the sudo command: wait for it.
    SudoStarted,
    CancelSudo,
    /// Open the local copy in the editor again.
    Reopen,
    /// Stop editing: delete the local copy.
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SudoFor {
    Open,
    Save,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditState {
    Synced,
    Uploading,
    /// The server's file changed since it was downloaded.
    Conflict,
    /// Not allowed without sudo.
    Denied(SudoFor),
    /// The sudo command is ready to run in the terminal.
    SudoReady(SudoFor),
    /// ... and the user ran it.
    SudoRunning(SudoFor),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditStatus {
    pub id: u64,
    pub remote: String,
    pub local: PathBuf,
    pub state: EditState,
    /// What to run in the terminal, while sudo is ready.
    pub command: Option<String>,
    /// The local copy differs from the server's file.
    pub unsaved: bool,
}

/// What the server's file was when last synced.
#[derive(Clone, Copy, Debug)]
struct Base {
    size: Option<u64>,
    mtime: Option<u32>,
    /// Content hash; `None` where it couldn't be read (sudo).
    hash: Option<[u8; 32]>,
}

impl Base {
    fn of(attrs: &Attrs, hash: Option<[u8; 32]>) -> Self {
        Self { size: attrs.size, mtime: attrs.mtime(), hash }
    }
}

struct SudoJob {
    /// The script's folder on the server.
    dir: String,
    purpose: SudoFor,
    command: String,
    next_poll: Instant,
}

struct Edit {
    id: u64,
    remote: String,
    local: PathBuf,
    editor: Option<String>,
    base: Option<Base>,
    /// The local copy as last synced.
    local_hash: Option<[u8; 32]>,
    state: EditState,
    /// Upload once this passes.
    due: Option<Instant>,
    sudo: Option<SudoJob>,
    /// Opened through sudo: saves go that way too.
    via_sudo: bool,
}

pub struct Edits {
    root: PathBuf,
    /// Runs the hash on the server.
    channels: Arc<dyn Channels>,
    /// Whether the server has a hash tool, once known on this connection.
    hash_tool: Option<bool>,
    watcher: Option<Watcher>,
    tx: Sender<Command>,
    edits: Vec<Edit>,
    next_id: u64,
    report: Option<Result<String, String>>,
}

impl Edits {
    /// Local copies go under `root`.
    pub fn new(root: PathBuf, tx: Sender<Command>, channels: Arc<dyn Channels>) -> Self {
        Self { root, channels, hash_tool: None, watcher: None, tx, edits: Vec::new(), next_id: 1, report: None }
    }

    pub fn set_channels(&mut self, channels: Arc<dyn Channels>) {
        self.channels = channels;
        self.hash_tool = None;
    }

    /// Attached to a (new) connection.
    pub fn connected(&mut self) {
        self.hash_tool = None;
    }

    /// The connection is gone: an upload it cut off is tried again once
    /// there's a new one.
    pub fn disconnected(&mut self) {
        for edit in &mut self.edits {
            if edit.state == EditState::Uploading {
                edit.due = Some(Instant::now());
            }
        }
    }

    /// The hash of `path` as the server computes it.
    fn server_hash(&mut self, path: &str) -> ServerHash {
        if self.hash_tool == Some(false) || path.contains(['\n', '\r']) {
            return ServerHash::NoTool;
        }
        let output = self.channels.exec(HASH_COMMAND, format!("{path}\n").as_bytes());
        let result = output.map_or(ServerHash::Unknown, |output| parse_hash(&output));
        match result {
            ServerHash::Hash(_) => self.hash_tool = Some(true),
            ServerHash::NoTool => self.hash_tool = Some(false),
            ServerHash::Unknown => {}
        }
        result
    }

    /// Where local copies go by default: in `$XDG_RUNTIME_DIR`, else a
    /// private folder in the temporary directory.
    pub fn default_root(label: &str) -> PathBuf {
        let base = match std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
            Some(dir) => PathBuf::from(dir).join("terminaal"),
            None => std::env::temp_dir().join(format!("terminaal-{}", unsafe { libc::getuid() })),
        };
        let label: String = label.chars().map(|c| if c.is_alphanumeric() || "@.-_".contains(c) { c } else { '_' }).collect();
        base.join("edit").join(format!("{label}-{}-{}", std::process::id(), unique()))
    }

    pub fn take_report(&mut self) -> Option<Result<String, String>> {
        self.report.take()
    }

    pub fn statuses(&self) -> Vec<EditStatus> {
        self.edits
            .iter()
            .map(|edit| EditStatus {
                id: edit.id,
                remote: edit.remote.clone(),
                local: edit.local.clone(),
                state: edit.state.clone(),
                command: edit.sudo.as_ref().filter(|_| matches!(edit.state, EditState::SudoReady(_))).map(|job| job.command.clone()),
                unsaved: edit.state != EditState::Synced
                    && std::fs::read(&edit.local).is_ok_and(|data| Some(hash(&data)) != edit.local_hash),
            })
            .collect()
    }

    /// Start editing `path`, or open the editor again if it's being edited.
    pub fn open<R: std::io::Read, W: std::io::Write>(
        &mut self,
        client: &mut Client<R, W>,
        path: &str,
        editor: Option<String>,
    ) -> Result<(), Fatal> {
        let remote = match check(client.realpath(path))? {
            Ok(remote) => remote,
            Err(err) => return self.fail_report(t!("files-edit-failed", name = file_name(path), err = err.to_string())),
        };
        if let Some(edit) = self.edits.iter_mut().find(|edit| edit.remote == remote) {
            edit.editor = editor;
            launch_editor(edit.editor.as_deref(), &edit.local);
            return Ok(());
        }
        let attrs = match check(client.stat(&remote))? {
            Ok(attrs) => attrs,
            Err(err) => return self.fail_report(t!("files-edit-failed", name = file_name(&remote), err = err.to_string())),
        };
        if !attrs.is_file() {
            return self.fail_report(t!("files-edit-not-a-file", name = file_name(&remote)));
        }
        if attrs.size.unwrap_or(0) > EDIT_LIMIT {
            return self.fail_report(t!("files-edit-too-big", name = file_name(&remote), limit = EDIT_LIMIT / (1024 * 1024)));
        }
        let local = match self.local_path(&remote) {
            Ok(local) => local,
            Err(err) => return self.fail_report(t!("files-local-failed", path = self.root.display().to_string(), err = err)),
        };
        let id = self.next_id;
        self.next_id += 1;
        let mut edit = Edit {
            id,
            remote,
            local,
            editor,
            base: Some(Base::of(&attrs, None)),
            local_hash: None,
            state: EditState::Synced,
            due: None,
            sudo: None,
            via_sudo: false,
        };
        match check(client.read_file(&edit.remote))? {
            Ok(data) => self.opened(&mut edit, &attrs, &data),
            Err(err) if err.permission_denied() => edit.state = EditState::Denied(SudoFor::Open),
            Err(err) => {
                let _ = std::fs::remove_dir_all(edit.local.parent().expect("in its own folder"));
                return self.fail_report(t!("files-edit-failed", name = file_name(&edit.remote), err = err.to_string()));
            }
        }
        self.edits.push(edit);
        Ok(())
    }

    fn fail_report(&mut self, message: String) -> Result<(), Fatal> {
        self.report = Some(Err(message));
        Ok(())
    }

    /// A fresh 0700 folder for one file's local copy.
    fn local_path(&self, remote: &str) -> Result<PathBuf, String> {
        let dir = self.root.join(unique());
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|err| err.to_string())?;
        // `recursive` leaves existing parents as they are: make sure no one
        // else can look in.
        for folder in [&self.root, &dir] {
            std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700)).map_err(|err| err.to_string())?;
        }
        Ok(dir.join(file_name(remote)))
    }

    /// The content arrived: write the local copy, watch it, open the editor.
    fn opened(&mut self, edit: &mut Edit, attrs: &Attrs, data: &[u8]) {
        if let Err(err) = write_private(&edit.local, data) {
            edit.state = EditState::Failed(t!("files-local-failed", path = edit.local.display().to_string(), err = err.to_string()));
            return;
        }
        let digest = hash(data);
        edit.base = Some(Base::of(attrs, Some(digest)));
        edit.local_hash = Some(digest);
        edit.state = EditState::Synced;
        self.watch(edit.local.parent().expect("in its own folder"));
        launch_editor(edit.editor.as_deref(), &edit.local);
    }

    fn watch(&mut self, dir: &Path) {
        if self.watcher.is_none() {
            let tx = self.tx.clone();
            match Watcher::new(move |path| tx.send(Command::LocalChanged(path)).is_ok()) {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(err) => {
                    log::warn!("can't watch local copies: {err}");
                    return;
                }
            }
        }
        if let Some(watcher) = &self.watcher
            && let Err(err) = watcher.add(dir)
        {
            log::warn!("can't watch {}: {err}", dir.display());
        }
    }

    /// The watcher saw `path` written.
    pub fn local_changed(&mut self, path: &Path) {
        let Some(edit) = self.edits.iter_mut().find(|edit| edit.local == path) else { return };
        let waits = matches!(
            edit.state,
            EditState::Denied(SudoFor::Open) | EditState::SudoReady(SudoFor::Open) | EditState::SudoRunning(_)
        );
        if !waits {
            edit.due = Some(Instant::now() + SETTLE);
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.edits
            .iter()
            .flat_map(|edit| {
                let poll = edit.sudo.as_ref().filter(|_| matches!(edit.state, EditState::SudoRunning(_))).map(|job| job.next_poll);
                [edit.due, poll]
            })
            .flatten()
            .min()
    }

    /// Upload what's due, look for finished sudo scripts. Returns whether
    /// anything changed.
    pub fn tick<R: std::io::Read, W: std::io::Write>(&mut self, client: &mut Client<R, W>) -> Result<bool, Fatal> {
        let now = Instant::now();
        let mut changed = false;
        for i in 0..self.edits.len() {
            if self.edits[i].due.is_some_and(|due| due <= now) {
                self.edits[i].due = None;
                changed |= self.save(client, i, false)?;
            }
            let running = matches!(self.edits[i].state, EditState::SudoRunning(_));
            if running && self.edits[i].sudo.as_ref().is_some_and(|job| job.next_poll <= now) {
                self.poll_sudo(client, i)?;
                changed = true;
            }
        }
        Ok(changed)
    }

    pub fn action<R: std::io::Read, W: std::io::Write>(
        &mut self,
        client: &mut Client<R, W>,
        id: u64,
        action: EditAction,
    ) -> Result<(), Fatal> {
        let Some(i) = self.edits.iter().position(|edit| edit.id == id) else { return Ok(()) };
        match action {
            EditAction::Overwrite => {
                self.save(client, i, true)?;
            }
            EditAction::TakeTheirs => self.take_theirs(client, i)?,
            EditAction::UseSudo => {
                let purpose = match self.edits[i].state {
                    EditState::Denied(purpose) => purpose,
                    _ => return Ok(()),
                };
                self.prepare_sudo(client, i, purpose, false)?;
            }
            EditAction::SudoStarted => {
                let edit = &mut self.edits[i];
                if let (EditState::SudoReady(purpose), Some(job)) = (edit.state.clone(), edit.sudo.as_mut()) {
                    job.next_poll = Instant::now() + SUDO_POLL;
                    edit.state = EditState::SudoRunning(purpose);
                }
            }
            EditAction::CancelSudo => {
                if let Some(job) = self.edits[i].sudo.take() {
                    remove_sudo_dir(client, &job.dir)?;
                    self.edits[i].state = EditState::Denied(job.purpose);
                }
            }
            EditAction::Reopen => launch_editor(self.edits[i].editor.as_deref(), &self.edits[i].local),
            EditAction::Close => {
                let edit = self.edits.remove(i);
                if let Some(job) = &edit.sudo {
                    remove_sudo_dir(client, &job.dir)?;
                }
                self.forget(&edit);
            }
        }
        Ok(())
    }

    fn forget(&self, edit: &Edit) {
        let dir = edit.local.parent().expect("in its own folder");
        if let Some(watcher) = &self.watcher {
            watcher.remove(dir);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Upload the local copy of edit `i` if it changed -- unless the
    /// server's file changed too (`force` uploads anyway). Returns whether
    /// anything happened.
    fn save<R: std::io::Read, W: std::io::Write>(
        &mut self,
        client: &mut Client<R, W>,
        i: usize,
        force: bool,
    ) -> Result<bool, Fatal> {
        let edit = &mut self.edits[i];
        let data = match std::fs::read(&edit.local) {
            Ok(data) => data,
            Err(err) => {
                edit.state = EditState::Failed(t!("files-local-failed", path = edit.local.display().to_string(), err = err.to_string()));
                return Ok(true);
            }
        };
        let digest = hash(&data);
        let unchanged = Some(digest) == edit.local_hash;
        if unchanged && !force && edit.state == EditState::Synced {
            return Ok(false);
        }
        // Through sudo, the script checks right before copying.
        let script_checks = self.edits[i].via_sudo && self.hash_tool != Some(false);
        if !(force || script_checks) {
            match self.server_changed(client, i)? {
                Ok(false) => {}
                Ok(true) => {
                    self.edits[i].state = EditState::Conflict;
                    self.report = Some(Err(t!("files-edit-conflict", name = file_name(&self.edits[i].remote))));
                    return Ok(true);
                }
                Err(err) => {
                    self.edits[i].state = EditState::Failed(err);
                    return Ok(true);
                }
            }
        }
        if self.edits[i].via_sudo {
            // A fresh script with this content; the user runs it.
            self.prepare_sudo(client, i, SudoFor::Save, force)?;
            return Ok(true);
        }
        if self.edits[i].state == EditState::Denied(SudoFor::Save) {
            // Still waiting for the user to choose sudo; that reads the
            // newest content then.
            return Ok(true);
        }
        let edit = &mut self.edits[i];
        edit.state = EditState::Uploading;
        match check(upload(client, &edit.remote, &data))? {
            Ok(attrs) => {
                edit.base = Some(Base::of(&attrs, Some(digest)));
                edit.local_hash = Some(digest);
                edit.state = EditState::Synced;
                self.report = Some(Ok(t!("files-edit-saved", name = file_name(&edit.remote))));
            }
            Err(err) if err.permission_denied() => edit.state = EditState::Denied(SudoFor::Save),
            Err(err) => edit.state = EditState::Failed(t!("files-edit-upload-failed", err = err.to_string())),
        }
        Ok(true)
    }

    /// Whether the server's file is no longer what was synced last. `Err`:
    /// couldn't tell, and why.
    fn server_changed<R: std::io::Read, W: std::io::Write>(
        &mut self,
        client: &mut Client<R, W>,
        i: usize,
    ) -> Result<Result<bool, String>, Fatal> {
        let Some(base) = self.edits[i].base else { return Ok(Ok(false)) };
        let remote = self.edits[i].remote.clone();
        if let (Some(base_hash), ServerHash::Hash(now)) = (base.hash, self.server_hash(&remote)) {
            return Ok(Ok(now != base_hash));
        }
        let edit = &self.edits[i];
        let attrs = match check(client.stat(&edit.remote))? {
            Ok(attrs) => attrs,
            // Deleted meanwhile: that's a change too.
            Err(err) if err.not_found() => return Ok(Ok(true)),
            Err(err) => return Ok(Err(t!("files-edit-upload-failed", err = err.to_string()))),
        };
        let same_attrs = attrs.size == base.size && attrs.mtime() == base.mtime;
        let (Some(base_hash), Some(size)) = (base.hash, attrs.size) else { return Ok(Ok(!same_attrs)) };
        if size > COMPARE_LIMIT {
            return Ok(Ok(!same_attrs));
        }
        match check(client.read_file(&edit.remote))? {
            Ok(data) => Ok(Ok(hash(&data) != base_hash)),
            Err(_) => Ok(Ok(!same_attrs)),
        }
    }

    fn take_theirs<R: std::io::Read, W: std::io::Write>(&mut self, client: &mut Client<R, W>, i: usize) -> Result<(), Fatal> {
        let remote = self.edits[i].remote.clone();
        let fetched = check(client.stat(&remote).and_then(|attrs| Ok((attrs, client.read_file(&remote)?))))?;
        let edit = &mut self.edits[i];
        match fetched {
            Ok((attrs, data)) => match write_in_place(&edit.local, &data) {
                Ok(()) => {
                    let digest = hash(&data);
                    edit.base = Some(Base::of(&attrs, Some(digest)));
                    edit.local_hash = Some(digest);
                    edit.state = EditState::Synced;
                    edit.due = None;
                }
                Err(err) => {
                    edit.state =
                        EditState::Failed(t!("files-local-failed", path = edit.local.display().to_string(), err = err.to_string()))
                }
            },
            Err(err) if err.permission_denied() => {
                // Only sudo could read it: start over through that.
                edit.via_sudo = true;
                self.prepare_sudo(client, i, SudoFor::Open, false)?;
            }
            Err(err) => edit.state = EditState::Failed(t!("files-edit-failed", name = file_name(&remote), err = err.to_string())),
        }
        Ok(())
    }

    /// Put the script (and for a save, the content) on the server and show
    /// the command to run. A save checks the file's hash first, unless
    /// `force`.
    fn prepare_sudo<R: std::io::Read, W: std::io::Write>(
        &mut self,
        client: &mut Client<R, W>,
        i: usize,
        purpose: SudoFor,
        force: bool,
    ) -> Result<(), Fatal> {
        if let Some(job) = self.edits[i].sudo.take() {
            remove_sudo_dir(client, &job.dir)?;
        }
        let data = match purpose {
            SudoFor::Save => match std::fs::read(&self.edits[i].local) {
                Ok(data) => Some(data),
                Err(err) => {
                    let path = self.edits[i].local.display().to_string();
                    self.edits[i].state = EditState::Failed(t!("files-local-failed", path = path, err = err.to_string()));
                    return Ok(());
                }
            },
            SudoFor::Open => None,
        };
        let remote = self.edits[i].remote.clone();
        let expected = self.edits[i].base.and_then(|base| base.hash).filter(|_| !force && purpose == SudoFor::Save);
        match check(write_sudo_job(client, &remote, purpose, data.as_deref(), expected))? {
            Ok(Ok(job)) => {
                let edit = &mut self.edits[i];
                if purpose == SudoFor::Save {
                    edit.via_sudo = true;
                    edit.local_hash = data.as_deref().map(hash).or(edit.local_hash);
                }
                edit.sudo = Some(job);
                edit.state = EditState::SudoReady(purpose);
            }
            Ok(Err(message)) => self.edits[i].state = EditState::Failed(message),
            Err(err) => self.edits[i].state = EditState::Failed(t!("files-sudo-prepare-failed", err = err.to_string())),
        }
        Ok(())
    }

    fn poll_sudo<R: std::io::Read, W: std::io::Write>(&mut self, client: &mut Client<R, W>, i: usize) -> Result<(), Fatal> {
        let Some(job) = self.edits[i].sudo.as_mut() else { return Ok(()) };
        let status_path = join(&job.dir, "status");
        let status = match check(client.read_file(&status_path))? {
            Ok(status) => status,
            Err(_) => {
                job.next_poll = Instant::now() + SUDO_POLL;
                return Ok(());
            }
        };
        let job = self.edits[i].sudo.take().expect("checked above");
        let code: i32 = String::from_utf8_lossy(&status).trim().parse().unwrap_or(-1);
        if code == CONFLICT_EXIT && job.purpose == SudoFor::Save {
            remove_sudo_dir(client, &job.dir)?;
            let edit = &mut self.edits[i];
            edit.state = EditState::Conflict;
            self.report = Some(Err(t!("files-edit-conflict", name = file_name(&edit.remote))));
            return Ok(());
        }
        let result = if code != 0 {
            Err(t!("files-sudo-failed", code = code))
        } else {
            match job.purpose {
                SudoFor::Open => match check(client.read_file(&join(&job.dir, "data")))? {
                    Ok(data) => Ok(Some(data)),
                    Err(err) => Err(t!("files-edit-failed", name = file_name(&self.edits[i].remote), err = err.to_string())),
                },
                SudoFor::Save => Ok(None),
            }
        };
        remove_sudo_dir(client, &job.dir)?;
        let remote = self.edits[i].remote.clone();
        let attrs = check(client.stat(&remote))?.unwrap_or_default();
        match result {
            Err(message) => {
                self.edits[i].state = EditState::Failed(message.clone());
                self.report = Some(Err(message));
            }
            Ok(Some(data)) => {
                let mut edit = self.edits.remove(i);
                edit.via_sudo = true;
                self.opened(&mut edit, &attrs, &data);
                self.edits.insert(i, edit);
            }
            Ok(None) => {
                let edit = &mut self.edits[i];
                // What was copied: the content as it was when the script was made.
                edit.base = Some(Base::of(&attrs, edit.local_hash));
                edit.state = EditState::Synced;
                self.report = Some(Ok(t!("files-edit-saved", name = file_name(&remote))));
            }
        }
        Ok(())
    }

    /// Without a connection: only opening the editor and closing work.
    pub fn offline_action(&mut self, id: u64, action: EditAction) {
        let Some(i) = self.edits.iter().position(|edit| edit.id == id) else { return };
        match action {
            EditAction::Reopen => launch_editor(self.edits[i].editor.as_deref(), &self.edits[i].local),
            EditAction::Close => {
                let edit = self.edits.remove(i);
                self.forget(&edit);
            }
            _ => {}
        }
    }

    /// The tab closed: local copies go, unless they hold changes the server
    /// never got -- those stay where they are.
    pub fn close_all(&mut self) {
        for edit in std::mem::take(&mut self.edits) {
            let unsaved = std::fs::read(&edit.local).is_ok_and(|data| Some(hash(&data)) != edit.local_hash);
            if unsaved {
                log::warn!("unsaved local copy of {} kept at {}", edit.remote, edit.local.display());
            } else {
                self.forget(&edit);
            }
        }
        let _ = std::fs::remove_dir(&self.root);
    }
}

/// A broken connection ends the session; any other error is the caller's.
fn check<T>(result: protocol::Result<T>) -> Result<protocol::Result<T>, Fatal> {
    match result {
        Err(err) if err.is_fatal() => Err(err.into()),
        other => Ok(other),
    }
}

fn hash(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Different each call, for folder and file names.
fn unique() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{nanos:08x}{count:x}")
}

fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    file.write_all(data)
}

/// Replace the content of a local copy the editor may have open: into the
/// same file, so an editor that watches it sees the change.
fn write_in_place(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).truncate(true).open(path)?;
    file.write_all(data)
}

/// Open `path` with `editor` (a shell command, the path appended), or the
/// desktop's default application.
fn launch_editor(editor: Option<&str>, path: &Path) {
    let mut command = match editor.map(str::trim).filter(|editor| !editor.is_empty()) {
        Some(editor) => {
            let mut command = std::process::Command::new("/bin/sh");
            command.arg("-c").arg(format!("exec {editor} \"$1\"")).arg("sh").arg(path);
            command
        }
        None => {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(path);
            command
        }
    };
    let spawned = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match spawned {
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(err) => log::warn!("failed to open {} in an editor: {err}", path.display()),
    }
}

/// Write `data` over `remote`: through a hidden file renamed over it where
/// that keeps owner and mode, else in place. Returns the file's attributes
/// afterwards.
fn upload<R: std::io::Read, W: std::io::Write>(client: &mut Client<R, W>, remote: &str, data: &[u8]) -> protocol::Result<Attrs> {
    let original = client.stat(remote)?;
    let mode = original.permissions.unwrap_or(0o644) & 0o7777;
    if client.has_extension("posix-rename@openssh.com") {
        let temp = join(&parent(remote), &format!(".{}.terminaal-{}", file_name(remote), unique()));
        match client.open(&temp, open::WRITE | open::CREATE | open::EXCLUSIVE, Attrs::mode(mode)) {
            Ok(handle) => {
                let written = client.write_all_at(&handle, data).and_then(|()| client.fstat(&handle));
                let closed = client.close(&handle);
                match written.and_then(|attrs| closed.map(|()| attrs)) {
                    // Ours, like the original: rename it over.
                    Ok(attrs) if attrs.uid_gid == original.uid_gid => {
                        let renamed = client.setstat(&temp, Attrs::mode(mode)).and_then(|()| client.posix_rename(&temp, remote));
                        if renamed.is_ok() {
                            return client.stat(remote);
                        }
                        let _ = client.remove(&temp);
                    }
                    Ok(_) => {
                        let _ = client.remove(&temp);
                    }
                    Err(err) => {
                        let _ = client.remove(&temp);
                        if err.is_fatal() {
                            return Err(err);
                        }
                    }
                }
            }
            // The folder isn't ours to write in; maybe the file is.
            Err(err) if err.permission_denied() => {}
            Err(err) => return Err(err),
        }
    }
    client.write_file(remote, data, open::TRUNCATE, mode)?;
    client.stat(remote)
}

/// Create the sudo script's folder with the script in it (and the content
/// to save). `Ok(Err)`: a path the script can't safely carry.
fn write_sudo_job<R: std::io::Read, W: std::io::Write>(
    client: &mut Client<R, W>,
    remote: &str,
    purpose: SudoFor,
    data: Option<&[u8]>,
    expected: Option<[u8; 32]>,
) -> protocol::Result<Result<SudoJob, String>> {
    let home = client.realpath(".")?;
    let mut dir = home.clone();
    for part in [".cache", "terminaal", "sudo"] {
        dir = join(&dir, part);
        match client.stat(&dir) {
            Ok(attrs) if attrs.is_dir() => {}
            Ok(_) => return Ok(Err(t!("files-sudo-prepare-failed", err = dir))),
            Err(err) if err.not_found() => client.mkdir(&dir, Attrs::mode(0o700))?,
            Err(err) => return Err(err),
        }
    }
    let dir = join(&dir, &unique());
    let script_path = join(&dir, "run.sh");
    let Some(command) = paste_command(&script_path) else {
        return Ok(Err(t!("files-sudo-path", path = &home)));
    };
    client.mkdir(&dir, Attrs::mode(0o700))?;
    if let Some(data) = data {
        client.write_file(&join(&dir, "data"), data, open::EXCLUSIVE, 0o600)?;
    }
    let message = match purpose {
        SudoFor::Open => t!("files-sudo-reading", path = remote),
        SudoFor::Save => t!("files-sudo-writing", path = remote),
    };
    let script = sudo_script(purpose, &dir, remote, &message, expected.as_ref());
    client.write_file(&script_path, script.as_bytes(), open::EXCLUSIVE, 0o600)?;
    Ok(Ok(SudoJob { dir, purpose, command, next_poll: Instant::now() }))
}

fn remove_sudo_dir<R: std::io::Read, W: std::io::Write>(client: &mut Client<R, W>, dir: &str) -> Result<(), Fatal> {
    for name in ["run.sh", "data", "data.part", "status", "status.part"] {
        check(client.remove(&join(dir, name)))?.ok();
    }
    check(client.rmdir(dir))?.ok();
    Ok(())
}

/// Quote for `sh`: in single quotes, a quote as `'\''`.
pub fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// What the user runs: ` sh '<script>'` -- a leading space keeps it out of
/// the history in shells set up for that. `None` for a path that some
/// shell would read differently in single quotes (fish takes `\` there).
pub fn paste_command(script: &str) -> Option<String> {
    let plain = !script.contains(['\'', '\\', '\n', '\r']);
    plain.then(|| format!(" sh '{script}'"))
}

/// The script: `sudo cat` the file into `data` (reading) or `sudo cp` `data`
/// over the file (writing -- `cp` into an existing file keeps its owner and
/// mode), then leave the exit code in `status`, renamed into place so it's
/// never seen half-written. Writing with `expected`, the file's hash is
/// compared first (one sudo for both, where a hash tool exists); a
/// different one stops with [`CONFLICT_EXIT`].
pub fn sudo_script(purpose: SudoFor, dir: &str, target: &str, message: &str, expected: Option<&[u8; 32]>) -> String {
    let work = match purpose {
        SudoFor::Open => {
            "if sudo cat -- \"$t\" > \"$d/data.part\"; then mv -f -- \"$d/data.part\" \"$d/data\"; s=0; else s=$?; fi".to_string()
        }
        SudoFor::Save => {
            let check = match expected {
                Some(hash) => format!(
                    "h={}\n\
                     c=$(sudo sh -c 'if command -v sha256sum >/dev/null 2>&1; then sha256sum -- \"$1\"; elif command -v shasum >/dev/null 2>&1; then shasum -a 256 -- \"$1\"; fi' sh \"$t\")\n\
                     c=${{c%% *}}\n\
                     if [ -n \"$c\" ] && [ \"$c\" != \"$h\" ]; then s={CONFLICT_EXIT}; fi\n",
                    sh_quote(&hex(hash))
                ),
                None => String::new(),
            };
            format!("s=0\n{check}if [ \"$s\" = 0 ]; then if sudo cp -- \"$d/data\" \"$t\"; then s=0; else s=$?; fi; fi")
        }
    };
    format!(
        "# Terminaal\nd={}\nt={}\nprintf '%s\\n' {}\n{work}\nprintf '%s\\n' \"$s\" > \"$d/status.part\" && mv -f -- \"$d/status.part\" \"$d/status\"\nrm -f -- \"$0\"\n",
        sh_quote(dir),
        sh_quote(target),
        sh_quote(message),
    )
}

/// inotify on the folders of local copies, on a thread of its own:
/// `changed` gets a file that was written (closed after writing) or renamed
/// into a watched folder, until it returns `false`.
struct Watcher {
    fd: Arc<OwnedFd>,
    dirs: Arc<Mutex<HashMap<i32, PathBuf>>>,
    /// Writing to it ends the thread.
    stop: std::os::unix::net::UnixStream,
}

impl Watcher {
    fn new(changed: impl Fn(PathBuf) -> bool + Send + 'static) -> std::io::Result<Self> {
        // SAFETY: a plain syscall; the descriptor is owned right away.
        let raw = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
        if raw < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fd = Arc::new(unsafe { OwnedFd::from_raw_fd(raw) });
        let dirs: Arc<Mutex<HashMap<i32, PathBuf>>> = Arc::default();
        let (stop, stop_rx) = std::os::unix::net::UnixStream::pair()?;
        let (thread_fd, thread_dirs) = (fd.clone(), dirs.clone());
        std::thread::Builder::new().name("edit-watch".into()).spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                let mut fds = [
                    libc::pollfd { fd: thread_fd.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                    libc::pollfd { fd: stop_rx.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                ];
                // SAFETY: two valid pollfds for the duration of the call.
                let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
                if ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                if ready < 0 || fds[1].revents != 0 {
                    return;
                }
                // SAFETY: `buf` is valid for its length.
                let n = unsafe { libc::read(thread_fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    continue;
                }
                for (wd, name) in parse_events(&buf[..n as usize]) {
                    let dir = thread_dirs.lock().unwrap_or_else(PoisonError::into_inner).get(&wd).cloned();
                    if let Some(dir) = dir
                        && !changed(dir.join(name))
                    {
                        return;
                    }
                }
            }
        })?;
        Ok(Self { fd, dirs, stop })
    }

    fn add(&self, dir: &Path) -> std::io::Result<()> {
        let path = CString::new(dir.as_os_str().as_bytes()).map_err(std::io::Error::other)?;
        let mask = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO;
        // SAFETY: a valid descriptor and NUL-terminated path.
        let wd = unsafe { libc::inotify_add_watch(self.fd.as_raw_fd(), path.as_ptr(), mask) };
        if wd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        self.dirs.lock().unwrap_or_else(PoisonError::into_inner).insert(wd, dir.to_path_buf());
        Ok(())
    }

    fn remove(&self, dir: &Path) {
        let mut dirs = self.dirs.lock().unwrap_or_else(PoisonError::into_inner);
        let wds: Vec<i32> = dirs.iter().filter(|(_, watched)| *watched == dir).map(|(wd, _)| *wd).collect();
        for wd in wds {
            dirs.remove(&wd);
            // SAFETY: a plain syscall on our descriptor.
            unsafe { libc::inotify_rm_watch(self.fd.as_raw_fd(), wd) };
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        use std::io::Write;
        let _ = (&self.stop).write(&[1]);
    }
}

/// The watch descriptor and file name of each event with a name.
fn parse_events(mut buf: &[u8]) -> Vec<(i32, std::ffi::OsString)> {
    const HEADER: usize = std::mem::size_of::<libc::inotify_event>();
    let mut events = Vec::new();
    while buf.len() >= HEADER {
        // SAFETY: at least a header's worth of bytes; read unaligned.
        let event: libc::inotify_event = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast()) };
        let end = HEADER + event.len as usize;
        if buf.len() < end {
            break;
        }
        let name = &buf[HEADER..end];
        let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
        if !name.is_empty() {
            events.push((event.wd, std::ffi::OsStr::from_bytes(name).to_os_string()));
        }
        buf = &buf[end..];
    }
    events
}

#[cfg(test)]
mod tests {
    use std::process::Command as Process;

    use super::super::protocol::tests::scratch;
    use super::super::session::tests::{Session, session, wait_for};
    use super::super::session::{Command, Connection};
    use super::*;

    /// A `sudo` that makes `target` readable and writable just for the
    /// command, as the real one would by running it as root.
    fn fake_sudo(dir: &Path, target: &Path) -> PathBuf {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = format!(
            "#!/bin/sh\nchmod 600 {t}\n\"$@\"\ns=$?\nchmod 000 {t}\nexit $s\n",
            t = sh_quote(target.to_str().unwrap())
        );
        std::fs::write(bin.join("sudo"), script).unwrap();
        std::fs::set_permissions(bin.join("sudo"), std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    fn run_pasted(command: &str, bin: &Path) -> std::process::ExitStatus {
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        Process::new("/bin/sh").arg("-c").arg(command).env("PATH", path).status().unwrap()
    }

    /// Change a file's content behind the editor's back, keeping its size
    /// and modification time -- only a hash tells.
    fn sneaky_change(path: &Path, content: &[u8]) {
        let before = std::fs::metadata(path).unwrap();
        assert_eq!(before.len(), content.len() as u64);
        std::fs::write(path, content).unwrap();
        std::fs::File::options().write(true).open(path).unwrap().set_modified(before.modified().unwrap()).unwrap();
    }

    #[test]
    fn quoting_survives_awkward_paths() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(paste_command("/home/u/.cache/terminaal/sudo/ab/run.sh").unwrap(), " sh '/home/u/.cache/terminaal/sudo/ab/run.sh'");
        for bad in ["/home/o'neil/x", r"/home/a\b/x", "/home/a\nb/x"] {
            assert!(paste_command(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn server_hashes_are_read() {
        let hex_hash = "a".repeat(62) + "0f";
        let ServerHash::Hash(hash) = parse_hash(format!("{hex_hash}  /etc/hosts\n").as_bytes()) else { panic!() };
        assert_eq!((hash[0], hash[31]), (0xaa, 0x0f));
        assert_eq!(hex(&hash), hex_hash);
        assert_eq!(parse_hash(b"no-tool\n"), ServerHash::NoTool);
        assert_eq!(parse_hash(b"sha256sum: /x: Permission denied\n"), ServerHash::Unknown);
        assert_eq!(parse_hash(b""), ServerHash::Unknown);
        assert_eq!(parse_hash(format!("{}  x", "z".repeat(64)).as_bytes()), ServerHash::Unknown);

        // The command itself, with this machine's tools and a nasty name.
        let dir = scratch("hash");
        let file = dir.join("it's \"a\" $file");
        std::fs::write(&file, "hello").unwrap();
        let output = Process::new("/bin/sh")
            .arg("-c")
            .arg(HASH_COMMAND)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(format!("{}\n", file.display()).as_bytes())?;
                child.wait_with_output()
            })
            .unwrap();
        assert_eq!(parse_hash(&output.stdout), ServerHash::Hash(super::hash(b"hello")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_sudo_scripts_copy_and_leave_their_status() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let root = scratch("script");
        let work = root.join("work dir");
        std::fs::create_dir_all(&work).unwrap();
        let target = root.join("it's \"root's\" $file");
        std::fs::write(&target, "secret\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
        let bin = fake_sudo(&root, &target);
        let (work_s, target_s) = (work.to_str().unwrap(), target.to_str().unwrap());
        let script = work.join("run.sh");
        let run = |text: String| {
            let _ = std::fs::remove_file(work.join("status"));
            std::fs::write(&script, text).unwrap();
            assert!(run_pasted(&format!("sh {}", sh_quote(script.to_str().unwrap())), &bin).success());
            assert!(!script.exists(), "the script removes itself");
            std::fs::read_to_string(work.join("status")).unwrap()
        };

        assert_eq!(run(sudo_script(SudoFor::Open, work_s, target_s, "reading it's file", None)), "0\n");
        assert_eq!(std::fs::read_to_string(work.join("data")).unwrap(), "secret\n");

        // The hash doesn't match what was read: nothing is copied.
        std::fs::write(work.join("data"), "changed\n").unwrap();
        let stale = super::hash(b"something else\n");
        assert_eq!(run(sudo_script(SudoFor::Save, work_s, target_s, "writing", Some(&stale))), format!("{CONFLICT_EXIT}\n"));
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "secret\n");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();

        // It matches: copied.
        let current = super::hash(b"secret\n");
        assert_eq!(run(sudo_script(SudoFor::Save, work_s, target_s, "writing", Some(&current))), "0\n");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "changed\n");

        // A failing sudo leaves its exit code.
        let missing = root.join("missing");
        assert_ne!(run(sudo_script(SudoFor::Open, work_s, missing.to_str().unwrap(), "x", None)), "0\n");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_watcher_reports_writes_and_renames() {
        let dir = scratch("watch");
        let (tx, rx) = std::sync::mpsc::channel();
        let watcher = Watcher::new(move |path| tx.send(path).is_ok()).unwrap();
        watcher.add(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "1").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), dir.join("a.txt"));
        // Like editors that write a new file and rename it over the old one.
        std::fs::write(dir.join(".a.txt.swp"), "2").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), dir.join(".a.txt.swp"));
        std::fs::rename(dir.join(".a.txt.swp"), dir.join("a.txt")).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), dir.join("a.txt"));
        watcher.remove(&dir);
        std::fs::write(dir.join("b.txt"), "3").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "no longer watched");
        drop(watcher);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn edit_of<'a>(state: &'a super::super::session::State, remote: &Path) -> Option<&'a EditStatus> {
        state.edits.iter().find(|edit| Path::new(&edit.remote) == remote)
    }

    /// Open `file` for editing and wait until it's synced.
    fn open(remote: &super::super::session::Remote, file: &Path) -> EditStatus {
        remote.send(Command::Edit { path: file.to_str().unwrap().into(), editor: Some("true".into()) });
        let state = wait_for(remote, "opened", |s| edit_of(s, file).is_some_and(|e| e.state == EditState::Synced));
        edit_of(&state, file).unwrap().clone()
    }

    /// Dropping the tab's end ends the session and deletes synced copies,
    /// even though the watcher still holds a sender.
    #[test]
    fn closing_the_tab_ends_the_session() {
        let Some(Session { remote, server, .. }) = session("close") else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("close-server");
        let file = root.join("f.txt");
        std::fs::write(&file, "x").unwrap();
        let file = file.canonicalize().unwrap();
        let local = open(&remote, &file).local;
        drop(remote);
        let start = Instant::now();
        while local.exists() || server.running() > 0 {
            assert!(start.elapsed() < Duration::from_secs(5), "local copy or sftp-server still there");
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn edits_sync_back_and_catch_conflicts() {
        let Some(Session { remote, .. }) = session("edit") else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("edit-server");
        let file = root.join("notes.txt");
        std::fs::write(&file, "one\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        let file = file.canonicalize().unwrap();

        let edit = open(&remote, &file);
        assert_eq!(std::fs::read_to_string(&edit.local).unwrap(), "one\n");
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(&edit.local), mode(edit.local.parent().unwrap())), (0o600, 0o700));

        // Saved locally: the server gets it, keeping its mode.
        std::fs::write(&edit.local, "two\n").unwrap();
        wait_for(&remote, "uploaded", |_| std::fs::read_to_string(&file).unwrap() == "two\n");
        assert_eq!(mode(&file), 0o640);
        assert!(root.read_dir().unwrap().count() == 1, "no temporary file left");
        wait_for(&remote, "synced", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Synced && !e.unsaved));

        // Someone else changes it too: conflict, nothing overwritten.
        std::fs::write(&file, "theirs\n").unwrap();
        std::fs::write(&edit.local, "mine\n").unwrap();
        wait_for(&remote, "conflict", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Conflict && e.unsaved));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "theirs\n");
        remote.send(Command::EditAction { id: edit.id, action: EditAction::Overwrite });
        wait_for(&remote, "overwritten", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Synced));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine\n");

        // Or take theirs.
        std::fs::write(&file, "theirs again\n").unwrap();
        std::fs::write(&edit.local, "mine again\n").unwrap();
        wait_for(&remote, "second conflict", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Conflict));
        remote.send(Command::EditAction { id: edit.id, action: EditAction::TakeTheirs });
        wait_for(&remote, "taken", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Synced));
        assert_eq!(std::fs::read_to_string(&edit.local).unwrap(), "theirs again\n");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "theirs again\n");

        remote.send(Command::EditAction { id: edit.id, action: EditAction::Close });
        wait_for(&remote, "closed", |s| s.edits.is_empty());
        assert!(!edit.local.parent().unwrap().exists());
        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Too big to download for comparing, same size, same time: the hash
    /// on the server still tells.
    #[test]
    fn a_big_file_changed_behind_the_back_is_caught_by_its_hash() {
        let Some(Session { remote, .. }) = session("hash-conflict") else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("hash-conflict-server");
        let file = root.join("big.log");
        let original = vec![b'a'; COMPARE_LIMIT as usize * 3];
        std::fs::write(&file, &original).unwrap();
        let file = file.canonicalize().unwrap();
        let edit = open(&remote, &file);

        let mut theirs = original.clone();
        theirs[12345] = b'b';
        sneaky_change(&file, &theirs);
        let mut mine = original.clone();
        mine[0] = b'm';
        std::fs::write(&edit.local, &mine).unwrap();
        wait_for(&remote, "conflict", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Conflict));
        assert_eq!(std::fs::read(&file).unwrap(), theirs);
        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Saves made while disconnected go up once the session is back.
    #[test]
    fn saves_wait_for_the_connection() {
        let Some(Session { remote, server, .. }) = session("offline-save") else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("offline-save-server");
        let file = root.join("todo.txt");
        std::fs::write(&file, "old\n").unwrap();
        let file = file.canonicalize().unwrap();
        let edit = open(&remote, &file);

        server.drop_connection();
        wait_for(&remote, "waiting", |s| s.connection == Connection::Waiting);
        std::fs::write(&edit.local, "written offline\n").unwrap();
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old\n");
        server.reconnect();
        wait_for(&remote, "uploaded", |_| std::fs::read_to_string(&file).unwrap() == "written offline\n");
        wait_for(&remote, "synced", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Synced));
        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn files_without_permission_go_through_sudo() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let Some(Session { remote, .. }) = session("sudo") else { return };
        let state = wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("sudo-server");
        let file = root.join("config");
        std::fs::write(&file, "root only\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let file = file.canonicalize().unwrap();
        let bin = fake_sudo(&root, &file);
        let run_ready = |purpose: SudoFor| {
            let ready = wait_for(&remote, "sudo ready", |s| {
                edit_of(s, &file).is_some_and(|e| e.state == EditState::SudoReady(purpose) && e.command.is_some())
            });
            let edit = edit_of(&ready, &file).unwrap();
            let command = edit.command.clone().unwrap();
            assert!(command.starts_with(&format!(" sh '{}/.cache/terminaal/sudo/", state.home)), "{command}");
            assert!(run_pasted(&command, &bin).success());
            remote.send(Command::EditAction { id: edit.id, action: EditAction::SudoStarted });
        };

        remote.send(Command::Edit { path: file.to_str().unwrap().into(), editor: Some("true".into()) });
        let denied = wait_for(&remote, "denied", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Denied(SudoFor::Open)));
        let id = edit_of(&denied, &file).unwrap().id;
        remote.send(Command::EditAction { id, action: EditAction::UseSudo });
        run_ready(SudoFor::Open);
        let opened = wait_for(&remote, "opened via sudo", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Synced));
        let local = edit_of(&opened, &file).unwrap().local.clone();
        assert_eq!(std::fs::read_to_string(&local).unwrap(), "root only\n");
        let sudo_dir = Path::new(&state.home).join(".cache/terminaal/sudo");
        assert_eq!(std::fs::read_dir(&sudo_dir).unwrap().count(), 0, "the script's folder is gone");

        // Saving goes through sudo as well.
        std::fs::write(&local, "edited\n").unwrap();
        run_ready(SudoFor::Save);
        wait_for(&remote, "saved via sudo", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Synced));
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o000, "mode kept");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "edited\n");

        // Changed by someone else, same size and time: the script refuses.
        sneaky_change(&file, b"theirs\n");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        std::fs::write(&local, "mine...\n").unwrap();
        run_ready(SudoFor::Save);
        wait_for(&remote, "sudo conflict", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Conflict));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "theirs\n");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Overwriting goes through without the check.
        remote.send(Command::EditAction { id, action: EditAction::Overwrite });
        run_ready(SudoFor::Save);
        wait_for(&remote, "overwritten via sudo", |s| edit_of(s, &file).is_some_and(|e| e.state == EditState::Synced));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine...\n");

        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
