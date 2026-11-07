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
//! PowerShell gets only the integration, inline (`-EncodedCommand`): a
//! script file wouldn't run under Windows' default execution policy.
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
    let result = match (shell.kind, managed::path_for(shell.kind)) {
        (ShellKind::PowerShell, file) => Ok(powershell(shell, file.as_deref())),
        (ShellKind::Fish, Some(file)) => fish(shell, &file),
        (ShellKind::Bash, Some(file)) => bash(shell, &file),
        (ShellKind::Zsh, Some(file)) => zsh(shell, &file),
        _ if shell.name.eq_ignore_ascii_case("cmd") => Ok(cmd(shell, std::env::var("PROMPT").ok())),
        _ => Ok(Launch::plain(shell)),
    };
    result.unwrap_or_else(|err| {
        log::warn!("{}: shell integration unavailable, starting without managed aliases: {err}", shell.name);
        Launch::plain(shell)
    })
}

/// The command `terminaal -c "..."` runs: the shell evaluates the whole
/// line, like `sh -c`. Deliberately without the managed alias file and
/// the shell integration -- those belong to an interactive shell, and
/// bash and zsh don't read any rc file for `-c` anyway. What the line
/// means is exactly what `<shell> -c '<line>'` means anywhere else
/// (cmd: `/C`; PowerShell takes `-c` too).
pub fn launch_command(shell: &InstalledShell, line: &str) -> Launch {
    let flag = if shell.name.eq_ignore_ascii_case("cmd") { "/C" } else { "-c" };
    Launch { args: vec![flag.into(), line.to_string()], ..Launch::plain(shell) }
}

/// What the tab is called while the command runs: its first word without
/// its path (`sudo`, `htop`), or the whole line if there's no word in it.
pub fn command_title(line: &str) -> String {
    let word = line.split_whitespace().next().unwrap_or("").trim_matches(|c| c == '\'' || c == '"');
    let name = word.rsplit('/').next().unwrap_or(word);
    if name.is_empty() { line.trim().to_string() } else { name.to_string() }
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

/// Runs after the user's `$PROFILE` and wraps their `prompt`. `C` comes
/// from Enter (PSReadLine), PowerShell has no hook before a command; the
/// status is `$LASTEXITCODE` of a failed program, else 1 for a failed
/// cmdlet. `ESC`/`BEL` as `[char]`: Windows PowerShell 5.1 has no `` `e ``.
/// The user's prompt sits in a variable of its own: after `&` PowerShell
/// reads `$table.Prompt` as text, not as the table's entry. A prompt that
/// throws shows as a bare `PS>`.
const POWERSHELL_INTEGRATION: &str = r#"# Terminaal: working directory (OSC 7) and prompts (OSC 133).
if (-not $global:__terminaal_prompt) {
    $global:__terminaal_prompt = $function:prompt
    $global:__terminaal_ran = $false
    function global:prompt {
        $ok = $?
        $code = $global:LASTEXITCODE
        $e = [char]27; $a = [char]7
        $out = ''
        if ($global:__terminaal_ran) {
            $status = if ($ok) { 0 } elseif ($code) { $code } else { 1 }
            $out += "$e]133;D;$status$a"
            $global:__terminaal_ran = $false
        }
        $loc = $ExecutionContext.SessionState.Path.CurrentLocation
        if ($loc.Provider.Name -eq 'FileSystem') {
            $path = ('/' + $loc.ProviderPath.Replace('\', '/')).Replace('%', '%25').Replace(' ', '%20')
            $out += "$e]7;file://$env:COMPUTERNAME$path$a"
        }
        $out += "$e]133;A$a"
        $out + (& $global:__terminaal_prompt)
    }
    if (Get-Command Set-PSReadLineKeyHandler -ErrorAction SilentlyContinue) {
        Set-PSReadLineKeyHandler -Chord Enter -ScriptBlock {
            $line = $null; $cursor = $null
            [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
            [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
            if ($line.Trim()) {
                $global:__terminaal_ran = $true
                [Console]::Write("$([char]27)]133;C$([char]7)")
            }
        }
    }
    # The managed aliases and functions, as a script block: a .ps1 file
    # wouldn't run under the default execution policy. UTF-8 also for 5.1,
    # which reads files without a BOM as ANSI.
    if ($env:TERMINAAL_MANAGED) {
        if (Test-Path -LiteralPath $env:TERMINAAL_MANAGED) {
            $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $env:TERMINAAL_MANAGED
            if ($text) { . ([ScriptBlock]::Create($text)) }
            Remove-Variable text
        }
        Remove-Item Env:TERMINAAL_MANAGED
    }
}
"#;

/// The managed `file` comes in by environment (`TERMINAAL_MANAGED`), which
/// the script removes again -- no quoting of a path inside the script.
fn powershell(shell: &InstalledShell, file: Option<&Path>) -> Launch {
    use base64::Engine;
    // `-EncodedCommand` takes Base64 of UTF-16LE.
    let utf16: Vec<u8> = POWERSHELL_INTEGRATION.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let mut launch = Launch { args: vec!["-NoExit".into(), "-EncodedCommand".into(), encoded], ..Launch::plain(shell) };
    if let Some(file) = file {
        launch.env.insert("TERMINAAL_MANAGED".into(), file.to_string_lossy().into_owned());
    }
    launch
}

/// The marks cmd's prompt gets: `$E` is an escape there.
const CMD_PROMPT_START: &str = "$E]9;9;$P$E\\$E]133;A$E\\";
const CMD_PROMPT_END: &str = "$E]133;B$E\\";

/// cmd has no hooks, only its `PROMPT`, which can print escapes: the
/// folder (OSC 9;9) and the prompt marks around the user's own prompt
/// (`prompt`, else cmd's `$P$G`). Nothing runs before a command, so
/// there's no C or D: no durations, exit codes or notifications.
fn cmd(shell: &InstalledShell, prompt: Option<String>) -> Launch {
    let own = prompt.filter(|prompt| !prompt.trim().is_empty()).unwrap_or_else(|| "$P$G".into());
    // Started from a cmd tab of Terminaal's: marked already.
    let marked = if own.contains(CMD_PROMPT_START) { own } else { format!("{CMD_PROMPT_START}{own}{CMD_PROMPT_END}") };
    let mut launch = Launch::plain(shell);
    launch.env.insert("PROMPT".into(), marked);
    launch
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
    fn command_runs_the_whole_line_through_the_shell() {
        let launch = launch_command(&InstalledShell::new("/usr/bin/fish"), "ls -l | less");
        assert_eq!(launch.program, "/usr/bin/fish");
        assert_eq!(launch.args, ["-c", "ls -l | less"]);
        assert!(launch.env.is_empty());
    }

    #[test]
    fn command_title_is_the_first_word_without_its_path() {
        assert_eq!(command_title("htop"), "htop");
        assert_eq!(command_title("  /usr/bin/btop --utf-force "), "btop");
        assert_eq!(command_title("ssh host 'tail -f log'"), "ssh");
        assert_eq!(command_title("   "), "");
    }

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

    #[test]
    fn cmd_gets_marks_around_its_prompt() {
        let shell = InstalledShell::new("/x/cmd");
        let prompt = |own: Option<&str>| cmd(&shell, own.map(str::to_string)).env["PROMPT"].clone();
        assert_eq!(prompt(None), r"$E]9;9;$P$E\$E]133;A$E\$P$G$E]133;B$E\");
        assert_eq!(prompt(Some("$T $P$G")), r"$E]9;9;$P$E\$E]133;A$E\$T $P$G$E]133;B$E\");
        assert_eq!(prompt(Some(&prompt(None))), prompt(None), "not marked twice");

        // What cmd prints for it goes through the filter as a prompt and a folder.
        let printed = b"\x1b]9;9;C:\\Users\x1b\\\x1b]133;A\x1b\\C:\\Users>\x1b]133;B\x1b\\";
        let mut filter = crate::terminal::integration::Filter::default();
        let (mut out, mut events) = (Vec::new(), Vec::new());
        filter.feed(printed, &mut out, &mut events);
        use crate::terminal::integration::ShellEvent;
        assert_eq!(events, [ShellEvent::Cwd { host: String::new(), path: "C:\\Users".into() }, ShellEvent::Prompt]);
    }

    #[test]
    fn powershell_gets_the_integration_inline() {
        use base64::Engine;
        for path in ["/usr/bin/pwsh", "/opt/powershell"] {
            let shell = InstalledShell::new(path);
            assert_eq!(shell.kind, ShellKind::PowerShell, "{path}");
            let launch = launch(&shell);
            assert_eq!(launch.args[..2], ["-NoExit", "-EncodedCommand"]);
            let bytes = base64::engine::general_purpose::STANDARD.decode(&launch.args[2]).unwrap();
            let utf16: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
            assert_eq!(String::from_utf16(&utf16).unwrap(), POWERSHELL_INTEGRATION);
        }
        // Both once threw in the prompt, which then showed as a bare `PS>`:
        // an empty search string, and `& $table.Prompt` (text after `&`).
        assert!(POWERSHELL_INTEGRATION.contains(r"ProviderPath.Replace('\', '/')"));
        assert!(!POWERSHELL_INTEGRATION.contains("Replace('',"));
        assert!(POWERSHELL_INTEGRATION.contains("(& $global:__terminaal_prompt)"));
    }
}
