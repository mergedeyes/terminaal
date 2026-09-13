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
//! The same files set up shell integration (`terminal::integration`):
//! the working directory (OSC 7) whenever the prompt comes, and prompt
//! marks (OSC 133) -- `A` before the prompt, `C` when a command starts,
//! `D;status` when it's done. Other terminals never see any of it.
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
        ShellKind::Fish => fish(shell, &file),
        ShellKind::Bash => bash(shell, &file),
        ShellKind::Zsh => zsh(shell, &file),
        ShellKind::Other => Ok(Launch::plain(shell)),
    };
    result.unwrap_or_else(|err| {
        log::warn!("{}: shell integration unavailable, starting without managed aliases: {err}", shell.name);
        Launch::plain(shell)
    })
}

const FISH_INTEGRATION: &str = r#"# Von Terminaal erzeugt: meldet Arbeitsverzeichnis (OSC 7) und Prompts
# (OSC 133) an Terminaal. Ab fish 4 tut fish das selbst.
if not set -q __terminaal_integration; and test (string split -f1 . -- $version) -lt 4
    set -g __terminaal_integration 1
    function __terminaal_cwd --on-event fish_prompt
        printf '\e]7;file://%s%s\a' $hostname (string escape --style=url -- $PWD)
    end
    function __terminaal_prompt_start --on-event fish_prompt
        printf '\e]133;A\a'
    end
    function __terminaal_command_start --on-event fish_preexec
        printf '\e]133;C\a'
    end
    function __terminaal_command_end --on-event fish_postexec
        printf '\e]133;D;%s\a' $status
    end
end
"#;

fn fish(shell: &InstalledShell, file: &Path) -> io::Result<Launch> {
    let integration = integration_dir()?.join("terminaal.fish");
    write_if_changed(&integration, FISH_INTEGRATION)?;
    Ok(fish_launch(shell, &integration, file))
}

fn fish_launch(shell: &InstalledShell, integration: &Path, file: &Path) -> Launch {
    let integration = quote_fish(&integration.to_string_lossy());
    let file = quote_fish(&file.to_string_lossy());
    Launch {
        args: vec!["--init-command".into(), format!("source {integration}; test -f {file}; and source {file}")],
        ..Launch::plain(shell)
    }
}

const BASHRC: &str = "\
# Von Terminaal erzeugt: lädt deine eigene ~/.bashrc, danach die Aliase
# und Funktionen, die Terminaal verwaltet.
[ -f ~/.bashrc ] && . ~/.bashrc
[ -f @FILE@ ] && . @FILE@

# Arbeitsverzeichnis (OSC 7) und Prompts (OSC 133) an Terminaal melden.
if [ -z \"$__terminaal_integration\" ]; then
    __terminaal_integration=1
    __terminaal_prompt() {
        local ret=$?
        local cwd=\"${PWD//%/%25}\"
        printf '\\e]133;D;%s\\a\\e]7;file://%s%s\\a\\e]133;A\\a' \"$ret\" \"$HOSTNAME\" \"${cwd// /%20}\"
        return $ret
    }
    # First, so it still sees the command's status.
    if [[ \"$(declare -p PROMPT_COMMAND 2>/dev/null)\" == 'declare -a'* ]]; then
        PROMPT_COMMAND=(__terminaal_prompt \"${PROMPT_COMMAND[@]}\")
    else
        PROMPT_COMMAND=\"__terminaal_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}\"
    fi
    PS0=\"${PS0}\\e]133;C\\a\"
fi
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

# Arbeitsverzeichnis (OSC 7) und Prompts (OSC 133) an Terminaal melden.
if [[ -z $__terminaal_integration ]]; then
    __terminaal_integration=1
    __terminaal_precmd() {
        local ret=$?
        local cwd=${PWD//\%/%25}
        printf '\e]133;D;%s\a\e]7;file://%s%s\a\e]133;A\a' $ret $HOST ${cwd// /%20}
    }
    __terminaal_preexec() {
        printf '\e]133;C\a'
    }
    # First, so it still sees the command's status.
    precmd_functions=(__terminaal_precmd $precmd_functions)
    preexec_functions+=(__terminaal_preexec)
fi
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
        let launch = fish_launch(
            &InstalledShell::new("/usr/bin/fish"),
            Path::new("/home/u/.config/terminaal/shell-integration/terminaal.fish"),
            Path::new("/home/u/.config/fish/terminaal.fish"),
        );
        assert_eq!(launch.program, "/usr/bin/fish");
        assert_eq!(
            launch.args,
            [
                "--init-command",
                "source '/home/u/.config/terminaal/shell-integration/terminaal.fish'; \
                 test -f '/home/u/.config/fish/terminaal.fish'; and source '/home/u/.config/fish/terminaal.fish'"
            ]
        );
    }
}
