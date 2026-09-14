//! The session: which tabs were open, how their panes were split, what
//! each terminal ran and where -- so the next start can open them again.
//!
//! Stored in `$XDG_STATE_HOME/terminaal/session.toml` (else
//! `~/.local/state/terminaal`), written whole (to a file next to it, then
//! renamed) a moment after something changed and when the window closes.
//! Closing the last tab leaves nothing to restore: the file goes.
//!
//! Only one running Terminaal owns the session ([`Lock`]): a second window
//! starts with a fresh tab and doesn't write -- otherwise it would open
//! every connection of the first one again and the two would overwrite
//! each other's tabs.
//!
//! Nothing but names and paths: no scrollback, nothing typed. An SSH
//! terminal is its host's name in the sidebar and the user, resolved
//! through the [`Catalog`] again at start, so changed host settings apply.
//!
//! ```toml
//! active = 0
//!
//! [[tab]]
//! kind = "terminals"
//! focus = 1
//! layout = { axis = "horizontal", ratio = 0.5, first = 0, second = 1 }
//!
//! [[tab.panes]]
//! kind = "shell"
//! shell = "/usr/bin/fish"
//! cwd = "/home/me/src"
//!
//! [[tab.panes]]
//! kind = "ssh"
//! host = "web1"
//! user = "admin"
//! ```

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::panes::Node;
use crate::ssh::{Catalog, Host, SshTarget};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Index of the tab in view.
    #[serde(default)]
    pub active: usize,
    #[serde(default, rename = "tab", skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<SavedTab>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SavedTab {
    Terminals {
        /// The split tree; its leaves are indices into `panes`.
        layout: Node,
        /// Index of the pane with the keyboard.
        #[serde(default)]
        focus: usize,
        #[serde(default, skip_serializing_if = "is_false")]
        zoomed: bool,
        panes: Vec<SavedPane>,
    },
    Settings,
    /// A files tab: it goes over a restored terminal logged in to this host
    /// as this user, if there is one.
    Files {
        host: String,
        user: String,
        /// The folder it showed; empty for the home folder.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        dir: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SavedPane {
    Shell {
        shell: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<PathBuf>,
    },
    Ssh {
        /// The host's name in the sidebar.
        host: String,
        user: String,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl SavedPane {
    pub fn ssh(target: &SshTarget) -> Self {
        Self::Ssh { host: target.name.clone(), user: target.user.clone() }
    }
}

impl SavedTab {
    /// The layout, if it names every pane exactly once -- a file edited by
    /// hand might not.
    pub fn checked_layout(layout: &Node, panes: usize) -> Option<Node> {
        let mut ids = layout.ids();
        ids.sort_unstable();
        (panes > 0 && ids.iter().copied().eq(0..panes)).then(|| {
            let mut layout = layout.clone();
            layout.clamp_ratios();
            layout
        })
    }
}

impl Session {
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|err| err.to_string())
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self).map_err(|err| err.to_string())
    }

    /// The saved session at `path`; `None` if there's none or it can't be
    /// read (logged -- a broken file shouldn't stop the terminal).
    pub fn load(path: &Path) -> Option<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
            Err(err) => {
                log::warn!("failed to read session at {}: {err}", path.display());
                return None;
            }
        };
        Self::parse(&text).inspect_err(|err| log::warn!("failed to parse session at {}: {err}", path.display())).ok()
    }

    /// Write it to `path`; an empty session removes the file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if self.is_empty() {
            return match std::fs::remove_file(path) {
                Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
                _ => Ok(()),
            };
        }
        let text = self.to_toml().map_err(io::Error::other)?;
        crate::ssh::write_atomically(path, &text)
    }
}

/// `$XDG_STATE_HOME/terminaal`, else `~/.local/state/terminaal`.
pub fn dir() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|dir| dir.is_absolute());
    let state = state.or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/state")))?;
    Some(state.join("terminaal"))
}

/// The host a saved SSH terminal logs in to: `host`'s login as `user`, or
/// its default login under that user name if it has none as `user`.
pub fn ssh_target(catalog: &Catalog, host: &str, user: &str) -> Result<SshTarget, String> {
    let found = catalog.find(host).ok_or_else(|| format!("no host named {host:?}"))?;
    let local = crate::ssh::local_user();
    let index = found.all_logins().iter().position(|login| login.user == user || (login.user.is_empty() && user == local));
    let host = match index {
        Some(index) => found.with_login(index).expect("index comes from all_logins"),
        None => Host { user: user.to_string(), logins: Vec::new(), ..found.clone() },
    };
    catalog.target(&host)
}

/// Being the Terminaal that restores and saves the session: an exclusive
/// `flock` on `session.lock`, held while this lives -- and let go by the
/// kernel however the process ends.
pub struct Lock {
    _file: File,
}

impl Lock {
    /// `None` if another Terminaal holds it (or the file can't be opened).
    /// `name`: which session, `session` or `quake-session`.
    pub fn acquire(dir: &Path, name: &str) -> Option<Self> {
        if let Err(err) = std::fs::create_dir_all(dir) {
            log::warn!("no session: can't create {}: {err}", dir.display());
            return None;
        }
        let path = dir.join(format!("{name}.lock"));
        let file = match File::options().create(true).truncate(false).write(true).open(&path) {
            Ok(file) => file,
            Err(err) => {
                log::warn!("no session: can't open {}: {err}", path.display());
                return None;
            }
        };
        // SAFETY: a valid descriptor, owned by `file` for the call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            log::info!("another Terminaal owns the session; starting fresh");
            return None;
        }
        Some(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panes::Axis;

    fn sample() -> Session {
        let mut layout = Node::Leaf(0);
        layout.split(0, 1, Axis::Horizontal);
        layout.split(1, 2, Axis::Vertical);
        layout.set_ratio(0, 0.3333333);
        Session {
            active: 1,
            tabs: vec![
                SavedTab::Terminals {
                    layout,
                    focus: 2,
                    zoomed: true,
                    panes: vec![
                        SavedPane::Shell { shell: "/usr/bin/fish".into(), cwd: Some("/home/me/src".into()) },
                        SavedPane::Shell { shell: "/bin/bash".into(), cwd: None },
                        SavedPane::Ssh { host: "web1".into(), user: "admin".into() },
                    ],
                },
                SavedTab::Settings,
                SavedTab::Files { host: "web1".into(), user: "admin".into(), dir: "/srv".into() },
            ],
        }
    }

    #[test]
    fn reads_back_what_it_writes() {
        let session = sample();
        let text = session.to_toml().unwrap();
        let read = Session::parse(&text).unwrap();
        // The ratio comes back rounded to a thousandth.
        let SavedTab::Terminals { layout: Node::Split { ratio, .. }, .. } = &read.tabs[0] else { panic!("{text}") };
        assert_eq!(*ratio, 0.333, "{text}");
        let mut expected = sample();
        let SavedTab::Terminals { layout, .. } = &mut expected.tabs[0] else { unreachable!() };
        layout.set_ratio(0, 0.333);
        assert_eq!(read, expected, "{text}");
        assert!(!text.contains("cwd = \"\""), "unset fields stay out: {text}");
    }

    #[test]
    fn reads_the_documented_format() {
        let text = "active = 0\n\n[[tab]]\nkind = \"terminals\"\nfocus = 1\n\
                    layout = { axis = \"horizontal\", ratio = 0.5, first = 0, second = 1 }\n\n\
                    [[tab.panes]]\nkind = \"shell\"\nshell = \"/usr/bin/fish\"\ncwd = \"/home/me/src\"\n\n\
                    [[tab.panes]]\nkind = \"ssh\"\nhost = \"web1\"\nuser = \"admin\"\n";
        let session = Session::parse(text).unwrap();
        let SavedTab::Terminals { layout, focus, zoomed, panes } = &session.tabs[0] else { panic!() };
        assert_eq!((layout.ids(), *focus, *zoomed, panes.len()), (vec![0, 1], 1, false, 2));
        assert_eq!(Session::parse("").unwrap(), Session::default());
        assert!(Session::parse("[[tab]]\nkind = \"spaceship\"").is_err());
    }

    #[test]
    fn reads_the_example_in_the_guide() {
        let guide = include_str!("../docs/guides/sessions.md");
        let example = guide.split("```toml\n").nth(1).and_then(|rest| rest.split("```").next()).expect("example");
        let session = Session::parse(example).unwrap();
        assert_eq!(session.tabs.len(), 3);
        let SavedTab::Terminals { layout, focus, panes, .. } = &session.tabs[0] else { panic!() };
        assert!(SavedTab::checked_layout(layout, panes.len()).is_some());
        assert_eq!(*focus, 1);
        assert_eq!(session.tabs[1], SavedTab::Files { host: "web1".into(), user: "admin".into(), dir: "/var/log".into() });
        assert_eq!(session.tabs[2], SavedTab::Settings);
    }

    #[test]
    fn a_layout_must_name_every_pane_once() {
        let mut layout = Node::Leaf(0);
        layout.split(0, 1, Axis::Vertical);
        assert!(SavedTab::checked_layout(&layout, 2).is_some());
        assert!(SavedTab::checked_layout(&layout, 3).is_none(), "a pane left out");
        assert!(SavedTab::checked_layout(&Node::Leaf(0), 0).is_none());
        let mut twice = Node::Leaf(1);
        twice.split(1, 1, Axis::Vertical);
        assert!(SavedTab::checked_layout(&twice, 2).is_none());
        let wild = Session::parse("[[tab]]\nkind = \"terminals\"\nlayout = { axis = \"vertical\", ratio = 7.0, first = 0, second = 1 }\npanes = []").unwrap();
        let SavedTab::Terminals { layout, .. } = &wild.tabs[0] else { panic!() };
        let Some(Node::Split { ratio, .. }) = SavedTab::checked_layout(layout, 2) else { panic!() };
        assert_eq!(ratio, 1.0);
    }

    #[test]
    fn saving_nothing_removes_the_file() {
        let dir = std::env::temp_dir().join(format!("terminaal-session-{}", std::process::id()));
        let path = dir.join("session.toml");
        sample().save(&path).unwrap();
        assert_eq!(Session::load(&path), Some(sample()).map(|mut s| {
            let SavedTab::Terminals { layout, .. } = &mut s.tabs[0] else { unreachable!() };
            layout.set_ratio(0, 0.333);
            s
        }));
        Session::default().save(&path).unwrap();
        assert!(!path.exists());
        Session::default().save(&path).unwrap();
        std::fs::write(&path, "tab = 3").unwrap();
        assert_eq!(Session::load(&path), None, "broken file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_one_terminaal_holds_the_lock() {
        let dir = std::env::temp_dir().join(format!("terminaal-lock-{}", std::process::id()));
        let first = Lock::acquire(&dir, "session").expect("free");
        assert!(Lock::acquire(&dir, "session").is_none(), "held");
        assert!(Lock::acquire(&dir, "quake-session").is_some(), "another session");
        drop(first);
        assert!(Lock::acquire(&dir, "session").is_some(), "free again");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ssh_terminals_find_their_login() {
        let mut web = Host::new("web1", "web1.example.com");
        web.user = "deploy".into();
        web.logins = vec![crate::ssh::Login { user: "admin".into(), key: None }];
        let catalog = Catalog { saved: vec![web], ..Catalog::default() };
        assert_eq!(ssh_target(&catalog, "web1", "admin").unwrap().user, "admin");
        assert_eq!(ssh_target(&catalog, "web1", "deploy").unwrap().user, "deploy");
        let other = ssh_target(&catalog, "web1", "guest").unwrap();
        assert_eq!((other.user.as_str(), other.name.as_str()), ("guest", "web1"));
        assert!(ssh_target(&catalog, "gone", "admin").is_err());
    }
}
