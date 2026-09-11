//! The command a new tab runs for a given shell.
//!
//! The interesting part is loading the managed alias/function file
//! (`managed::path_for`) in Terminaal only, without the user adding a
//! `source` line to their own startup files:
//!
//! - fish: `--init-command`, which runs right after fish's own config.
//! - bash: `--rcfile` pointing at a small generated rc that sources
//!   `~/.bashrc` first, then the managed file.
//! - zsh: `ZDOTDIR` pointing at a generated directory whose `.zshenv` and
//!   `.zshrc` hand over to the user's own files and then source the
//!   managed file -- the same trick kitty and ghostty use for their shell
//!   integration. If `/etc/zshenv` overrides `ZDOTDIR` itself, zsh never
//!   sees ours and simply starts without the managed file.
//!
//! The generated files live in `~/.config/terminaal/shell-integration`.
//! Every shell still starts as an interactive non-login shell, same as
//! alacritty_terminal's default.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use super::managed::{self, quote_fish, quote_posix};
use super::{InstalledShell, ShellKind};

/// Program, arguments and extra environment for a new tab's PTY.
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

impl Launch {
    fn plain(shell: &InstalledShell) -> Self {
        Self { program: shell.path.to_string_lossy().into_owned(), args: Vec::new(), env: HashMap::new() }
    }
}

/// Never fails: if the integration files can't be written, the shell
/// just starts without Terminaal's aliases/functions.
pub fn launch(shell: &InstalledShell) -> Launch {
    let Some(file) = managed::path_for(shell.kind) else { return Launch::plain(shell) };
    let result = match shell.kind {
        ShellKind::Fish => Ok(fish(shell, &file)),
        ShellKind::Bash => bash(shell, &file),
        ShellKind::Zsh => zsh(shell, &file),
        ShellKind::Other => Ok(Launch::plain(shell)),
    };
    result.unwrap_or_else(|err| {
        log::warn!("{}: shell integration unavailable, starting without managed aliases: {err}", shell.name);
        Launch::plain(shell)
    })
}

fn fish(shell: &InstalledShell, file: &Path) -> Launch {
    let file = quote_fish(&file.to_string_lossy());
    Launch {
        args: vec!["--init-command".into(), format!("test -f {file}; and source {file}")],
        ..Launch::plain(shell)
    }
}

const BASHRC: &str = "\
# Von Terminaal erzeugt: lädt deine eigene ~/.bashrc, danach die Aliase
# und Funktionen, die Terminaal verwaltet.
[ -f ~/.bashrc ] && . ~/.bashrc
[ -f @FILE@ ] && . @FILE@
";

fn bash(shell: &InstalledShell, file: &Path) -> io::Result<Launch> {
    let rc = integration_dir()?.join("bashrc");
    write_if_changed(&rc, &BASHRC.replace("@FILE@", &quote_posix(&file.to_string_lossy())))?;
    Ok(Launch { args: vec!["--rcfile".into(), rc.to_string_lossy().into_owned()], ..Launch::plain(shell) })
}

const ZSHENV: &str = r##"# Von Terminaal erzeugt: gibt an deine eigene .zshenv weiter.
ZDOTDIR="${TERMINAAL_USER_ZDOTDIR:-$HOME}"
[ -f "$ZDOTDIR/.zshenv" ] && . "$ZDOTDIR/.zshenv"
# Deine .zshenv darf ZDOTDIR selbst ändern: Ergebnis merken und zsh
# für die .zshrc wieder hierher zeigen lassen.
TERMINAAL_USER_ZDOTDIR="$ZDOTDIR"
ZDOTDIR=@DIR@
"##;

const ZSHRC: &str = r##"# Von Terminaal erzeugt: lädt deine eigene .zshrc, danach die Aliase
# und Funktionen, die Terminaal verwaltet.
ZDOTDIR="$TERMINAAL_USER_ZDOTDIR"
unset TERMINAAL_USER_ZDOTDIR
[ "$ZDOTDIR" = "$HOME" ] && unset ZDOTDIR
[ -f "${ZDOTDIR:-$HOME}/.zshrc" ] && . "${ZDOTDIR:-$HOME}/.zshrc"
[ -f @FILE@ ] && . @FILE@
"##;

fn zsh(shell: &InstalledShell, file: &Path) -> io::Result<Launch> {
    let dir = integration_dir()?.join("zsh");
    write_if_changed(&dir.join(".zshenv"), &ZSHENV.replace("@DIR@", &quote_posix(&dir.to_string_lossy())))?;
    write_if_changed(&dir.join(".zshrc"), &ZSHRC.replace("@FILE@", &quote_posix(&file.to_string_lossy())))?;
    let user_zdotdir = super::zdotdir().ok_or_else(|| io::Error::other("$HOME is not set"))?;

    let mut launch = Launch::plain(shell);
    launch.env.insert("ZDOTDIR".into(), dir.to_string_lossy().into_owned());
    launch.env.insert("TERMINAAL_USER_ZDOTDIR".into(), user_zdotdir.to_string_lossy().into_owned());
    Ok(launch)
}

fn integration_dir() -> io::Result<PathBuf> {
    let config = super::xdg_config_home().ok_or_else(|| io::Error::other("$HOME is not set"))?;
    Ok(config.join("terminaal/shell-integration"))
}

/// Skips the write when nothing changed, so opening a tab doesn't touch
/// the disk every time.
fn write_if_changed(path: &Path, content: &str) -> io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == content) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_sources_the_managed_file_via_init_command() {
        let launch = fish(&InstalledShell::new("/usr/bin/fish"), Path::new("/home/u/.config/fish/terminaal.fish"));
        assert_eq!(launch.program, "/usr/bin/fish");
        assert_eq!(
            launch.args,
            [
                "--init-command",
                "test -f '/home/u/.config/fish/terminaal.fish'; and source '/home/u/.config/fish/terminaal.fish'"
            ]
        );
    }
}
