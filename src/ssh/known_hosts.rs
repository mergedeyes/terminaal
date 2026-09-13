//! A host's entries in `known_hosts`: finding them, to show which key is
//! trusted, and removing them, so a server that really got a new key can
//! be trusted anew.
//!
//! Whether a line is about a host is decided by libssh2, line by line --
//! the same check `connection::verify_host_key` does, so hashed entries
//! count and what's shown is exactly what connecting goes by. Unlike
//! OpenSSH, libssh2 lets a line without a port (`example.com`) vouch for
//! any port, besides `[example.com]:2222` for its own.
//!
//! Removing rewrites the file with just the confirmed lines left out:
//! everything else stays byte for byte, comments and lines libssh2
//! doesn't understand included. The new content goes into a file next to
//! it that replaces the old one only once it's complete.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use ssh2::{CheckResult, KnownHostFileKind, Session};

use super::connection::{base64, base64_decode};
use super::{SshTarget, ssh_dir};

/// One line of `known_hosts` that's about the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// 0-based line number.
    pub line: usize,
    /// The line as it is in the file, without the line break.
    pub text: String,
    /// The names the line is for (comma-separated patterns); `None` when
    /// they're hashed.
    pub hosts: Option<String>,
    /// `ssh-ed25519`, `ecdsa-sha2-nistp256`, ...
    pub key_type: String,
    /// `SHA256:…`, the way OpenSSH shows it.
    pub fingerprint: String,
}

/// The file `target`'s host key is checked against: its
/// `UserKnownHostsFile`, or `~/.ssh/known_hosts`.
pub fn file_for(target: &SshTarget) -> Option<PathBuf> {
    target.settings.known_hosts.clone().or_else(|| Some(ssh_dir()?.join("known_hosts")))
}

/// How `host` and `port` appear in known_hosts: `[host]:port` unless
/// it's port 22.
pub fn entry_name(host: &str, port: u16) -> String {
    if port == 22 { host.to_string() } else { format!("[{host}]:{port}") }
}

/// The lines of `path` for `host` on `port`, first to last. A missing
/// file has none.
pub fn entries(path: &Path, host: &str, port: u16) -> io::Result<Vec<Entry>> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let session = Session::new().map_err(io::Error::other)?;
    let mut found = Vec::new();
    for (line, raw) in lines(&data).enumerate() {
        let Ok(text) = std::str::from_utf8(raw) else { continue };
        let Some(entry) = parse(line, text) else { continue };
        let Some(key) = base64_decode(text.split_whitespace().nth(2).unwrap_or_default()) else { continue };
        // A collection of just this line: does it vouch for the host?
        let mut known = session.known_hosts().map_err(io::Error::other)?;
        if known.read_str(text.trim_end(), KnownHostFileKind::OpenSSH).is_ok()
            && matches!(known.check_port(host, port, &key), CheckResult::Match)
        {
            found.push(entry);
        }
    }
    Ok(found)
}

/// A host key line split up; `None` for comments, blank lines, markers
/// (`@cert-authority`, `@revoked`) and anything else that isn't one.
fn parse(line: usize, text: &str) -> Option<Entry> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('@') {
        return None;
    }
    let mut fields = trimmed.split_whitespace();
    let (hosts, key_type, key) = (fields.next()?, fields.next()?, fields.next()?);
    let key = base64_decode(key).filter(|key| !key.is_empty())?;
    let hash = base64(&Sha256::digest(&key));
    Some(Entry {
        line,
        text: text.trim_end_matches('\r').to_string(),
        hosts: (!hosts.starts_with('|')).then(|| hosts.to_string()),
        key_type: key_type.to_string(),
        fingerprint: format!("SHA256:{}", hash.trim_end_matches('=')),
    })
}

/// The file's lines without their line breaks (`\n`, or `\r\n`).
fn lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split_inclusive(|&b| b == b'\n').map(|line| {
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        line.strip_suffix(b"\r").unwrap_or(line)
    })
}

/// Take `remove` out of `path`. Refused, with nothing changed, if any of
/// them isn't on its line any more -- the file changed since it was read.
/// A symlinked file stays a symlink; its target is rewritten.
pub fn remove(path: &Path, remove: &[Entry]) -> io::Result<()> {
    let path = fs::canonicalize(path)?;
    let data = fs::read(&path)?;
    let lines: Vec<&[u8]> = lines(&data).collect();
    for entry in remove {
        if lines.get(entry.line).is_none_or(|line| *line != entry.text.as_bytes()) {
            return Err(io::Error::other(crate::i18n::t!("known-hosts-changed")));
        }
    }
    let skip: HashSet<usize> = remove.iter().map(|entry| entry.line).collect();
    let kept: Vec<u8> = data
        .split_inclusive(|&b| b == b'\n')
        .enumerate()
        .filter(|(line, _)| !skip.contains(line))
        .flat_map(|(_, bytes)| bytes.iter().copied())
        .collect();

    let mode = fs::metadata(&path)?.permissions().mode() & 0o7777;
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.terminaal-{}", std::process::id()));
    let written = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
        file.write_all(&kept)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        fs::rename(&temp, &path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An Ed25519 public key (libssh2 refuses implausibly short ones) and
    /// its fingerprint.
    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
    const FINGERPRINT: &str = "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU";
    /// `[example.com]:2222`, hashed with the salt 0, 1, ..., 19.
    const HASHED: &str = "|1|AAECAwQFBgcICQoLDA0ODxAREhM=|Wgcx+Fm+LmaWwC7rQ80eIf2uHe0=";

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("terminaal-kh-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(dir: &Path, content: &str) -> PathBuf {
        let path = dir.join("known_hosts");
        fs::write(&path, content).unwrap();
        path
    }

    fn sample() -> String {
        format!(
            "# my servers\n\
             example.com ssh-ed25519 {KEY}\n\
             @cert-authority *.example.com ssh-ed25519 {KEY}\n\
             other.org,example.com ssh-rsa {KEY} laptop\n\
             [example.com]:2222 ssh-ed25519 {KEY}\n\
             {HASHED} ecdsa-sha2-nistp256 {KEY}\r\n\
             not a valid line\n\
             example.comx ssh-ed25519 {KEY}"
        )
    }

    #[test]
    fn finds_the_lines_for_a_host_and_port() {
        let dir = temp_dir("find");
        let path = file(&dir, &sample());

        let plain = entries(&path, "example.com", 22).unwrap();
        assert_eq!(plain.iter().map(|e| e.line).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(plain[0].hosts.as_deref(), Some("example.com"));
        assert_eq!(plain[0].key_type, "ssh-ed25519");
        assert_eq!(plain[0].fingerprint, FINGERPRINT);
        assert_eq!(plain[1].hosts.as_deref(), Some("other.org,example.com"));

        // libssh2 counts the lines without a port for every port.
        let on_port = entries(&path, "example.com", 2222).unwrap();
        assert_eq!(on_port.iter().map(|e| e.line).collect::<Vec<_>>(), [1, 3, 4, 5]);
        assert_eq!(on_port[3].hosts, None);
        assert_eq!(on_port[3].text, format!("{HASHED} ecdsa-sha2-nistp256 {KEY}"));

        assert!(entries(&path, "nowhere.net", 22).unwrap().is_empty());
        assert!(entries(&dir.join("missing"), "example.com", 22).unwrap().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn removing_leaves_every_other_byte_alone() {
        let dir = temp_dir("remove");
        let path = file(&dir, &sample());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let found: Vec<Entry> = entries(&path, "example.com", 2222).unwrap().into_iter().filter(|e| e.line >= 4).collect();
        remove(&path, &found).unwrap();
        let expected = sample().replace(&format!("[example.com]:2222 ssh-ed25519 {KEY}\n{HASHED} ecdsa-sha2-nistp256 {KEY}\r\n"), "");
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o644);
        // The last line, which has no line break, goes; the one before keeps its own.
        let found = entries(&path, "example.comx", 22).unwrap();
        remove(&path, &found).unwrap();
        assert!(fs::read_to_string(&path).unwrap().ends_with("laptop\nnot a valid line\n"));
        // No temporary file left behind.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn refuses_when_the_file_changed_meanwhile() {
        let dir = temp_dir("changed");
        let path = file(&dir, &sample());
        let found = entries(&path, "example.com", 22).unwrap();
        // A line added at the top moves everything down.
        fs::write(&path, format!("new.host ssh-ed25519 {KEY}\n{}", sample())).unwrap();
        assert!(remove(&path, &found).is_err());
        assert!(fs::read_to_string(&path).unwrap().contains(&format!("\nexample.com ssh-ed25519 {KEY}\n")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_symlinked_file_stays_a_symlink() {
        let dir = temp_dir("symlink");
        let real = file(&dir, &sample());
        let link = dir.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let found = entries(&link, "example.com", 22).unwrap();
        remove(&link, &found).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(entries(&link, "example.com", 22).unwrap().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn entry_names_like_openssh() {
        assert_eq!(entry_name("example.com", 22), "example.com");
        assert_eq!(entry_name("10.0.0.1", 2222), "[10.0.0.1]:2222");
    }
}
