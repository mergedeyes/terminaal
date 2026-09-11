//! Installed shells and what Terminaal manages for each of them.
//!
//! - [`detect`] reads `/etc/shells` into a deduplicated list of
//!   [`InstalledShell`]s for the sidebar's shell picker.
//! - [`managed`] reads/writes the per-shell file of aliases and
//!   functions that the sidebar edits.
//! - [`launch`] builds the command a new tab runs, wiring that file in
//!   so it's loaded in Terminaal only -- the user's own `config.fish`,
//!   `.bashrc` or `.zshrc` are never touched.

pub mod launch;
pub mod managed;

use std::path::{Path, PathBuf};

/// Shells whose alias/function syntax Terminaal knows how to write.
/// Everything else can still be picked and launched, just not managed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    Fish,
    Bash,
    Zsh,
    Other,
}

impl ShellKind {
    /// By binary name. `sh` is deliberately `Other` even where it's bash
    /// underneath: in sh mode bash doesn't read any rc file.
    fn from_name(name: &str) -> Self {
        match name {
            "fish" => Self::Fish,
            "bash" => Self::Bash,
            "zsh" => Self::Zsh,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InstalledShell {
    /// Binary name, e.g. `fish` -- what the sidebar and tab titles show.
    pub name: String,
    /// Path as listed in `/etc/shells` (or configured).
    pub path: PathBuf,
    pub kind: ShellKind,
    /// `path` with symlinks resolved, to recognise the same shell listed
    /// under several paths (`/bin/bash` vs `/usr/bin/bash`).
    canonical: PathBuf,
}

impl InstalledShell {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        Self::from_parts(path, canonical)
    }

    fn from_parts(path: PathBuf, canonical: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Self { kind: ShellKind::from_name(&name), name, path, canonical }
    }

    /// Whether both refer to the same shell, e.g. `/usr/bin/zsh` and
    /// `/bin/zsh` on a usrmerge system. The name has to match too:
    /// `/bin/sh` may resolve to bash's binary but behaves differently.
    pub fn is(&self, other: &InstalledShell) -> bool {
        self.name == other.name && self.canonical == other.canonical
    }
}

/// Listed in `/etc/shells` for login/access control, not something to
/// run in a terminal tab.
const NOT_INTERACTIVE: &[&str] = &["nologin", "false", "git-shell", "systemd-home-fallback-shell"];

/// All interactive shells from `/etc/shells` that actually exist,
/// deduplicated, with the ones Terminaal can manage listed first.
pub fn detect() -> Vec<InstalledShell> {
    let listing = std::fs::read_to_string("/etc/shells").unwrap_or_else(|err| {
        log::warn!("failed to read /etc/shells: {err}");
        String::new()
    });
    let shells = parse_etc_shells(&listing).into_iter().filter(|p| p.is_file()).map(InstalledShell::new).collect();
    let mut shells = dedup(shells);
    shells.sort_by_key(|s| s.kind == ShellKind::Other);
    shells
}

/// The shell new tabs use unless told otherwise: the configured one,
/// else `$SHELL`, else `/bin/sh`.
pub fn default_shell(configured: Option<&Path>) -> InstalledShell {
    let path = configured
        .map(Path::to_path_buf)
        .filter(|p| p.is_file())
        .or_else(|| std::env::var_os("SHELL").map(PathBuf::from).filter(|p| p.is_file()))
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    InstalledShell::new(path)
}

fn parse_etc_shells(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .filter(|path| path.file_name().is_some_and(|n| !NOT_INTERACTIVE.iter().any(|x| n == *x)))
        .collect()
}

/// Keeps the first-listed path of each shell.
fn dedup(shells: Vec<InstalledShell>) -> Vec<InstalledShell> {
    let mut out: Vec<InstalledShell> = Vec::new();
    for shell in shells {
        if !out.iter().any(|seen| seen.is(&shell)) {
            out.push(shell);
        }
    }
    out
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

fn xdg_config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| Some(home()?.join(".config")))
}

/// Where zsh looks for its dotfiles.
fn zdotdir() -> Option<PathBuf> {
    std::env::var_os("ZDOTDIR").filter(|d| !d.is_empty()).map(PathBuf::from).or_else(home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_etc_shells_skipping_comments_and_non_interactive() {
        let text = "# Pathnames of valid login shells.\n\n/bin/sh\n/usr/bin/git-shell\n  /usr/bin/fish \n/usr/bin/nologin\n";
        assert_eq!(parse_etc_shells(text), vec![PathBuf::from("/bin/sh"), PathBuf::from("/usr/bin/fish")]);
    }

    #[test]
    fn dedup_merges_same_binary_but_keeps_sh_mode() {
        let shell = |path: &str, canonical: &str| InstalledShell::from_parts(path.into(), canonical.into());
        let out = dedup(vec![
            shell("/bin/bash", "/usr/bin/bash"),
            shell("/usr/bin/bash", "/usr/bin/bash"),
            shell("/bin/sh", "/usr/bin/bash"),
        ]);
        let names: Vec<_> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["bash", "sh"]);
        assert_eq!(out[0].path, PathBuf::from("/bin/bash"));
        assert_eq!(out[0].kind, ShellKind::Bash);
        assert_eq!(out[1].kind, ShellKind::Other);
    }
}
