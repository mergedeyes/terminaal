//! Your own commands ("snippets"), next to the built-in ones in the
//! sidebar: a name and a command line -- or several lines -- optionally
//! only for one system ([`Family`]) or one host.
//!
//! Stored in `~/.config/terminaal/snippets.toml`, written whole like the
//! host and key stores (first to a file next to it, then renamed):
//!
//! ```toml
//! [[snippet]]
//! name = "Logs"
//! command = "journalctl -f"
//! system = "arch"   # optional, a `Family` key
//! host = "web1"     # optional, a host's name in the sidebar
//! ```

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::commands::{Family, Target};
use crate::config::Config;
use crate::i18n::t;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub name: String,
    pub command: String,
    /// Only on this system (a [`Family`] key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Only on this host (its name in the sidebar); never in a local tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

impl Snippet {
    /// The system it's bound to, if it's one we know.
    pub fn family(&self) -> Option<Family> {
        self.system.as_deref().and_then(Family::parse).filter(|family| *family != Family::Unknown)
    }

    /// Whether its button shows for the tab `target` describes. A system
    /// not detected yet (or not known) matches no bound snippet.
    pub fn applies(&self, target: &Target) -> bool {
        let system = match &self.system {
            None => true,
            Some(_) => self.family().is_some_and(|family| target.system.is_some_and(|system| system.family == family)),
        };
        let host = match &self.host {
            None => true,
            Some(host) => target.host_name.as_deref() == Some(host.as_str()),
        };
        system && host
    }
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "snippet")]
    snippets: Vec<Snippet>,
}

fn path() -> Option<PathBuf> {
    Some(Config::dir()?.join("snippets.toml"))
}

/// The saved snippets; none if there's no file yet.
pub fn load() -> Result<Vec<Snippet>, String> {
    let Some(path) = path() else { return Ok(Vec::new()) };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(t!("common-file-unreadable", path = path.display().to_string(), err = err.to_string())),
    };
    parse(&text).map_err(|err| t!("common-file-invalid", path = path.display().to_string(), err = err))
}

fn parse(text: &str) -> Result<Vec<Snippet>, String> {
    toml::from_str::<File>(text).map(|file| file.snippets).map_err(|err| err.to_string())
}

pub fn save(snippets: &[Snippet]) -> Result<(), String> {
    let path = path().ok_or_else(|| t!("common-home-unset"))?;
    let text = toml::to_string_pretty(&File { snippets: snippets.to_vec() }).map_err(|err| err.to_string())?;
    crate::ssh::write_atomically(&path, &text).map_err(|err| format!("{}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::System;

    fn target(family: Family, host: Option<&str>) -> Target {
        Target { system: Some(System { family, root: false }), host_name: host.map(str::to_string), ..Target::default() }
    }

    #[test]
    fn reads_and_writes_the_file_format() {
        let text = "[[snippet]]\nname = \"Logs\"\ncommand = \"journalctl -f\"\nsystem = \"arch\"\n\n\
                    [[snippet]]\nname = \"Deploy\"\ncommand = \"cd /srv\\n./deploy.sh\"\nhost = \"web1\"\n";
        let snippets = parse(text).unwrap();
        assert_eq!(snippets.len(), 2);
        assert_eq!(snippets[0].family(), Some(Family::Arch));
        assert_eq!(snippets[1].command, "cd /srv\n./deploy.sh");
        let written = toml::to_string_pretty(&File { snippets: snippets.clone() }).unwrap();
        assert_eq!(parse(&written).unwrap(), snippets);
        assert!(!written.contains("host = \"\""), "unset fields stay out: {written}");
        assert_eq!(parse("").unwrap(), []);
        assert!(parse("[[snippet]]\nname = 1").is_err());
    }

    #[test]
    fn shows_only_where_it_belongs() {
        let any = Snippet { name: "a".into(), command: "ls".into(), ..Snippet::default() };
        let arch = Snippet { system: Some("arch".into()), ..any.clone() };
        let web1 = Snippet { host: Some("web1".into()), ..any.clone() };
        let local_arch = target(Family::Arch, None);
        let web1_debian = target(Family::Debian, Some("web1"));

        assert!(any.applies(&local_arch) && any.applies(&web1_debian) && any.applies(&Target::default()));
        assert!(arch.applies(&local_arch) && !arch.applies(&web1_debian));
        assert!(!arch.applies(&Target::default()), "system not known yet");
        assert!(!web1.applies(&local_arch) && web1.applies(&web1_debian));
        let unknown_system = Snippet { system: Some("plan9".into()), ..any };
        assert!(!unknown_system.applies(&local_arch));
    }
}
