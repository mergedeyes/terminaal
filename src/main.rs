mod app;
mod blur;
mod commands;
mod config;
mod gpu;
mod i18n;
mod input;
mod panes;
mod quake;
mod render;
mod session;
mod sftp;
mod shells;
mod shortcuts;
mod snippets;
mod ssh;
mod terminal;
mod theme;
mod ui;
mod window;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::i18n::t;
use crate::shells::InstalledShell;

fn main() {
    env_logger::Builder::from_default_env().format_timestamp_millis().init();

    let config = config::Config::load();
    i18n::set(config.language());
    let (cli, quake) = match parse_args() {
        Ok(Started::Run(cli)) => (cli, None),
        Ok(Started::Done(Done::Quake(toggle))) => (Cli::default(), Some(toggle)),
        Ok(Started::Done(_)) => return,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    let event_loop = EventLoop::<app::UserEvent>::with_user_event()
        .build()
        .expect("failed to create winit event loop");
    // Redraws happen in response to terminal output/title changes (the
    // user-event wakeup in `app::App::user_event`), window events like
    // resize, or a scheduled `WaitUntil` deadline (cursor blink, egui
    // animations) that `App::about_to_wait` sets before every sleep.
    // `Wait` is just the safe starting point before any of those are known.
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    let mut app = app::App::new(proxy, config, cli, quake);
    event_loop.run_app(&mut app).expect("event loop error");
}

/// What the command line asks for. `--quake`, `--help`, `--version` and
/// `-s` without a shell name are done before the window ever opens, so
/// they never turn into a [`Cli`].
#[derive(Default)]
pub struct Cli {
    /// Open the first tab as this SSH connection instead of a local shell
    /// (`--connect`).
    pub connect: Option<Box<ssh::SshTarget>>,
    /// `-s`: the first tab's shell, instead of the configured default.
    pub shell: Option<InstalledShell>,
    /// `-c`: one line for the shell to run instead of starting an
    /// interactive one. With `--connect` it runs on the server
    /// (`RemoteCommand`).
    pub command: Option<String>,
    /// `--hold`: keep the tab once what it runs has ended.
    pub hold: bool,
}

impl Cli {
    /// Started for one job (a host, a shell, a command) rather than
    /// plainly: like `--connect` those neither restore the saved session
    /// nor save one -- the tabs are what the command line asked for, not
    /// what was open last time.
    pub fn one_off(&self) -> bool {
        self.connect.is_some() || self.shell.is_some() || self.command.is_some()
    }
}

/// Handled before the window opens.
enum Done {
    /// `--quake`: another instance was told to show or hide itself.
    Toggled,
    /// This instance is the drop-down Terminaal.
    Quake(quake::Toggle),
    /// `--help`, `--version`, `-s` without a name: printed and finished.
    Printed,
}

enum Started {
    Done(Done),
    Run(Cli),
}

/// The flags as given, before anything is looked up on disk. Kept apart
/// from [`resolve`] so the argument rules can be tested without a
/// `~/.ssh/config`, an `/etc/shells` or a window.
#[derive(Debug, Default, PartialEq)]
struct Flags {
    quake: bool,
    help: bool,
    version: bool,
    connect: Option<String>,
    /// `Some(None)` is `-s` without a name: list the installed shells.
    shell: Option<Option<String>>,
    command: Option<String>,
    hold: bool,
}

fn usage_error(message: String) -> String {
    format!("{message}\n{}", t!("cli-usage"))
}

/// `terminaal [--connect [USER@]HOST] [-s SHELL] [-c LINE] [--hold]`,
/// or `terminaal --quake`.
///
/// `--connect` opens the first tab as an SSH connection to the saved --
/// or `~/.ssh/config` -- host of that name; with `USER@`, that host's
/// login for that user, or else its default login under that user name.
/// `-s` picks the shell of the first tab, `-s` on its own lists the
/// installed ones. `-c` hands one line to that shell (on the server with
/// `--connect`), `--hold` keeps the tab once it ends. `--quake` shows or
/// hides the drop-down Terminaal (`quake`).
fn parse_flags(args: impl Iterator<Item = String>) -> Result<Flags, String> {
    let mut flags = Flags::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        // A value of its own, never the next flag: `-c` takes a whole
        // command line, which may well start with a dash.
        let mut value = |opt: &str| args.next().ok_or_else(|| usage_error(t!("cli-needs-value", option = opt)));
        match arg.as_str() {
            "--quake" => flags.quake = true,
            "--help" | "-h" => flags.help = true,
            "--version" | "-V" => flags.version = true,
            "--hold" => flags.hold = true,
            "--connect" => flags.connect = Some(value("--connect")?),
            "-c" => flags.command = Some(value("-c")?),
            // A shell name is one word and never starts with a dash, so
            // `-s` at the end or before another flag means "list them".
            "-s" => {
                let named = args.peek().is_some_and(|next| !next.starts_with('-'));
                flags.shell = Some(named.then(|| args.next().expect("peeked")));
            }
            _ => return Err(usage_error(t!("cli-unknown-argument", arg = &arg))),
        }
    }
    if flags.quake && flags != (Flags { quake: true, ..Flags::default() }) {
        return Err(usage_error(t!("cli-quake-alone")));
    }
    // On its own it would have nothing to keep open: a plain start
    // restores the saved session rather than running one thing.
    let one_off = flags.connect.is_some() || flags.command.is_some() || flags.shell.is_some();
    if flags.hold && !one_off {
        return Err(usage_error(t!("cli-hold-alone")));
    }
    Ok(flags)
}

fn parse_args() -> Result<Started, String> {
    let flags = parse_flags(std::env::args().skip(1))?;
    if flags.help {
        println!("{}", t!("cli-help"));
        return Ok(Started::Done(Done::Printed));
    }
    if flags.version {
        println!("terminaal {}", env!("CARGO_PKG_VERSION"));
        return Ok(Started::Done(Done::Printed));
    }
    if flags.quake {
        return match quake::start(&quake::runtime_dir()) {
            Ok(quake::Start::Toggled) => Ok(Started::Done(Done::Toggled)),
            Ok(quake::Start::Owner(toggle)) => Ok(Started::Done(Done::Quake(toggle))),
            Err(err) => Err(t!("cli-quake-failed", err = err.to_string())),
        };
    }
    resolve(flags)
}

/// The flags as tabs: the host looked up in the catalog, the shell among
/// the installed ones.
fn resolve(flags: Flags) -> Result<Started, String> {
    let shell = match flags.shell {
        None => None,
        Some(None) => {
            list_shells();
            return Ok(Started::Done(Done::Printed));
        }
        Some(Some(name)) => {
            let installed = shells::detect();
            Some(shells::find(&name, &installed).ok_or_else(|| {
                let list = installed.iter().map(|shell| format!("  {}", shell.name)).collect::<Vec<_>>().join("\n");
                format!("{}\n{}:\n{list}", t!("cli-unknown-shell", shell = &name), t!("shells-installed"))
            })?)
        }
    };
    let connect = match &flags.connect {
        None => None,
        Some(name) => Some(Box::new(connect_target(name, flags.command.as_deref())?)),
    };
    // Over SSH the line is what the server's shell runs (RemoteCommand);
    // locally it's what the first tab's shell runs.
    let command = flags.command.filter(|_| connect.is_none());
    Ok(Started::Run(Cli { connect, shell, command, hold: flags.hold }))
}

fn list_shells() {
    for shell in shells::detect() {
        println!("{:<12} {}", shell.name, shell.path.display());
    }
}

/// The saved -- or `~/.ssh/config` -- host called `name`, optionally
/// `user@host`; `command` becomes its `RemoteCommand`.
fn connect_target(name: &str, command: Option<&str>) -> Result<ssh::SshTarget, String> {
    let catalog = ssh::Catalog::load()?;
    let (user, host_name) = match catalog.find(name) {
        Some(_) => (None, name),
        None => match name.split_once('@') {
            Some((user, host)) => (Some(user), host),
            None => (None, name),
        },
    };
    let host = catalog
        .find(host_name)
        .ok_or_else(|| t!("cli-unknown-host", host = host_name))?;
    let host = match user {
        None => host.clone(),
        Some(user) => match host.login_index(user) {
            Some(index) => host.with_login(index).expect("index comes from login_index"),
            None => ssh::Host { user: user.to_string(), logins: Vec::new(), ..host.clone() },
        },
    };
    let mut target = catalog.target(&host)?;
    if let Some(line) = command {
        target.settings.remote_command = Some(line.to_string());
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(args: &[&str]) -> Result<Flags, String> {
        parse_flags(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn plain_start_has_no_flags() {
        assert_eq!(flags(&[]).unwrap(), Flags::default());
    }

    #[test]
    fn reads_shell_command_and_hold() {
        let parsed = flags(&["-s", "fish", "-c", "btop -u", "--hold"]).unwrap();
        assert_eq!(parsed.shell, Some(Some("fish".into())));
        assert_eq!(parsed.command.as_deref(), Some("btop -u"));
        assert!(parsed.hold);
    }

    #[test]
    fn a_command_may_start_with_a_dash() {
        assert_eq!(flags(&["-c", "-x --y"]).unwrap().command.as_deref(), Some("-x --y"));
    }

    #[test]
    fn bare_s_lists_the_shells_even_before_another_flag() {
        assert_eq!(flags(&["-s"]).unwrap().shell, Some(None));
        assert_eq!(flags(&["-s", "-c", "ls"]).unwrap().shell, Some(None));
    }

    #[test]
    fn connect_takes_a_host_and_a_missing_value_is_an_error() {
        assert_eq!(flags(&["--connect", "server"]).unwrap().connect.as_deref(), Some("server"));
        let err = flags(&["--connect"]).unwrap_err();
        assert!(err.contains("--connect"), "{err}");
        assert!(flags(&["-c"]).is_err());
    }

    #[test]
    fn hold_needs_something_to_hold() {
        assert!(flags(&["--hold"]).is_err());
        assert!(flags(&["-s", "fish", "--hold"]).unwrap().hold);
        assert!(flags(&["--connect", "server", "--hold"]).unwrap().hold);
    }

    #[test]
    fn quake_is_alone_and_unknown_arguments_are_refused() {
        assert!(flags(&["--quake"]).unwrap().quake);
        assert!(flags(&["--quake", "--hold"]).is_err());
        assert!(flags(&["--quake", "-c", "ls"]).is_err());
        let err = flags(&["dev"]).unwrap_err();
        assert!(err.contains("dev"), "{err}");
    }
}
