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

fn main() {
    env_logger::Builder::from_default_env().format_timestamp_millis().init();

    let config = config::Config::load();
    i18n::set(config.language());
    let (connect, quake) = match parse_args() {
        Ok(Args::Normal) => (None, None),
        Ok(Args::Connect(target)) => (Some(*target), None),
        Ok(Args::Quake) => match quake::start(&quake::runtime_dir()) {
            Ok(quake::Start::Toggled) => return,
            Ok(quake::Start::Owner(toggle)) => (None, Some(toggle)),
            Err(err) => {
                eprintln!("{}", t!("cli-quake-failed", err = err.to_string()));
                std::process::exit(1);
            }
        },
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
    let mut app = app::App::new(proxy, config, connect, quake);
    event_loop.run_app(&mut app).expect("event loop error");
}

enum Args {
    Normal,
    Connect(Box<ssh::SshTarget>),
    Quake,
}

/// `terminaal [--connect [USER@]HOST | --quake]`: with `--connect`, the
/// first tab is an SSH connection to the saved host -- or `~/.ssh/config`
/// host -- of that name instead of a local shell. With `USER@`, the host's
/// login for that user, or else its default login under that user name.
/// `--quake` shows or hides the drop-down Terminaal (`quake`).
fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);
    let usage = t!("cli-usage");
    let Some(arg) = args.next() else { return Ok(Args::Normal) };
    if arg == "--quake" {
        return match args.next() {
            Some(extra) => Err(format!("{}\n{usage}", t!("cli-unexpected-argument", arg = &extra))),
            None => Ok(Args::Quake),
        };
    }
    if arg != "--connect" {
        return Err(format!("{}\n{usage}", t!("cli-unknown-argument", arg = &arg)));
    }
    let name = args.next().ok_or_else(|| format!("{}\n{usage}", t!("cli-connect-needs-host")))?;
    if let Some(extra) = args.next() {
        return Err(format!("{}\n{usage}", t!("cli-unexpected-argument", arg = &extra)));
    }
    let catalog = ssh::Catalog::load()?;
    let (user, host_name) = match catalog.find(&name) {
        Some(_) => (None, name.as_str()),
        None => match name.split_once('@') {
            Some((user, host)) => (Some(user), host),
            None => (None, name.as_str()),
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
    catalog.target(&host).map(|target| Args::Connect(Box::new(target)))
}
