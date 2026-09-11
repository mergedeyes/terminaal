//! Port forwarding (`LocalForward`, `RemoteForward`, `DynamicForward`) on
//! a tab's SSH session, driven by the connection's pump loop -- the
//! session is non-blocking and owned by that one thread, so every
//! forwarded connection is serviced from there, next to the shell channel.
//!
//! Local: we listen, on a port or a Unix socket, and each accepted
//! connection gets a channel to its target -- `direct-tcpip`, or
//! `direct-streamlocal` for a socket on the server. libssh2 keeps the
//! state of a channel being opened in the session, so only one open is in
//! flight at a time; the rest wait in a queue. Dynamic: the same, but
//! each connection names its target in a SOCKS handshake first
//! ([`super::socks`]). Remote: the server listens (TCP only -- libssh2
//! has no `streamlocal-forward`); each connection it reports is connected
//! to its local target here, or without one, is a SOCKS request that says
//! where to connect. Agent forwarding: the server opens a channel for each
//! use of the agent ([`super::agent_forward`]), tied here to a new
//! connection to the local agent's socket.
//!
//! libssh2 reads packets for *all* channels whenever it reads for one, so
//! data can end up buffered for a channel that was already serviced in
//! this round. [`Forwards::buffered`] tells the pump not to sleep then.

use std::collections::VecDeque;
use std::fs::Permissions;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ssh2::{Channel, ErrorCode, ExtendedData, Listener, Session};

use super::agent_forward::{AgentChannel, AgentForwarding};
use super::expand_tilde;
use super::options::{Forward, Listen, Target};
use super::socks::{self, Handshake, Step, Version};
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

/// A connection on this machine, TCP or Unix socket.
enum Stream {
    Tcp(TcpStream),
    Unix(UnixStream),
}

impl Stream {
    fn set_nonblocking(&self) -> io::Result<()> {
        match self {
            Stream::Tcp(stream) => stream.set_nonblocking(true),
            Stream::Unix(stream) => stream.set_nonblocking(true),
        }
    }

    fn shutdown_write(&self) {
        let _ = match self {
            Stream::Tcp(stream) => stream.shutdown(Shutdown::Write),
            Stream::Unix(stream) => stream.shutdown(Shutdown::Write),
        };
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(stream) => stream.read(buf),
            Stream::Unix(stream) => stream.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(stream) => stream.write(buf),
            Stream::Unix(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawFd for Stream {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Stream::Tcp(stream) => stream.as_raw_fd(),
            Stream::Unix(stream) => stream.as_raw_fd(),
        }
    }
}

/// What a local forward listens on.
enum Socket {
    Tcp(TcpListener),
    /// `ino` tells our socket file apart from one that replaced it since;
    /// only ours is removed when the forward ends.
    Unix { listener: UnixListener, path: PathBuf, ino: u64 },
}

impl Socket {
    /// The connection, and where it came from (for the server).
    fn accept(&self) -> io::Result<(Stream, (String, u16))> {
        match self {
            Socket::Tcp(listener) => listener.accept().map(|(stream, origin)| {
                let _ = stream.set_nodelay(true);
                (Stream::Tcp(stream), (origin.ip().to_string(), origin.port()))
            }),
            Socket::Unix { listener, .. } => {
                listener.accept().map(|(stream, _)| (Stream::Unix(stream), ("127.0.0.1".to_string(), 0)))
            }
        }
    }

    fn as_raw_fd(&self) -> RawFd {
        match self {
            Socket::Tcp(listener) => listener.as_raw_fd(),
            Socket::Unix { listener, .. } => listener.as_raw_fd(),
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        if let Socket::Unix { path, ino, .. } = self
            && std::fs::symlink_metadata(&*path).is_ok_and(|meta| meta.ino() == *ino)
        {
            let _ = std::fs::remove_file(&*path);
        }
    }
}

struct LocalListener {
    socket: Socket,
    forward: Forward,
}

struct RemoteListener {
    listener: Listener,
    forward: Forward,
}

/// A local connection waiting for its channel.
struct Pending {
    stream: Stream,
    origin: (String, u16),
    target: Target,
    /// Came through SOCKS: the client waits for the answer, after `unsent`
    /// (what's left of the handshake), and sent `early` after its request.
    socks: Option<Version>,
    unsent: Vec<u8>,
    early: Vec<u8>,
}

/// Either end of a SOCKS handshake: a local socket, or a channel from the
/// server.
trait Conn: Read + Write {
    /// After a read of 0 bytes: is the other side done?
    fn ended(&self) -> bool;
}

impl Conn for Stream {
    fn ended(&self) -> bool {
        true
    }
}

impl Conn for Channel {
    fn ended(&self) -> bool {
        self.eof()
    }
}

/// The server's side of a tunnel: a channel, or one libssh2 opened for the
/// forwarded agent.
trait Remote: Read + Write {
    fn eof(&self) -> bool;
    fn send_eof(&mut self) -> Result<(), ssh2::Error>;
    fn close(&mut self) -> Result<(), ssh2::Error>;
    /// libssh2 already holds data for it.
    fn has_data(&self) -> bool;
}

impl Remote for Channel {
    fn eof(&self) -> bool {
        Channel::eof(self)
    }

    fn send_eof(&mut self) -> Result<(), ssh2::Error> {
        Channel::send_eof(self)
    }

    fn close(&mut self) -> Result<(), ssh2::Error> {
        Channel::close(self)
    }

    fn has_data(&self) -> bool {
        self.read_window().available > 0
    }
}

impl Remote for AgentChannel {
    fn eof(&self) -> bool {
        AgentChannel::eof(self)
    }

    fn send_eof(&mut self) -> Result<(), ssh2::Error> {
        AgentChannel::send_eof(self)
    }

    fn close(&mut self) -> Result<(), ssh2::Error> {
        AgentChannel::close(self)
    }

    fn has_data(&self) -> bool {
        self.available()
    }
}

/// A connection to a dynamic forward, in its SOCKS handshake.
struct Negotiation<C> {
    conn: C,
    handshake: Handshake,
    /// Replies not sent yet.
    out: Vec<u8>,
    /// Refused: close once `out` is sent.
    refused: bool,
}

enum Progress {
    Waiting,
    Connect { host: String, port: u16, version: Version },
    Close,
}

impl<C: Conn> Negotiation<C> {
    fn new(conn: C) -> Self {
        Self { conn, handshake: Handshake::default(), out: Vec::new(), refused: false }
    }

    /// Send `reply`, then close.
    fn refuse(conn: C, reply: Vec<u8>) -> Self {
        Self { out: reply, refused: true, ..Self::new(conn) }
    }

    fn progress(&mut self) -> Progress {
        if !self.refused {
            let mut buf = [0u8; 1024];
            loop {
                match self.conn.read(&mut buf) {
                    Ok(0) if self.conn.ended() => return Progress::Close,
                    Ok(0) => break,
                    Ok(n) => self.handshake.push(&buf[..n]),
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => return Progress::Close,
                }
            }
            loop {
                match self.handshake.step() {
                    Step::NeedMore => break,
                    Step::Reply(reply) => self.out.extend(reply),
                    Step::Connect { host, port, version } => return Progress::Connect { host, port, version },
                    Step::Fail(reply) => {
                        self.out.extend(reply);
                        self.refused = true;
                        break;
                    }
                }
            }
        }
        match flush(&mut self.conn, &mut self.out) {
            Err(_) => Progress::Close,
            Ok(()) if self.refused && self.out.is_empty() => Progress::Close,
            Ok(()) => Progress::Waiting,
        }
    }

    /// After `Connect`: the connection, replies still owed before the
    /// answer to the request, and what the client sent after it.
    fn finish(self) -> (C, Vec<u8>, Vec<u8>) {
        (self.conn, self.out, self.handshake.into_rest())
    }
}

/// One forwarded connection: a channel and the local socket it's tied to.
struct Tunnel {
    channel: Box<dyn Remote>,
    /// `None` for a channel that had nowhere to go and is only being closed.
    stream: Option<Stream>,
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
    /// Local connections to a dynamic forward, still in the handshake.
    socks_local: Vec<(Negotiation<Stream>, (String, u16))>,
    /// Connections to a SOCKS proxy on the server, still in the handshake.
    socks_remote: Vec<Negotiation<Channel>>,
    pending: VecDeque<Pending>,
    tunnels: Vec<Tunnel>,
    agent: Option<AgentForwarding>,
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
            let failed =
                |err: String| Err(t!("forward-failed", forward = describe(forward.listen_label()), err = err));
            if forward.remote {
                let Listen::Port { bind, port } = &forward.listen else {
                    report.push(failed(t!("opt-forward-remote-unix")));
                    continue;
                };
                let address = match bind.as_deref() {
                    None => "localhost",
                    Some("*") => "",
                    Some(bind) => bind,
                };
                match session.channel_forward_listen(*port, Some(address), None) {
                    Ok((listener, bound)) => {
                        let shown = Forward { listen: Listen::Port { bind: bind.clone(), port: bound }, ..forward.clone() };
                        report.push(Ok(t!("forward-up", forward = describe(shown.listen_label()))));
                        this.remote.push(RemoteListener { listener, forward: forward.clone() });
                    }
                    Err(err) => report.push(failed(err.to_string())),
                }
            } else {
                match listen_locally(forward) {
                    Ok(sockets) => {
                        report.push(Ok(t!("forward-up", forward = describe(forward.listen_label()))));
                        this.local.extend(sockets.into_iter().map(|socket| LocalListener { socket, forward: forward.clone() }));
                    }
                    Err(err) => report.push(failed(err.to_string())),
                }
            }
        }
        (this, report)
    }

    /// Accept local connections, run SOCKS handshakes, open channels and
    /// move data. Returns how many bytes came in from the server.
    pub fn service(&mut self, session: &Session) -> usize {
        for listener in &self.local {
            loop {
                match listener.socket.accept() {
                    Ok((stream, origin)) => {
                        if stream.set_nonblocking().is_err() {
                            continue;
                        }
                        match &listener.forward.target {
                            Some(target) => self.pending.push_back(Pending {
                                stream,
                                origin,
                                target: target.clone(),
                                socks: None,
                                unsent: Vec::new(),
                                early: Vec::new(),
                            }),
                            None => self.socks_local.push((Negotiation::new(stream), origin)),
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

        for (mut negotiation, origin) in std::mem::take(&mut self.socks_local) {
            match negotiation.progress() {
                Progress::Waiting => self.socks_local.push((negotiation, origin)),
                Progress::Close => {}
                Progress::Connect { host, port, version } => {
                    let (stream, unsent, early) = negotiation.finish();
                    let target = Target::Tcp { host, port };
                    self.pending.push_back(Pending { stream, origin, target, socks: Some(version), unsent, early });
                }
            }
        }
        for mut negotiation in std::mem::take(&mut self.socks_remote) {
            match negotiation.progress() {
                Progress::Waiting => self.socks_remote.push(negotiation),
                Progress::Close => self.tunnels.push(Tunnel::closing(negotiation.conn)),
                Progress::Connect { host, port, version } => {
                    let (channel, mut reply, early) = negotiation.finish();
                    match connect_local(&Target::Tcp { host, port }) {
                        Ok(stream) => {
                            reply.extend(socks::reply(version, true));
                            self.tunnels.push(Tunnel { to_local: early, to_remote: reply, ..Tunnel::new(channel, stream) });
                        }
                        Err(err) => {
                            log::debug!("SOCKS request from the server: {err}");
                            reply.extend(socks::reply(version, false));
                            // Now: nothing would wake the pump for it later.
                            // What doesn't fit waits for the session socket
                            // to take it (POLLOUT).
                            let mut refusal = Negotiation::refuse(channel, reply);
                            match refusal.progress() {
                                Progress::Waiting => self.socks_remote.push(refusal),
                                _ => self.tunnels.push(Tunnel::closing(refusal.conn)),
                            }
                        }
                    }
                }
            }
        }

        // One open at a time, retried with the same arguments until done.
        while let Some(pending) = self.pending.front() {
            let origin = Some((pending.origin.0.as_str(), pending.origin.1));
            let opened = match &pending.target {
                Target::Tcp { host, port } => session.channel_direct_tcpip(host, *port, origin),
                Target::Unix(path) => session.channel_direct_streamlocal(path, origin),
            };
            match opened {
                Ok(channel) => {
                    let pending = self.pending.pop_front().expect("front exists");
                    let mut to_local = pending.unsent;
                    if let Some(version) = pending.socks {
                        to_local.extend(socks::reply(version, true));
                    }
                    let tunnel = Tunnel::new(channel, pending.stream);
                    self.tunnels.push(Tunnel { to_local, to_remote: pending.early, ..tunnel });
                }
                Err(err) if would_block(&err) => break,
                Err(err) => {
                    // Dropping the stream closes it: the client sees the
                    // connection refused, like with OpenSSH.
                    let mut pending = self.pending.pop_front().expect("front exists");
                    log::debug!("forward to {} refused: {err}", pending.target.spec());
                    if let Some(version) = pending.socks {
                        pending.unsent.extend(socks::reply(version, false));
                        let _ = pending.stream.write_all(&pending.unsent);
                    }
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
                    Ok(channel) => match &listener.forward.target {
                        None => self.socks_remote.push(Negotiation::new(merge_stderr(channel))),
                        Some(target) => match connect_local(target) {
                            Ok(stream) => self.tunnels.push(Tunnel::new(channel, stream)),
                            Err(err) => {
                                log::debug!("forward to {}: {err}", listener.forward.target_label());
                                self.tunnels.push(Tunnel::closing(channel));
                            }
                        },
                    },
                    Err(err) if would_block(&err) => break,
                    Err(err) => {
                        log::debug!("forward {}: accept failed: {err}", listener.forward.listen_label());
                        break;
                    }
                }
            }
        }
        // Last: the accepts above may have read an agent request.
        if let Some(agent) = &self.agent {
            for channel in agent.take() {
                match UnixStream::connect(agent.socket()) {
                    Ok(stream) => self.tunnels.push(Tunnel::over(Box::new(channel), Stream::Unix(stream))),
                    Err(err) => {
                        log::debug!("agent {}: {err}", agent.socket().display());
                        self.tunnels.push(Tunnel::closing(channel));
                    }
                }
            }
        }
    }

    /// Tie the server's agent channels to the local agent from now on.
    pub fn forward_agent(&mut self, forwarding: AgentForwarding) {
        self.agent = Some(forwarding);
    }

    /// Some channel has data waiting inside libssh2 that the next round
    /// would pick up -- don't sleep.
    pub fn buffered(&self) -> bool {
        self.tunnels.iter().any(|t| !t.closing && t.to_local.len() < MAX_BUFFER && t.channel.has_data())
            || self.socks_remote.iter().any(|n| !n.refused && n.conn.read_window().available > 0)
    }

    /// Local sockets to wake up for. Channels need nothing extra: their
    /// data arrives on the session's socket.
    pub fn poll_fds(&self, fds: &mut Vec<(RawFd, libc::c_short)>) {
        fds.extend(self.local.iter().map(|l| (l.socket.as_raw_fd(), libc::POLLIN)));
        for (negotiation, _) in &self.socks_local {
            let mut events = 0;
            if !negotiation.refused {
                events |= libc::POLLIN;
            }
            if !negotiation.out.is_empty() {
                events |= libc::POLLOUT;
            }
            fds.push((negotiation.conn.as_raw_fd(), events));
        }
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

/// Write as much of `out` as `conn` takes now.
fn flush(conn: &mut impl Write, out: &mut Vec<u8>) -> io::Result<()> {
    while !out.is_empty() {
        match conn.write(out) {
            Ok(0) => return Err(ErrorKind::WriteZero.into()),
            Ok(n) => drop(out.drain(..n)),
            Err(err) if err.kind() == ErrorKind::WouldBlock => break,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

impl Tunnel {
    fn new(channel: Channel, stream: Stream) -> Self {
        Self::over(Box::new(merge_stderr(channel)), stream)
    }

    fn over(channel: Box<dyn Remote>, stream: Stream) -> Self {
        let _ = stream.set_nonblocking();
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
    fn closing(channel: impl Remote + 'static) -> Self {
        Self {
            channel: Box::new(channel),
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

    fn pump(&mut self) -> io::Result<usize> {
        let Some(stream) = self.stream.as_mut() else { return Err(io::Error::other("no local socket")) };
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
        flush(stream, &mut self.to_local)?;
        if self.channel.eof() && self.to_local.is_empty() && !self.remote_done {
            stream.shutdown_write();
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
        flush(&mut self.channel, &mut self.to_remote)?;
        if self.local_eof && self.to_remote.is_empty() && !self.eof_sent {
            match self.channel.send_eof() {
                Ok(()) => self.eof_sent = true,
                Err(err) if would_block(&err) => {}
                Err(err) => return Err(io::Error::other(err)),
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
fn listen_locally(forward: &Forward) -> io::Result<Vec<Socket>> {
    let (bind, port) = match &forward.listen {
        Listen::Unix(path) => return listen_unix(&expand_tilde(path)).map(|socket| vec![socket]),
        Listen::Port { bind, port } => (bind.as_deref(), *port),
    };
    let addrs: Vec<SocketAddr> = match bind {
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
                sockets.push(Socket::Tcp(socket));
            }
            Err(err) => last_err = Some(err),
        }
    }
    match (sockets.is_empty(), last_err) {
        (true, Some(err)) => Err(err),
        (true, None) => Err(io::Error::other(t!("forward-no-address"))),
        _ => Ok(sockets),
    }
}

/// A socket file nobody listens on any more (left behind by a crash) is
/// replaced, one in use is not. The socket is for this user only (0600,
/// OpenSSH's default `StreamLocalBindMask`) -- set right after binding;
/// a umask would be process-wide and hit files other threads create.
fn listen_unix(path: &Path) -> io::Result<Socket> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket())
        && UnixStream::connect(path).is_err_and(|err| err.kind() == ErrorKind::ConnectionRefused)
    {
        log::info!("replacing stale socket {}", path.display());
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    listener.set_nonblocking(true)?;
    let ino = std::fs::symlink_metadata(path)?.ino();
    let socket = Socket::Unix { listener, path: path.to_path_buf(), ino };
    std::fs::set_permissions(path, Permissions::from_mode(0o600))?;
    Ok(socket)
}

fn connect_local(target: &Target) -> io::Result<Stream> {
    let (host, port) = match target {
        Target::Unix(path) => return UnixStream::connect(expand_tilde(path)).map(Stream::Unix),
        Target::Tcp { host, port } => (host.as_str(), *port),
    };
    let mut last_err = None;
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, LOCAL_CONNECT_TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(Stream::Tcp(stream));
            }
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::other(t!("forward-no-address"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        // Short: socket paths are limited to ~107 bytes.
        std::env::temp_dir().join(format!("tf-{}-{name}", std::process::id()))
    }

    #[test]
    fn unix_listener_replaces_stale_sockets_and_cleans_up() {
        let path = scratch("a.sock");
        // Left behind: bound, then closed without removing the file.
        drop(UnixListener::bind(&path).unwrap());
        let socket = listen_unix(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        // One that's in use stays.
        assert!(listen_unix(&path).is_err());
        UnixStream::connect(&path).unwrap();
        drop(socket);
        assert!(!path.exists(), "our socket file is removed");
    }

    #[test]
    fn unix_listener_leaves_files_it_did_not_create() {
        let path = scratch("b.sock");
        std::fs::write(&path, "not a socket").unwrap();
        assert!(listen_unix(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a socket");

        // Replaced by someone else while ours was up: theirs stays.
        std::fs::remove_file(&path).unwrap();
        let socket = listen_unix(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let theirs = UnixListener::bind(&path).unwrap();
        drop(socket);
        assert!(path.exists());
        drop(theirs);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn local_socks_negotiation_waits_for_the_whole_request() {
        let (ours, mut client) = UnixStream::pair().unwrap();
        ours.set_nonblocking(true).unwrap();
        let mut negotiation = Negotiation::new(Stream::Unix(ours));
        assert!(matches!(negotiation.progress(), Progress::Waiting));

        client.write_all(&[5, 1, 0]).unwrap();
        assert!(matches!(negotiation.progress(), Progress::Waiting));
        let mut answer = [0u8; 2];
        client.read_exact(&mut answer).unwrap();
        assert_eq!(answer, [5, 0]);

        client.write_all(&[5, 1, 0, 3, 4, b'h', b'o', b's', b't', 0, 80, b'x']).unwrap();
        match negotiation.progress() {
            Progress::Connect { host, port, version } => assert_eq!((host.as_str(), port, version), ("host", 80, Version::V5)),
            _ => panic!("expected a connect"),
        }
        let (_, unsent, early) = negotiation.finish();
        assert!(unsent.is_empty());
        assert_eq!(early, b"x");
    }

    #[test]
    fn refused_socks_negotiation_answers_then_closes() {
        let (ours, mut client) = UnixStream::pair().unwrap();
        ours.set_nonblocking(true).unwrap();
        let mut negotiation = Negotiation::new(Stream::Unix(ours));
        // Only username/password offered.
        client.write_all(&[5, 1, 2]).unwrap();
        assert!(matches!(negotiation.progress(), Progress::Close));
        let mut answer = [0u8; 2];
        client.read_exact(&mut answer).unwrap();
        assert_eq!(answer, [5, 0xff]);

        // A client that hangs up mid-handshake.
        let (ours, client) = UnixStream::pair().unwrap();
        ours.set_nonblocking(true).unwrap();
        drop(client);
        assert!(matches!(Negotiation::new(Stream::Unix(ours)).progress(), Progress::Close));
    }
}
