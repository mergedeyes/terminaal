//! The key store: SSH keys under names of the user's choosing
//! (`~/.config/terminaal/keys.toml`), which hosts refer to by name.
//!
//! A key is either a private key file (generated here, or an existing
//! one) or a key that only lives in the SSH agent -- e.g. 1Password --
//! identified by its public half. `keys.toml` never holds anything
//! secret: generated private keys go to their own file, encrypted if a
//! passphrase was given, exactly like `ssh-keygen` would write them.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};

use super::connection::base64;
use super::{expand_tilde, write_atomically};
use crate::i18n::t;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    pub name: String,
    /// Private key file (`~/` allowed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Public key in OpenSSH format, for a key held only by the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_key: Option<String>,
    /// The key pair was generated here. Only then does removing the key
    /// offer to delete its files as well; files added from elsewhere are
    /// never touched.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
}

/// What the UI shows about a key.
#[derive(Clone, Debug)]
pub struct KeyInfo {
    /// e.g. `ssh-ed25519`.
    pub algorithm: String,
    /// `SHA256:…`, as `ssh-keygen -l` prints it.
    pub fingerprint: String,
    pub comment: String,
    /// The line for a server's `authorized_keys`.
    pub public_openssh: String,
}

impl KeyInfo {
    fn of(key: &PublicKey) -> Self {
        Self {
            algorithm: key.algorithm().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            comment: key.comment().to_string(),
            public_openssh: key.to_openssh().unwrap_or_default(),
        }
    }
}

impl Key {
    pub fn file_path(&self) -> Option<PathBuf> {
        self.file.as_deref().map(expand_tilde)
    }

    pub fn public_key(&self) -> Result<PublicKey, String> {
        match (&self.agent_key, self.file_path()) {
            (Some(text), _) => PublicKey::from_openssh(text)
                .map_err(|err| t!("keystore-public-invalid", name = &self.name, err = err.to_string())),
            (None, Some(path)) => public_key_of_file(&path),
            (None, None) => Err(t!("keystore-no-source", name = &self.name)),
        }
    }

    /// SSH wire format, as the agent and libssh2 compare keys.
    pub fn public_blob(&self) -> Result<Vec<u8>, String> {
        self.public_key()?.to_bytes().map_err(|err| err.to_string())
    }

    pub fn info(&self) -> Result<KeyInfo, String> {
        self.public_key().map(|key| KeyInfo::of(&key))
    }
}

/// From `<path>.pub` if there is one, else from the private key file
/// itself -- OpenSSH-format keys carry their public half unencrypted, so
/// this works without the passphrase.
pub fn public_key_of_file(path: &Path) -> Result<PublicKey, String> {
    if let Ok(key) = PublicKey::read_openssh_file(&public_path(path)) {
        return Ok(key);
    }
    PrivateKey::read_openssh_file(path)
        .map(|key| key.public_key().clone())
        .map_err(|err| t!("keystore-unreadable-file", path = path.display().to_string(), err = err.to_string()))
}

pub fn public_blob_of_file(path: &Path) -> Result<Vec<u8>, String> {
    public_key_of_file(path)?.to_bytes().map_err(|err| err.to_string())
}

fn public_path(path: &Path) -> PathBuf {
    let mut public = path.as_os_str().to_owned();
    public.push(".pub");
    PathBuf::from(public)
}

/// Write a new Ed25519 key pair to `path` and `path.pub`, encrypted with
/// `passphrase` unless that's empty. Never overwrites anything.
pub fn generate(path: &Path, comment: &str, passphrase: &str) -> Result<PublicKey, String> {
    let fail = |err: &dyn std::fmt::Display| t!("keystore-generate-failed", err = err.to_string());
    let public_path = public_path(path);
    for existing in [path, public_path.as_path()] {
        if existing.exists() {
            return Err(t!("keystore-exists", path = existing.display().to_string()));
        }
    }

    let mut key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).map_err(|e| fail(&e))?;
    key.set_comment(comment);
    let mut public = key.public_key().clone();
    public.set_comment(comment);
    let key = if passphrase.is_empty() { key } else { key.encrypt(&mut OsRng, passphrase).map_err(|e| fail(&e))? };
    let pem = key.to_openssh(LineEnding::LF).map_err(|e| fail(&e))?;

    if let Some(dir) = path.parent()
        && !dir.exists()
    {
        std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| fail(&e))?;
    }
    // Private from the first byte on (0600), and `create_new` so a file
    // appearing in the meantime is never clobbered.
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(pem.as_bytes()))
        .map_err(|e| fail(&e))?;
    let line = public.to_openssh().map_err(|e| fail(&e))?;
    std::fs::write(&public_path, format!("{line}\n")).map_err(|e| fail(&e))?;
    Ok(public)
}

/// Those of `path` and `path.pub` that exist.
pub fn existing_files(path: &Path) -> Vec<PathBuf> {
    [path.to_path_buf(), public_path(path)].into_iter().filter(|file| file.exists()).collect()
}

/// Delete a key pair's files; ones already gone don't count as an error.
pub fn delete_files(files: &[PathBuf]) -> Result<(), String> {
    for file in files {
        match std::fs::remove_file(file) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                return Err(t!("keystore-delete-failed", path = file.display().to_string(), err = err.to_string()));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The keys in the SSH agent (`$SSH_AUTH_SOCK`), to add one to the store.
/// Only lists them -- nothing gets signed, so the agent doesn't ask.
pub fn agent_identities() -> Result<Vec<KeyInfo>, String> {
    if std::env::var_os("SSH_AUTH_SOCK").is_none() {
        return Err(t!("keystore-no-agent"));
    }
    let fail = |err: ssh2::Error| t!("keystore-agent-unreachable", err = err.to_string());
    let session = ssh2::Session::new().map_err(fail)?;
    let mut agent = session.agent().map_err(fail)?;
    agent.connect().map_err(fail)?;
    agent.list_identities().map_err(fail)?;
    let identities = agent.identities().map_err(fail)?;
    let _ = agent.disconnect();
    Ok(identities
        .iter()
        .filter_map(|identity| {
            let algorithm = blob_algorithm(identity.blob())?;
            let line = format!("{algorithm} {} {}", base64(identity.blob()), identity.comment());
            PublicKey::from_openssh(line.trim()).ok().map(|key| KeyInfo::of(&key))
        })
        .collect())
}

/// A public key blob starts with its algorithm name as an SSH string.
fn blob_algorithm(blob: &[u8]) -> Option<&str> {
    let (len, rest) = blob.split_first_chunk::<4>()?;
    std::str::from_utf8(rest.get(..u32::from_be_bytes(*len) as usize)?).ok()
}

fn keys_path() -> Option<PathBuf> {
    Some(crate::config::Config::dir()?.join("keys.toml"))
}

#[derive(Default, Serialize, Deserialize)]
struct KeysFile {
    #[serde(default, rename = "key")]
    keys: Vec<Key>,
}

/// Errors are meant for display in the UI.
pub fn load_keys() -> Result<Vec<Key>, String> {
    let Some(path) = keys_path() else { return Ok(Vec::new()) };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    toml::from_str::<KeysFile>(&text)
        .map(|file| file.keys)
        .map_err(|err| t!("common-file-invalid", path = path.display().to_string(), err = err.to_string()))
}

pub fn save_keys(keys: &[Key]) -> Result<(), String> {
    let path = keys_path().ok_or_else(|| t!("common-home-unset"))?;
    let text = toml::to_string_pretty(&KeysFile { keys: keys.to_vec() }).map_err(|err| err.to_string())?;
    write_atomically(&path, &text).map_err(|err| format!("{}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("terminaal-keys-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn generates_encrypted_keys_whose_public_half_is_readable_without_passphrase() {
        let dir = temp_dir("gen");
        let path = dir.join("id_test");
        let public = generate(&path, "me@test", "geheim").unwrap();

        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(PrivateKey::read_openssh_file(&path).unwrap().is_encrypted());
        // …and the connection code's own check agrees, so it will ask for the passphrase.
        assert!(crate::ssh::connection::key_is_encrypted(&std::fs::read_to_string(&path).unwrap()));

        // OpenSSH's own tool can decrypt it and derives the same public key.
        if let Ok(out) = std::process::Command::new("ssh-keygen").args(["-y", "-P", "geheim", "-f"]).arg(&path).output() {
            let derived = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success(), "ssh-keygen: {}", String::from_utf8_lossy(&out.stderr));
            let base = public.to_openssh().unwrap();
            let base = base.rsplit_once(' ').map_or(base.as_str(), |(key, _comment)| key);
            assert!(derived.starts_with(base), "{derived} vs {base}");
        }

        let key = Key { name: "t".into(), file: Some(path.display().to_string()), agent_key: None, generated: true };
        assert_eq!(key.info().unwrap().comment, "me@test");

        // Without the .pub file, the private file alone still gives the
        // public key -- but not the comment, which OpenSSH keeps in the
        // encrypted part.
        std::fs::remove_file(public_path(&path)).unwrap();
        let info = key.info().unwrap();
        assert_eq!(info.fingerprint, public.fingerprint(HashAlg::Sha256).to_string());
        assert_eq!((info.algorithm.as_str(), info.comment.as_str()), ("ssh-ed25519", ""));
        assert_eq!(key.public_blob().unwrap(), public.to_bytes().unwrap());

        // Never overwrites.
        assert!(generate(&path, "again", "").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn deletes_both_files_of_a_key_pair() {
        let dir = temp_dir("delete");
        let path = dir.join("id_gone");
        generate(&path, "", "").unwrap();
        let files = existing_files(&path);
        assert_eq!(files, vec![path.clone(), dir.join("id_gone.pub")]);
        delete_files(&files).unwrap();
        assert!(existing_files(&path).is_empty());
        // Already gone is fine.
        delete_files(&files).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn agent_keys_are_identified_by_their_public_half() {
        let dir = temp_dir("agent");
        let public = generate(&dir.join("k"), "agent-held", "").unwrap();
        let key = Key { name: "a".into(), file: None, agent_key: Some(public.to_openssh().unwrap()), generated: false };
        assert_eq!(key.public_blob().unwrap(), public.to_bytes().unwrap());
        assert_eq!(blob_algorithm(&public.to_bytes().unwrap()), Some("ssh-ed25519"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keys_file_round_trips() {
        let keys = vec![
            Key { name: "work".into(), file: Some("~/.ssh/id_work".into()), agent_key: None, generated: true },
            Key { name: "1password".into(), file: None, agent_key: Some("ssh-ed25519 AAAA x".into()), generated: false },
        ];
        let text = toml::to_string_pretty(&KeysFile { keys: keys.clone() }).unwrap();
        assert!(text.contains("[[key]]"));
        // Only written where it's set.
        assert_eq!(text.matches("generated").count(), 1, "{text}");
        assert_eq!(toml::from_str::<KeysFile>(&text).unwrap().keys, keys);
    }
}
