//! The worker thread of a 5250 pane: connects (TCP, TLS if asked),
//! negotiates, then pumps records between the server and the
//! [`Screen`], and keys from the pane into it, drawing the screen into the
//! pane's `Term` after every change.
//!
//! Like the SSH worker, the app talks to it through a handle: an mpsc
//! queue plus a socketpair that wakes its `poll()`. Dropping the handle
//! (closing the tab) ends the thread. When the connection fails or ends,
//! the screen stays with a notice: Enter connects again (a new sign-on),
//! Ctrl+D closes the tab.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;

use super::Tn5250Target;
use super::codepage::Codepage;
use super::keys::{self, KITTY_FLAGS, Typed};
use super::render::Renderer;
use super::screen::{Key, Packet, Screen};
use super::telnet::{self, Telnet};
use crate::i18n::t;
use crate::terminal::listener::EventProxyListener;

type SharedTerm = Arc<FairMutex<Term<EventProxyListener>>>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// The TLS handshake runs blocking, with this as the socket timeout.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
/// Don't hold the screen hostage to one flood of input.
const MAX_READ_PER_LOOP: usize = 1 << 20;

enum Msg {
    Input(Vec<u8>),
    Resize(WindowSize),
}

pub struct Tn5250Handle {
    tx: Sender<Msg>,
    wake: UnixStream,
}

impl Tn5250Handle {
    pub fn send_input(&self, bytes: Vec<u8>) {
        self.send(Msg::Input(bytes));
    }

    pub fn resize(&self, size: WindowSize) {
        self.send(Msg::Resize(size));
    }

    fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_ok() {
            // Non-blocking: if the socket buffer is full, a wakeup is pending anyway.
            let _ = (&self.wake).write(&[1]);
        }
    }
}

/// Start connecting in the background; the pane shows how it goes.
pub fn spawn(target: Tn5250Target, term: SharedTerm, listener: EventProxyListener, size: WindowSize) -> io::Result<Tn5250Handle> {
    let (tx, rx) = mpsc::channel();
    let (wake_tx, wake_rx) = UnixStream::pair()?;
    wake_tx.set_nonblocking(true)?;
    wake_rx.set_nonblocking(true)?;
    let codepage = Codepage::new(target.ccsid).unwrap_or_else(|| {
        log::warn!("{}: no table for CCSID {}, using 37", target.label, target.ccsid);
        Codepage::new(super::codepage::DEFAULT_CCSID).expect("CCSID 37 has a table")
    });
    let worker = Worker {
        renderer: Renderer::new(usize::from(size.num_cols), usize::from(size.num_lines)),
        target,
        codepage,
        term,
        listener,
        parser: Processor::new(),
        rx,
        wake: wake_rx,
        screen: None,
    };
    std::thread::Builder::new().name(format!("tn5250 {}", worker.target.label)).spawn(move || worker.run())?;
    Ok(Tn5250Handle { tx, wake: wake_tx })
}

/// How a connection ended.
enum Stop {
    /// Shown in the pane; Enter tries again.
    Failed(String),
    TabClosed,
}

enum Choice {
    Retry,
    Close,
    TabClosed,
}

/// TCP, or TLS over it.
enum Transport {
    Plain(TcpStream),
    Tls(Box<native_tls::TlsStream<TcpStream>>),
}

impl Transport {
    fn fd(&self) -> RawFd {
        match self {
            Transport::Plain(stream) => stream.as_raw_fd(),
            Transport::Tls(stream) => stream.get_ref().as_raw_fd(),
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(stream) => stream.read(buf),
            Transport::Tls(stream) => stream.read(buf),
        }
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(stream) => stream.write(buf),
            Transport::Tls(stream) => stream.write(buf),
        }
    }
}

struct Worker {
    target: Tn5250Target,
    codepage: Codepage,
    term: SharedTerm,
    listener: EventProxyListener,
    parser: Processor,
    rx: Receiver<Msg>,
    wake: UnixStream,
    renderer: Renderer,
    /// The 5250 screen once the server sent one; it stays after the
    /// connection ends, under the notice.
    screen: Option<Screen>,
}

impl Worker {
    fn run(mut self) {
        // Title for the tab; the alternate screen (no scrollback for a
        // screen that is redrawn in place); the wheel as mouse reports
        // (Roll); every key as an escape code, with its text.
        let title: String = self.target.label.chars().filter(|c| !c.is_control()).collect();
        self.write(format!("\x1b]2;{title}\x07\x1b[?1049h\x1b[?1000h\x1b[?1006h\x1b[>{KITTY_FLAGS}u").as_bytes());
        let mut attempt = 0;
        loop {
            let target = self.target.address();
            let connecting =
                if attempt == 0 { t!("conn-connecting", target = &target) } else { t!("conn-reconnecting", target = &target) };
            attempt += 1;
            self.screen = None;
            self.renderer.notice = None;
            self.write(format!("\x1b[0 q\x1b[H\x1b[2J\x1b[2m{connecting}\x1b[0m\r\n").as_bytes());
            let message = match self.connect_and_pump() {
                Stop::TabClosed => return,
                Stop::Failed(message) => message,
            };
            log::info!("{}: {message}", self.target.label);
            let hint = t!("conn-retry-or-close");
            if self.screen.is_some() {
                // Over the status line: what's on the screen stays readable.
                self.renderer.notice = Some(format!("{message} – {hint}"));
                self.redraw();
            } else {
                self.write(format!("\x1b[31m{message}\x1b[0m\r\n\x1b[2m{hint}\x1b[0m\r\n").as_bytes());
            }
            match self.wait_choice() {
                Choice::Retry => {}
                Choice::Close => return self.term.lock().exit(),
                Choice::TabClosed => return,
            }
        }
    }

    /// Bytes into the pane's terminal, and a redraw.
    fn write(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut *self.term.lock(), bytes);
        self.listener.send_event(Event::Wakeup);
    }

    /// Bring the pane up to date with the screen.
    fn redraw(&mut self) {
        let Some(screen) = &mut self.screen else { return };
        let bell = screen.take_bell();
        let mut frame = self.renderer.frame(screen);
        if bell {
            frame.push(0x07);
        }
        self.write(&frame);
    }

    fn resize(&mut self, size: WindowSize) {
        self.renderer.resize(usize::from(size.num_cols), usize::from(size.num_lines));
        if self.screen.is_some() {
            // Whatever the resize did to the terminal's rows, start over.
            self.write(b"\x1b[H\x1b[2J");
            self.redraw();
        }
    }

    /// Wait for Enter (try again) or Ctrl+D (close).
    fn wait_choice(&mut self) -> Choice {
        loop {
            let Ok(msg) = self.rx.recv() else { return Choice::TabClosed };
            match msg {
                Msg::Input(bytes) => {
                    for typed in keys::decode(&bytes) {
                        match typed {
                            Typed::Close => return Choice::Close,
                            Typed::Key(Key::Enter | Key::NewLine) => return Choice::Retry,
                            _ => {}
                        }
                    }
                }
                Msg::Resize(size) => self.resize(size),
            }
        }
    }

    fn connect(&self) -> Result<Transport, Stop> {
        Self::connect_target(&self.target)
    }

    fn connect_target(target: &Tn5250Target) -> Result<Transport, Stop> {
        let (host, port) = (target.host.as_str(), target.port);
        let addresses: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|err| Stop::Failed(t!("conn-resolve-failed", host = host, err = err.to_string())))?
            .collect();
        let mut last_error = None;
        let mut stream = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(err) => last_error = Some(err),
            }
        }
        let stream = stream.ok_or_else(|| {
            let err = last_error.map_or_else(|| t!("conn-unknown-type"), |err| err.to_string());
            Stop::Failed(t!("conn-connect-failed", host = host, port = port, err = err))
        })?;
        let _ = stream.set_nodelay(true);
        if !target.tls {
            stream.set_nonblocking(true).map_err(io_failure)?;
            return Ok(Transport::Plain(stream));
        }
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT)).map_err(io_failure)?;
        stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT)).map_err(io_failure)?;
        let tls_failed = |err: String| Stop::Failed(t!("tn5250-tls-failed", host = host, err = err));
        let connector = native_tls::TlsConnector::new().map_err(|err| tls_failed(err.to_string()))?;
        let stream = connector.connect(host, stream).map_err(|err| tls_failed(err.to_string()))?;
        stream.get_ref().set_read_timeout(None).map_err(io_failure)?;
        stream.get_ref().set_write_timeout(None).map_err(io_failure)?;
        stream.get_ref().set_nonblocking(true).map_err(io_failure)?;
        Ok(Transport::Tls(Box::new(stream)))
    }

    fn connect_and_pump(&mut self) -> Stop {
        let mut transport = match self.connect() {
            Ok(transport) => transport,
            Err(stop) => return stop,
        };
        log::info!("{}: connected to {}", self.target.label, self.target.address());
        let (terminal_type, device) = self.target.terminal_type();
        let mut environment: Vec<(String, String)> =
            self.codepage.environment().into_iter().map(|(name, value)| (name.to_string(), value)).collect();
        if let Some(device) = &self.target.device {
            environment.insert(0, ("DEVNAME".to_string(), device.to_uppercase()));
        }
        let mut telnet = Telnet::new(terminal_type, environment);
        let mut outgoing: Vec<u8> = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut screen = Some(Screen::new(self.codepage.clone(), device));

        loop {
            // Keys and resizes from the app.
            loop {
                match self.rx.try_recv() {
                    Ok(Msg::Input(bytes)) => {
                        // Only how much: what's typed may be a password.
                        log::trace!("{}: {} bytes typed", self.target.label, bytes.len());
                        if let Some(screen) = self.screen.as_mut() {
                            for packet in typed(screen, &bytes) {
                                outgoing.extend(telnet::frame(&packet.to_record()));
                            }
                            self.redraw();
                        }
                    }
                    Ok(Msg::Resize(size)) => self.resize(size),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Stop::TabClosed,
                }
            }
            let mut drain = [0u8; 64];
            while matches!((&self.wake).read(&mut drain), Ok(n) if n > 0) {}

            // From the server.
            let mut processed = 0;
            let mut changed = false;
            while processed < MAX_READ_PER_LOOP {
                match transport.read(&mut buf) {
                    Ok(0) => return Stop::Failed(t!("tn5250-closed")),
                    Ok(n) => {
                        processed += n;
                        let (mut records, mut reply) = (Vec::new(), Vec::new());
                        telnet.feed(&buf[..n], &mut records, &mut reply);
                        outgoing.extend(reply);
                        for record in records {
                            // The first screen replaces the connection messages.
                            if self.screen.is_none() {
                                self.screen = screen.take();
                                self.write(b"\x1b[H\x1b[2J");
                                self.renderer.invalidate();
                            }
                            let Some(screen) = self.screen.as_mut() else { continue };
                            for packet in screen.record(&record) {
                                outgoing.extend(telnet::frame(&packet.to_record()));
                            }
                            changed = true;
                        }
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) if err.kind() == ErrorKind::Interrupted => {}
                    Err(err) => return Stop::Failed(t!("conn-lost", err = err.to_string())),
                }
            }
            if changed {
                self.redraw();
            }

            while !outgoing.is_empty() {
                match transport.write(&outgoing) {
                    Ok(0) => return Stop::Failed(t!("tn5250-closed")),
                    Ok(n) => drop(outgoing.drain(..n)),
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) if err.kind() == ErrorKind::Interrupted => {}
                    Err(err) => return Stop::Failed(t!("conn-lost", err = err.to_string())),
                }
            }

            if processed >= MAX_READ_PER_LOOP {
                continue;
            }
            let mut events = libc::POLLIN;
            if !outgoing.is_empty() {
                events |= libc::POLLOUT;
            }
            let mut fds = [
                libc::pollfd { fd: transport.fd(), events, revents: 0 },
                libc::pollfd { fd: self.wake.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            ];
            // SAFETY: two valid pollfds; both descriptors outlive the call.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if ready < 0 && io::Error::last_os_error().kind() != ErrorKind::Interrupted {
                return Stop::Failed(t!("conn-io-error", err = io::Error::last_os_error().to_string()));
            }
            // The app's end of the wakeup socket is gone: the tab closed.
            if fds[1].revents & libc::POLLHUP != 0 {
                return Stop::TabClosed;
            }
        }
    }
}

fn io_failure(err: io::Error) -> Stop {
    Stop::Failed(t!("conn-io-error", err = err.to_string()))
}

/// Keys typed into the pane, onto the screen; what to send. Pasted text
/// stops at the first key that locks or inhibits the keyboard.
fn typed(screen: &mut Screen, bytes: &[u8]) -> Vec<Packet> {
    let mut out = Vec::new();
    for typed in keys::decode(bytes) {
        let key = match typed {
            Typed::Key(key) => key,
            Typed::Keypad(c) if screen.in_numeric_field() => {
                if c == '-' {
                    Key::FieldMinus
                } else {
                    Key::FieldPlus
                }
            }
            Typed::Keypad(c) => Key::Char(c),
            // Only while disconnected.
            Typed::Close => continue,
        };
        out.extend(screen.key(key));
        if !out.is_empty() || !screen.typing() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::tn5250::screen::tests::text;

    /// Against the real thing: PUB400's sign-on screen, over plain telnet
    /// and over TLS. Nothing is typed. `cargo test -- --ignored talks_to_pub400`
    #[test]
    #[ignore]
    fn talks_to_pub400() {
        for (port, tls) in [(23, false), (992, true)] {
            let target = Tn5250Target {
                label: "PUB400".into(),
                name: "PUB400".into(),
                host: "pub400.com".into(),
                port,
                tls,
                ccsid: 37,
                wide: false,
                device: None,
                color: None,
                theme: None,
            };
            let worker_target = target.clone();
            let mut transport = Worker::connect_target(&worker_target).unwrap_or_else(|_| panic!("connect {port}"));
            let (terminal_type, device) = target.terminal_type();
            let codepage = Codepage::new(37).unwrap();
            let environment = codepage.environment().into_iter().map(|(n, v)| (n.to_string(), v)).collect();
            let mut telnet = Telnet::new(terminal_type, environment);
            let mut screen = Screen::new(codepage, device);
            let mut buf = vec![0u8; 65536];
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut records = 0;
            while Instant::now() < deadline && !(records > 0 && screen.typing()) {
                match transport.read(&mut buf) {
                    Ok(0) => panic!("closed"),
                    Ok(n) => {
                        let (mut got, mut reply) = (Vec::new(), Vec::new());
                        telnet.feed(&buf[..n], &mut got, &mut reply);
                        for record in got {
                            records += 1;
                            for packet in screen.record(&record) {
                                reply.extend(telnet::frame(&packet.to_record()));
                            }
                        }
                        let mut sent = 0;
                        while sent < reply.len() {
                            match transport.write(&reply[sent..]) {
                                Ok(n) => sent += n,
                                Err(err) if err.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
                                Err(err) => panic!("{err}"),
                            }
                        }
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(20)),
                    Err(err) => panic!("{err}"),
                }
            }
            let lines = text(&screen);
            println!("--- port {port}: {records} records, keyboard {:?}, cursor {:?}", screen.keyboard, screen.row_col(screen.cursor));
            for line in &lines {
                println!("|{line}");
            }
            assert!(screen.typing(), "the sign-on screen unlocks the keyboard");
            assert!(lines.iter().filter(|line| !line.is_empty()).count() > 3, "{lines:#?}");
        }
    }
}
