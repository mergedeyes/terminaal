//! Port forwarding (`LocalForward`, `RemoteForward`) on a tab's SSH
//! session, driven by the connection's pump loop -- the session is
//! non-blocking and owned by that one thread, so every forwarded
//! connection is serviced from there, next to the shell channel.
//!
//! Local: we listen, and each accepted connection gets a `direct-tcpip`
//! channel to its target. libssh2 keeps the state of a channel being
//! opened in the session, so only one open is in flight at a time; the
//! rest wait in a queue. Remote: the server listens; each connection it
//! reports is connected to its local target here.
//!
//! libssh2 reads packets for *all* channels whenever it reads for one, so
//! data can end up buffered for a channel that was already serviced in
//! this round. [`Forwards::buffered`] tells the pump not to sleep then.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, RawFd};
use std::time::Duration;

use ssh2::{Channel, ErrorCode, ExtendedData, Listener, Session};

use super::options::Forward;
use crate::i18n::t;

/// Per-direction buffer limit of one forwarded connection.
const MAX_BUFFER: usize = 256 * 1024;
/// Connecting to a remote forward's local target blocks the pump loop,
/// so it has to be quick. Targets are usually on this machine.
const LOCAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// libssh2's `LIBSSH2_ERROR_EAGAIN`.
const EAGAIN: i32 = -37;

fn would_block(err: &ssh2::Error) -> bool {
    err.code() == ErrorCode::Session(EAGAIN)
}

struct LocalListener {
    socket: TcpListener,
    forward: Forward,
}

struct RemoteListener {
    listener: Listener,
    forward: Forward,
}

/// An accepted local connection waiting for its channel.
struct Pending {
    stream: TcpStream,
    origin: SocketAddr,
    forward: Forward,
}

/// One forwarded connection: a channel and the local socket it's tied to.
struct Tunnel {
    channel: Channel,
    /// `None` for a channel that had nowhere to go and is only being closed.
    stream: Option<TcpStream>,
    to_local: Vec<u8>,
    to_remote: Vec<u8>,
    /// The local side won't send any more.
    local_eof: bool,
    /// ... and we've told the server so.
    eof_sent: bool,
    /// The server won't send any more, and the local side knows.
    remote_done: bool,
    /// Done or broken: only the channel close is left.
    closing: bool,
}

#[derive(Default)]
pub struct Forwards {
    local: Vec<LocalListener>,
    remote: Vec<RemoteListener>,
    pending: VecDeque<Pending>,
    tunnels: Vec<Tunnel>,
}

impl Forwards {
    /// Start listening, locally or on the server. Needs `session` still in
    /// blocking mode (the remote listen waits for the server's answer).
    /// Returns a line per forward for the tab: `Ok` if it's up, `Err` why
    /// not. A failed forward doesn't stop the others or the connection
    /// (OpenSSH's `ExitOnForwardFailure no`).
    pub fn start(session: &Session, forwards: &[Forward]) -> (Self, Vec<Result<String, String>>) {
        let mut this = Self::default();
        let mut report = Vec::new();
        for forward in forwards {
            let describe = |listen: String| format!("{listen} → {}", forward.target_label());
            if forward.remote {
                let bind = match forward.bind.as_deref() {
                    None => "localhost",
                    Some("*") => "",
                    Some(bind) => bind,
                };
                match session.channel_forward_listen(forward.port, Some(bind), None) {
                    Ok((listener, bound)) => {
                        let mut shown = forward.clone();
                        shown.port = bound;
                        report.push(Ok(t!("forward-up", forward = describe(shown.listen_label()))));
                        this.remote.push(RemoteListener { listener, forward: forward.clone() });
                    }
                    Err(err) => report.push(Err(t!(
                        "forward-failed",
                        forward = describe(forward.listen_label()),
                        err = err.to_string()
                    ))),
                }
            } else {
                match listen_locally(forward) {
                    Ok(sockets) => {
                        report.push(Ok(t!("forward-up", forward = describe(forward.listen_label()))));
                        this.local.extend(sockets.into_iter().map(|socket| LocalListener { socket, forward: forward.clone() }));
                    }
                    Err(err) => report.push(Err(t!(
                        "forward-failed",
                        forward = describe(forward.listen_label()),
                        err = err.to_string()
                    ))),
                }
            }
        }
        (this, report)
    }

    /// Accept local connections, open their channels and move data.
    /// Returns how many bytes came in from the server.
    pub fn service(&mut self, session: &Session) -> usize {
        for listener in &self.local {
            loop {
                match listener.socket.accept() {
                    Ok((stream, origin)) => {
                        if stream.set_nonblocking(true).is_ok() {
                            let _ = stream.set_nodelay(true);
                            self.pending.push_back(Pending { stream, origin, forward: listener.forward.clone() });
                        }
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) => {
                        log::debug!("forward {}: accept failed: {err}", listener.forward.listen_label());
                        break;
                    }
                }
            }
        }

        // One open at a time, retried with the same arguments until done.
        while let Some(pending) = self.pending.front() {
            let origin = (pending.origin.ip().to_string(), pending.origin.port());
            match session.channel_direct_tcpip(&pending.forward.host, pending.forward.host_port, Some((&origin.0, origin.1))) {
                Ok(channel) => {
                    let pending = self.pending.pop_front().expect("front exists");
                    self.tunnels.push(Tunnel::new(channel, pending.stream));
                }
                Err(err) if would_block(&err) => break,
                Err(err) => {
                    // Dropping the stream closes it: the client sees the
                    // connection refused, like with OpenSSH.
                    let pending = self.pending.pop_front().expect("front exists");
                    log::debug!("forward to {} refused: {err}", pending.forward.target_label());
                }
            }
        }

        let inbound = self.tunnels.iter_mut().map(Tunnel::service).sum();
        self.tunnels.retain_mut(|tunnel| !tunnel.finished());
        inbound
    }

    /// Take the connections the server reports on remote forwards and
    /// connect each to its local target. Call this last before sleeping:
    /// it drains what libssh2 can read, see the module docs.
    pub fn accept_remote(&mut self) {
        for listener in &mut self.remote {
            loop {
                match listener.listener.accept() {
                    Ok(channel) => {
                        let target = (listener.forward.host.as_str(), listener.forward.host_port);
                        match connect_local(target) {
                            Ok(stream) => self.tunnels.push(Tunnel::new(channel, stream)),
                            Err(err) => {
                                log::debug!("forward to {}: {err}", listener.forward.target_label());
                                self.tunnels.push(Tunnel::closing(channel));
                            }
                        }
                    }
                    Err(err) if would_block(&err) => break,
                    Err(err) => {
                        log::debug!("forward {}: accept failed: {err}", listener.forward.listen_label());
                        break;
                    }
                }
            }
        }
    }

    /// Some channel has data waiting inside libssh2 that the next round
    /// would pick up -- don't sleep.
    pub fn buffered(&self) -> bool {
        self.tunnels.iter().any(|t| !t.closing && t.to_local.len() < MAX_BUFFER && t.channel.read_window().available > 0)
    }

    /// Local sockets to wake up for. Channels need nothing extra: their
    /// data arrives on the session's socket.
    pub fn poll_fds(&self, fds: &mut Vec<(RawFd, libc::c_short)>) {
        fds.extend(self.local.iter().map(|l| (l.socket.as_raw_fd(), libc::POLLIN)));
        for tunnel in self.tunnels.iter().filter(|t| !t.closing) {
            let Some(stream) = &tunnel.stream else { continue };
            let mut events = 0;
            if !tunnel.local_eof && tunnel.to_remote.len() < MAX_BUFFER {
                events |= libc::POLLIN;
            }
            if !tunnel.to_local.is_empty() {
                events |= libc::POLLOUT;
            }
            fds.push((stream.as_raw_fd(), events));
        }
    }
}

/// Extended data is never read here, but `read_window().available`
/// counts it: merged, it can't make [`Forwards::buffered`] spin.
fn merge_stderr(mut channel: Channel) -> Channel {
    let _ = channel.handle_extended_data(ExtendedData::Merge);
    channel
}

impl Tunnel {
    fn new(channel: Channel, stream: TcpStream) -> Self {
        let _ = stream.set_nonblocking(true);
        let channel = merge_stderr(channel);
        Self {
            channel,
            stream: Some(stream),
            to_local: Vec::new(),
            to_remote: Vec::new(),
            local_eof: false,
            eof_sent: false,
            remote_done: false,
            closing: false,
        }
    }

    /// A channel with nothing to connect it to: close it right away.
    fn closing(channel: Channel) -> Self {
        Self {
            channel,
            stream: None,
            to_local: Vec::new(),
            to_remote: Vec::new(),
            local_eof: true,
            eof_sent: false,
            remote_done: true,
            closing: true,
        }
    }

    /// Move what can be moved; returns bytes received from the server.
    fn service(&mut self) -> usize {
        if self.closing {
            return 0;
        }
        match self.pump() {
            Ok(inbound) => inbound,
            Err(err) => {
                log::debug!("forwarded connection ended: {err}");
                self.closing = true;
                0
            }
        }
    }

    fn pump(&mut self) -> std::io::Result<usize> {
        let Some(stream) = self.stream.as_mut() else { return Err(std::io::Error::other("no local socket")) };
        let mut buf = [0u8; 16 * 1024];
        let mut inbound = 0;
        // Server → local.
        while self.to_local.len() < MAX_BUFFER {
            match self.channel.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    inbound += n;
                    self.to_local.extend_from_slice(&buf[..n]);
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        while !self.to_local.is_empty() {
            match stream.write(&self.to_local) {
                Ok(n) => drop(self.to_local.drain(..n)),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        if self.channel.eof() && self.to_local.is_empty() && !self.remote_done {
            let _ = stream.shutdown(Shutdown::Write);
            self.remote_done = true;
        }

        // Local → server.
        while !self.local_eof && self.to_remote.len() < MAX_BUFFER {
            match stream.read(&mut buf) {
                Ok(0) => self.local_eof = true,
                Ok(n) => self.to_remote.extend_from_slice(&buf[..n]),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        while !self.to_remote.is_empty() {
            match self.channel.write(&self.to_remote) {
                Ok(n) => drop(self.to_remote.drain(..n)),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        if self.local_eof && self.to_remote.is_empty() && !self.eof_sent {
            match self.channel.send_eof() {
                Ok(()) => self.eof_sent = true,
                Err(err) if would_block(&err) => {}
                Err(err) => return Err(std::io::Error::other(err)),
            }
        }
        if self.eof_sent && self.remote_done {
            self.closing = true;
        }
        Ok(inbound)
    }

    /// Closing waits for the server's close without blocking; only then
    /// can the channel be freed without libssh2 wanting to do I/O.
    fn finished(&mut self) -> bool {
        if !self.closing {
            return false;
        }
        !matches!(self.channel.close(), Err(err) if would_block(&err))
    }
}

/// Loopback by default -- both IPv4 and IPv6, like OpenSSH; `*` or an
/// empty address means all interfaces.
fn listen_locally(forward: &Forward) -> std::io::Result<Vec<TcpListener>> {
    let port = forward.port;
    let addrs: Vec<SocketAddr> = match forward.bind.as_deref() {
        None | Some("localhost") => vec![([127, 0, 0, 1], port).into(), (std::net::Ipv6Addr::LOCALHOST, port).into()],
        Some("" | "*") => vec![([0, 0, 0, 0], port).into()],
        Some(bind) => (bind, port).to_socket_addrs()?.collect(),
    };
    let mut sockets = Vec::new();
    let mut last_err = None;
    for addr in addrs {
        match TcpListener::bind(addr) {
            Ok(socket) => {
                socket.set_nonblocking(true)?;
                sockets.push(socket);
            }
            Err(err) => last_err = Some(err),
        }
    }
    match (sockets.is_empty(), last_err) {
        (true, Some(err)) => Err(err),
        (true, None) => Err(std::io::Error::other(t!("forward-no-address"))),
        _ => Ok(sockets),
    }
}

fn connect_local(target: (&str, u16)) -> std::io::Result<TcpStream> {
    let mut last_err = None;
    for addr in target.to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, LOCAL_CONNECT_TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other(t!("forward-no-address"))))
}
