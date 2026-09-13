//! One terminal session: an `alacritty_terminal::Term` (the "brain" --
//! grid, cursor, VT parser state) plus whatever feeds it bytes:
//!
//! - a local PTY running a shell, pumped by alacritty_terminal's own
//!   event loop, or
//! - an SSH channel, pumped by our worker thread in `ssh::connection`.
//!
//! Everything in `render/` and `input.rs` only ever sees `term`,
//! `send_input` and `resize`, so it works the same for both.

use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::event::{Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop as PtyEventLoop, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::tty;

use crate::commands::{self, Target};
use crate::shells::launch::Launch;
use crate::ssh::SshTarget;
use crate::ssh::connection::{self, SshHandle};
use crate::ssh::forward::ForwardStatus;
use crate::terminal::filtered_pty::FilteredPty;
use crate::terminal::listener::EventProxyListener;

/// Terminal grid size, in cells. Implements `Dimensions` so it can be
/// handed straight to `Term::new` / `Term::resize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub columns: usize,
    pub screen_lines: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

pub struct TerminalSession {
    pub term: Arc<FairMutex<Term<EventProxyListener>>>,
    backend: Backend,
}

enum Backend {
    Local(Notifier),
    Ssh(SshHandle),
}

impl TerminalSession {
    /// Spawn `launch` (a local shell, see `shells::launch`) in a PTY and
    /// start pumping its output into a fresh `Term`.
    pub fn spawn_local_shell(
        listener: EventProxyListener,
        launch: &Launch,
        working_directory: Option<PathBuf>,
        size: GridSize,
        cell_width: f32,
        cell_height: f32,
        scrollback: usize,
    ) -> std::io::Result<Self> {
        tty::setup_env();

        let term = new_term(&listener, size, scrollback);
        let options = tty::Options {
            shell: Some(tty::Shell::new(launch.program.clone(), launch.args.clone())),
            env: launch.env.clone(),
            working_directory,
            ..tty::Options::default()
        };
        let pty = tty::new(&options, window_size(size, cell_width, cell_height), 0)?;
        let shell_events = listener.clone();
        let pty = FilteredPty::new(pty, move |event| shell_events.send_shell(event))?;

        let pty_event_loop = PtyEventLoop::new(term.clone(), listener, pty, false, false)?;
        let notifier = Notifier(pty_event_loop.channel());
        // Runs the PTY read/write loop on its own thread for the lifetime
        // of the process; we don't need the join handle for the MVP.
        let _ = pty_event_loop.spawn();

        Ok(Self { term, backend: Backend::Local(notifier) })
    }

    /// Connect to `target` over SSH. Returns right away -- connecting,
    /// prompts and errors all play out inside the tab.
    pub fn connect_ssh(
        listener: EventProxyListener,
        target: SshTarget,
        size: GridSize,
        cell_width: f32,
        cell_height: f32,
        scrollback: usize,
    ) -> std::io::Result<Self> {
        let term = new_term(&listener, size, scrollback);
        let handle = connection::spawn(target, term.clone(), listener, window_size(size, cell_width, cell_height))?;
        Ok(Self { term, backend: Backend::Ssh(handle) })
    }

    /// Send raw bytes (already encoded by `input.rs`, or a reply to a
    /// PTY-write/color/size request from `app.rs`) to the shell.
    pub fn send_input(&self, bytes: Vec<u8>) {
        match &self.backend {
            Backend::Local(notifier) => notifier.notify(bytes),
            Backend::Ssh(handle) => handle.send_input(bytes),
        }
    }

    /// Tell both the `Term` and the shell's PTY about a new size.
    pub fn resize(&mut self, size: GridSize, cell_width: f32, cell_height: f32) {
        self.term.lock().resize(size);
        let window_size = window_size(size, cell_width, cell_height);
        match &mut self.backend {
            Backend::Local(notifier) => notifier.on_resize(window_size),
            Backend::Ssh(handle) => handle.resize(window_size),
        }
    }

    /// The SSH connection's port forwards and how they're doing; none for
    /// a local shell.
    pub fn forwards(&self) -> Vec<ForwardStatus> {
        match &self.backend {
            Backend::Local(_) => Vec::new(),
            Backend::Ssh(handle) => handle.forwards(),
        }
    }

    /// Pause forward `index` or start it again (SSH only).
    pub fn set_forward(&self, index: usize, enabled: bool) {
        if let Backend::Ssh(handle) = &self.backend {
            handle.set_forward(index, enabled);
        }
    }

    /// A local shell rather than an SSH connection.
    pub fn is_local(&self) -> bool {
        matches!(self.backend, Backend::Local(_))
    }

    /// What the built-in commands (`crate::commands`) should build for:
    /// this machine for a local shell, whatever the host turned out to be
    /// for SSH.
    pub fn command_target(&self) -> Target {
        match &self.backend {
            Backend::Local(_) => Target { system: Some(commands::local()), ..Target::default() },
            Backend::Ssh(handle) => handle.target(),
        }
    }

    /// Keep this many lines of scrollback from now on; fewer than before
    /// drops the oldest.
    pub fn set_scrollback(&self, lines: usize) {
        self.term.lock().set_options(term_config(lines));
    }
}

fn term_config(scrollback: usize) -> TermConfig {
    TermConfig { scrolling_history: scrollback, ..TermConfig::default() }
}

fn new_term(listener: &EventProxyListener, size: GridSize, scrollback: usize) -> Arc<FairMutex<Term<EventProxyListener>>> {
    Arc::new(FairMutex::new(Term::new(term_config(scrollback), &size, listener.clone())))
}

fn window_size(size: GridSize, cell_width: f32, cell_height: f32) -> WindowSize {
    WindowSize {
        num_lines: size.screen_lines as u16,
        num_cols: size.columns as u16,
        cell_width: cell_width as u16,
        cell_height: cell_height as u16,
    }
}
