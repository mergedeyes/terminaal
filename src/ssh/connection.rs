//! One SSH session in a tab: a worker thread that connects with libssh2
//! (`ssh2`), then pumps bytes between the remote shell's PTY channel and
//! the tab's `Term` -- the job alacritty_terminal's own event loop does
//! for a local PTY (its `event_loop.rs`, which this mirrors, including
//! the synchronized-update timeout).
//!
//! Everything interactive happens *inside the tab*, the way `ssh` does
//! it: progress is printed into the terminal, and the host-key question,
//! passphrases and passwords are typed there (secrets without echo).
//! Nothing secret is ever stored.
//!
//! With ProxyJump, each hop is a full session of its own (host key check,
//! authentication -- all in the same tab). The next hop's transport is a
//! `direct-tcpip` channel on the previous one, handed to libssh2 as one
//! end of a socketpair; a small forwarder thread per hop moves the bytes
//! between the channel and the other end.
//!
//! The thread owns the libssh2 sessions outright. The app only talks to
//! it through [`SshHandle`]: an mpsc queue, plus a socketpair whose only
//! job is to wake the thread's `poll()` when something was queued.
//! Dropping the handle (closing the tab) ends the thread.
//!
//! A `ProxyCommand` replaces the first hop's TCP connection the same way
//! a jump does: the command's stdin/stdout are one end of a socketpair.
//! Port forwards ([`super::forward`]) run on the target's session inside
//! the pump loop, and keepalives double as a dead-connection check
//! (`ServerAliveCountMax`).

use std::fs::OpenOptions;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::Processor;
use ssh2::{
    BlockDirections, Channel, CheckResult, ErrorCode, ExtendedData, HashType, HostKeyType, KeyboardInteractivePrompt,
    KnownHostFileKind, MethodType, Prompt, Session,
};

use crate::i18n::t;
use crate::ssh::agent_forward::AgentForwarding;
use crate::ssh::forward::Forwards;
use crate::ssh::options::{AlgorithmKind, AuthMethod, ForwardAgent, HostKeyCheck, algorithm_list};
use crate::ssh::{AgentSocket, AuthPlan, SshTarget, default_key_files, display_path, expand_tilde, ssh_dir};
use crate::terminal::listener::EventProxyListener;

/// For the blocking phase (handshake, auth); the pump loop is non-blocking.
const SESSION_TIMEOUT_MS: u32 = 20_000;
/// How long to sleep when nothing at all is scheduled (no keepalives).
const IDLE_POLL: Duration = Duration::from_secs(3600);
/// How much of a `ProxyCommand`'s stderr to keep for error messages.
const MAX_PROXY_STDERR: usize = 4096;
/// Don't hold the terminal hostage to one output flood; let input and
/// redraws through in between.
const MAX_READ_PER_LOOP: usize = 1 << 20;
/// Per-direction buffer limit of a jump forwarder before it stops reading.
const MAX_FORWARD_BUFFER: usize = 256 * 1024;
/// libssh2's `LIBSSH2_ERROR_FILE`: the private key couldn't be read --
/// e.g. it's encrypted and there's no public key file next to it.
const LIBSSH2_ERROR_FILE: i32 = -16;
/// libssh2's `LIBSSH2_ERROR_PUBLICKEY_UNVERIFIED`: the server accepted the
/// offered public key, but signing with the private one failed -- for an
/// encrypted key, because it wasn't decrypted (`userauth.c`, "Callback
/// returned error").
const LIBSSH2_ERROR_PUBLICKEY_UNVERIFIED: i32 = -19;
/// libssh2's `LIBSSH2_ERROR_EAGAIN`, i.e. "would block".
const LIBSSH2_ERROR_EAGAIN: i32 = -37;

type SharedTerm = Arc<FairMutex<Term<EventProxyListener>>>;

enum Msg {
    Input(Vec<u8>),
    Resize(WindowSize),
}

pub struct SshHandle {
    tx: Sender<Msg>,
    wake: UnixStream,
}

impl SshHandle {
    pub fn send_input(&self, bytes: Vec<u8>) {
        self.send(Msg::Input(bytes));
    }

    pub fn resize(&self, size: WindowSize) {
        self.send(Msg::Resize(size));
    }

    fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_ok() {
            // Non-blocking: if the socket buffer is full, a wakeup is
            // pending anyway.
            let _ = (&self.wake).write(&[1]);
        }
    }
}

/// Start connecting in the background; the tab shows how it goes.
pub fn spawn(
    target: SshTarget,
    term: SharedTerm,
    listener: EventProxyListener,
    size: WindowSize,
) -> io::Result<SshHandle> {
    let (tx, rx) = mpsc::channel();
    let (wake_tx, wake_rx) = UnixStream::pair()?;
    wake_tx.set_nonblocking(true)?;
    let name = format!("ssh {}", target.label);
    let worker = Worker { target, term, listener, parser: Processor::new(), rx, wake: wake_rx, size, proxy: None };
    std::thread::Builder::new().name(name).spawn(move || worker.run())?;
    Ok(SshHandle { tx, wake: wake_tx })
}

/// How a connection ended without an error.
enum End {
    /// The remote shell exited: close the tab, like a local shell's exit.
    Exited,
    /// The tab is gone already.
    TabClosed,
}

enum Stop {
    /// Shown in the tab until a key is pressed.
    Failed(String),
    /// Ctrl+C/Ctrl+D at a prompt: close the tab, like `ssh` exits.
    Cancelled,
    TabClosed,
}

impl From<ssh2::Error> for Stop {
    fn from(err: ssh2::Error) -> Self {
        Stop::Failed(t!("conn-ssh-error", err = err.to_string()))
    }
}

impl From<io::Error> for Stop {
    fn from(err: io::Error) -> Self {
        Stop::Failed(t!("conn-io-error", err = err.to_string()))
    }
}

/// What a session runs over: TCP for the first hop, a socketpair end
/// (fed by a forwarder thread) behind a jump host.
enum Transport {
    Tcp(TcpStream),
    Tunnel(UnixStream),
}

impl Transport {
    fn raw_fd(&self) -> RawFd {
        match self {
            Transport::Tcp(stream) => stream.as_raw_fd(),
            Transport::Tunnel(stream) => stream.as_raw_fd(),
        }
    }
}

struct Worker {
    target: SshTarget,
    term: SharedTerm,
    listener: EventProxyListener,
    parser: Processor,
    rx: Receiver<Msg>,
    /// Read end of the wakeup socketpair; see the module docs.
    wake: UnixStream,
    size: WindowSize,
    proxy: Option<Proxy>,
}

/// A running `ProxyCommand`, and what it wrote to stderr so far.
struct Proxy {
    child: Child,
    stderr: Arc<Mutex<String>>,
}

fn secs(n: u32) -> Duration {
    Duration::from_secs(n.into())
}

impl Worker {
    fn run(mut self) {
        let result = self.connect_and_pump();
        if let Some(mut proxy) = self.proxy.take() {
            let _ = proxy.child.kill();
            let _ = proxy.child.wait();
        }
        match result {
            Ok(End::Exited) | Err(Stop::Cancelled) => self.term.lock().exit(),
            Ok(End::TabClosed) | Err(Stop::TabClosed) => {}
            Err(Stop::Failed(message)) => {
                log::info!("{}: {message}", self.target.label);
                self.print(&format!("\n\x1b[31m{message}\x1b[0m\n\x1b[2m{}\x1b[0m", t!("conn-any-key-closes")));
                if self.wait_for_key() {
                    self.term.lock().exit();
                }
            }
        }
    }

    fn connect_and_pump(&mut self) -> Result<End, Stop> {
        // The hops in order, ending with the target itself.
        let mut chain = self.target.jumps.clone();
        chain.push(SshTarget { jumps: Vec::new(), ..self.target.clone() });
        let via: Vec<&str> = chain[..chain.len() - 1].iter().map(|hop| hop.label.as_str()).collect();
        let connecting = if via.is_empty() {
            t!("conn-connecting", target = &self.target.label)
        } else {
            t!("conn-connecting-via", target = &self.target.label, hops = via.join(" → "))
        };
        self.print(&format!("\x1b[2m{connecting}\x1b[0m\n"));

        let mut transport = match &chain[0].settings.proxy_command {
            Some(command) => Transport::Tunnel(self.spawn_proxy(command)?),
            None => Transport::Tcp(self.connect_tcp(&chain[0])?),
        };
        for (i, hop) in chain.iter().enumerate() {
            let (session, socket) = self.establish(hop, transport)?;
            let Some(next) = chain.get(i + 1) else {
                return self.open_shell(session, socket);
            };
            let channel = session.channel_direct_tcpip(&next.host, next.port, None).map_err(|err| {
                Stop::Failed(t!(
                    "conn-tunnel-failed",
                    hop = &hop.label,
                    host = &next.host,
                    port = next.port,
                    err = err.to_string()
                ))
            })?;
            let (near, far) = UnixStream::pair()?;
            spawn_forwarder(session, channel, socket, near, hop.settings.keepalive.map(|(interval, _)| interval))?;
            transport = Transport::Tunnel(far);
        }
        unreachable!("the chain always ends with the target")
    }

    /// Handshake, host key check and authentication for one hop.
    fn establish(&mut self, hop: &SshTarget, transport: Transport) -> Result<(Session, RawFd), Stop> {
        // The session takes ownership of the stream but keeps it (and so
        // the fd) alive as long as itself.
        let socket = transport.raw_fd();
        let mut session = Session::new()?;
        match transport {
            Transport::Tcp(stream) => session.set_tcp_stream(stream),
            Transport::Tunnel(stream) => session.set_tcp_stream(stream),
        }
        session.set_timeout(SESSION_TIMEOUT_MS);
        if hop.settings.compression {
            session.set_compress(true);
        }
        set_algorithms(&session, hop)?;
        session.handshake().map_err(|err| {
            let mut message = t!("conn-handshake-failed", hop = &hop.label, err = err.to_string());
            if let Some(output) = self.proxy_output() {
                message.push('\n');
                message.push_str(&t!("conn-proxy-says", output = output));
            }
            Stop::Failed(message)
        })?;
        self.verify_host_key(&session, hop)?;
        self.authenticate(&session, hop)?;
        log::debug!("{}: authenticated", hop.label);
        Ok((session, socket))
    }

    fn open_shell(&mut self, session: Session, socket: RawFd) -> Result<End, Stop> {
        let settings = self.target.settings.clone();
        let (mut forwards, report) = Forwards::start(&session, &settings.forwards);
        for line in report {
            match line {
                Ok(text) => self.print(&format!("\x1b[2m{text}\x1b[0m\n")),
                Err(text) => self.print(&format!("\x1b[33m{text}\x1b[0m\n")),
            }
        }

        let mut channel = session.channel_session()?;
        // Anything the server sends as stderr belongs in the tab as well
        // (and must be read, or the pump would think data is waiting).
        channel.handle_extended_data(ExtendedData::Merge)?;
        let WindowSize { num_cols, num_lines, cell_width, cell_height } = self.size;
        let (cols, lines) = (u32::from(num_cols), u32::from(num_lines));
        channel.request_pty(
            &settings.term,
            None,
            Some((cols, lines, cols * u32::from(cell_width), lines * u32::from(cell_height))),
        )?;
        // Servers take only what their AcceptEnv allows; the rest is
        // refused quietly, as with OpenSSH.
        for (name, value) in &settings.env {
            if let Err(err) = channel.setenv(name, value) {
                log::debug!("{}: {name} not accepted: {err}", self.target.label);
            }
        }
        if settings.forward_agent != ForwardAgent::Off {
            self.forward_agent(&session, &mut channel, &mut forwards);
        }
        match &settings.remote_command {
            Some(command) => channel.exec(command)?,
            None => channel.shell()?,
        }
        if let Some((interval, _)) = settings.keepalive {
            session.set_keepalive(true, interval);
        }
        log::debug!("{}: shell started", self.target.label);

        let end = self.pump(&session, &mut channel, socket, &mut forwards)?;
        // Say goodbye properly; best effort, the other side may be gone.
        session.set_blocking(true);
        session.set_timeout(2_000);
        let _ = channel.close();
        let _ = session.disconnect(None, &t!("conn-session-closed"), None);
        // Only now: on a live session, freeing forwarded channels and
        // listeners would wait for the server one by one.
        drop(forwards);
        Ok(end)
    }

    /// `ForwardAgent`: ask the server to forward the agent, and say in the
    /// tab how that went. The agent's channels are handled with the
    /// forwards.
    fn forward_agent(&mut self, session: &Session, channel: &mut Channel, forwards: &mut Forwards) {
        let socket = match self.agent_socket() {
            Ok(socket) => socket,
            Err(reason) => {
                self.print(&format!("\x1b[33m{}\x1b[0m\n", t!("conn-agent-forward-failed", err = reason)));
                return;
            }
        };
        // Accepting before asking: the server may use it right away.
        let forwarding = AgentForwarding::enable(session, socket);
        match channel.request_auth_agent_forwarding() {
            Ok(()) => {
                let socket = display_path(forwarding.socket());
                self.print(&format!("\x1b[2m{}\x1b[0m\n", t!("conn-agent-forward-up", socket = socket)));
                forwards.forward_agent(forwarding);
            }
            Err(err) => {
                log::debug!("{}: agent forwarding refused: {err}", self.target.label);
                let reason = t!("conn-agent-forward-refused");
                self.print(&format!("\x1b[33m{}\x1b[0m\n", t!("conn-agent-forward-failed", err = reason)));
            }
        }
    }

    /// The agent to forward: `ForwardAgent`'s own socket, or the one used
    /// to log in. `Err` says why there's none.
    fn agent_socket(&self) -> Result<PathBuf, String> {
        let from_env = |var: &str| {
            std::env::var_os(var)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| t!("conn-agent-forward-unset", var = var))
        };
        let socket = match (&self.target.settings.forward_agent, &self.target.auth.agent) {
            (ForwardAgent::Socket(spec), _) => match spec.strip_prefix('$') {
                Some(var) => from_env(var)?,
                None => expand_tilde(spec),
            },
            (_, AgentSocket::Path(path)) => path.clone(),
            (_, AgentSocket::Env) => from_env("SSH_AUTH_SOCK")?,
            (_, AgentSocket::Off) => return Err(t!("conn-agent-forward-off")),
        };
        if !socket.exists() {
            return Err(t!("conn-agent-forward-missing", socket = display_path(&socket)));
        }
        Ok(socket)
    }

    fn connect_tcp(&self, hop: &SshTarget) -> Result<TcpStream, Stop> {
        let (host, port) = (hop.host.as_str(), hop.port);
        let family = hop.settings.address_family;
        let addrs: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|err| Stop::Failed(t!("conn-resolve-failed", host = host, err = err.to_string())))?
            .filter(|addr| family.allows(addr))
            .collect();
        let mut last_err = None;
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, hop.settings.connect_timeout) {
                Ok(stream) => {
                    let _ = stream.set_nodelay(true);
                    return Ok(stream);
                }
                Err(err) => last_err = Some(err),
            }
        }
        Err(Stop::Failed(match last_err {
            Some(err) => t!("conn-connect-failed", host = host, port = port, err = err.to_string()),
            None => t!("conn-no-address", host = host, family = family.label()),
        }))
    }

    /// `ProxyCommand`: it runs with one end of a socketpair as stdin and
    /// stdout, and the session gets the other -- like OpenSSH, including
    /// the `exec`, so no shell lingers in between.
    fn spawn_proxy(&mut self, command: &str) -> Result<UnixStream, Stop> {
        self.print(&format!("\x1b[2mProxyCommand: {command}\x1b[0m\n"));
        let (ours, theirs) = UnixStream::pair()?;
        let stdin = OwnedFd::from(theirs.try_clone()?);
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("exec {command}"))
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(OwnedFd::from(theirs)))
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| Stop::Failed(t!("conn-proxy-start-failed", command = command, err = err.to_string())))?;
        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(mut pipe) = child.stderr.take() {
            let sink = stderr.clone();
            std::thread::Builder::new().name("ssh proxy stderr".into()).spawn(move || {
                let mut buf = [0u8; 1024];
                while let Ok(n @ 1..) = pipe.read(&mut buf) {
                    let mut text = sink.lock().unwrap_or_else(PoisonError::into_inner);
                    if text.len() < MAX_PROXY_STDERR {
                        text.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
            })?;
        }
        self.proxy = Some(Proxy { child, stderr });
        Ok(ours)
    }

    /// What the `ProxyCommand` wrote to stderr -- after a moment, so a
    /// command that just failed has had the chance to say why.
    fn proxy_output(&self) -> Option<String> {
        let proxy = self.proxy.as_ref()?;
        std::thread::sleep(Duration::from_millis(100));
        let text = proxy.stderr.lock().unwrap_or_else(PoisonError::into_inner).trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    /// Check the server's key against `~/.ssh/known_hosts` (or the
    /// host's `UserKnownHostsFile`) the way OpenSSH does: known and
    /// matching → go on; changed → refuse; unknown → ask (or, per
    /// `StrictHostKeyChecking`, refuse or accept without asking), then
    /// remember it.
    fn verify_host_key(&mut self, session: &Session, hop: &SshTarget) -> Result<(), Stop> {
        let (key, key_type) = session
            .host_key()
            .map(|(key, key_type)| (key.to_vec(), key_type))
            .ok_or_else(|| Stop::Failed(t!("conn-no-host-key")))?;
        let fingerprint = session
            .host_key_hash(HashType::Sha256)
            .map(|hash| format!("SHA256:{}", base64(hash).trim_end_matches('=')))
            .unwrap_or_default();
        let kind = key_type_name(key_type).map_or_else(|| t!("conn-unknown-type"), str::to_string);
        let (host, port) = (hop.host.as_str(), hop.port);
        let entry = if port == 22 { host.to_string() } else { format!("[{host}]:{port}") };

        let custom_file = hop.settings.known_hosts.clone();
        let known_hosts = custom_file.clone().or_else(|| ssh_dir().map(|dir| dir.join("known_hosts")));
        let file = known_hosts.as_deref().map_or_else(|| "known_hosts".to_string(), display_path);
        let mut known = session.known_hosts()?;
        if let Some(text) = known_hosts.as_ref().and_then(|path| std::fs::read_to_string(path).ok()) {
            // Line by line: libssh2 rejects the whole file over one line it
            // doesn't understand (certificate authorities, newer key types).
            for line in text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
                let _ = known.read_str(line, KnownHostFileKind::OpenSSH);
            }
        }

        match known.check_port(host, port, &key) {
            CheckResult::Match => Ok(()),
            CheckResult::Mismatch => Err(Stop::Failed(t!(
                "conn-host-key-changed",
                entry = &entry,
                kind = &kind,
                fingerprint = &fingerprint,
                file_option = custom_file.map(|path| format!(" -f '{}'", path.display())).unwrap_or_default()
            ))),
            CheckResult::NotFound | CheckResult::Failure => {
                let check = hop.settings.host_key_check;
                match check {
                    HostKeyCheck::Yes => {
                        return Err(Stop::Failed(t!(
                            "conn-host-key-refused",
                            entry = &entry,
                            file = &file,
                            kind = &kind,
                            fingerprint = &fingerprint
                        )));
                    }
                    HostKeyCheck::AcceptNew => {}
                    HostKeyCheck::Ask => {
                        let question = t!(
                            "conn-host-key-question",
                            entry = &entry,
                            kind = &kind,
                            fingerprint = &fingerprint,
                            file = &file
                        );
                        self.print(&format!("{question} "));
                        let answer = self.read_line(true)?.trim().to_lowercase();
                        if !matches!(answer.as_str(), "ja" | "j" | "yes" | "y") {
                            return Err(Stop::Failed(t!("conn-host-key-rejected")));
                        }
                    }
                }
                match known_hosts.as_ref().map(|path| append_known_host(path, &entry, key_type, &key)) {
                    Some(Err(err)) => {
                        let message = t!("conn-known-hosts-failed", file = &file, err = err.to_string());
                        self.print(&format!("\x1b[33m{message}\x1b[0m\n"));
                    }
                    Some(Ok(())) if check == HostKeyCheck::AcceptNew => {
                        let message = t!(
                            "conn-host-key-saved",
                            entry = &entry,
                            kind = &kind,
                            fingerprint = &fingerprint,
                            file = &file
                        );
                        self.print(&format!("\x1b[2m{message}\x1b[0m\n"));
                    }
                    _ => {}
                }
                Ok(())
            }
        }
    }

    /// The methods of `PreferredAuthentications` (default: keys,
    /// keyboard-interactive, password) that the server offers, in that
    /// order. Keys: agent → key files → (unless restricted) the default
    /// key files, see [`AuthPlan`].
    fn authenticate(&mut self, session: &Session, hop: &SshTarget) -> Result<(), Stop> {
        let user = hop.user.clone();
        let plan = hop.auth.clone();
        let methods = match session.auth_methods(&user) {
            Ok(methods) => methods.to_string(),
            // The server accepted "none" -- no authentication needed.
            Err(_) if session.authenticated() => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        let offered = |name: &str| methods.split(',').any(|method| method.trim() == name);

        let (mut agent_offered, mut asked_interactively) = (0, false);
        for method in &hop.settings.auth_methods {
            match method {
                AuthMethod::PublicKey if offered("publickey") => {
                    let authenticated;
                    (authenticated, agent_offered) = self.try_public_keys(session, &user, &plan)?;
                    if authenticated {
                        return Ok(());
                    }
                }
                AuthMethod::KeyboardInteractive if offered("keyboard-interactive") => {
                    asked_interactively = true;
                    for _ in 0..3 {
                        let mut prompter = Prompter { worker: self, stop: None };
                        let accepted = session.userauth_keyboard_interactive(&user, &mut prompter).is_ok();
                        if let Some(stop) = prompter.stop {
                            return Err(stop);
                        }
                        if accepted && session.authenticated() {
                            return Ok(());
                        }
                        self.print(&format!("{}\n", t!("conn-denied")));
                    }
                }
                // Both usually ask for the same password -- don't ask twice
                // as often.
                AuthMethod::Password if offered("password") && !asked_interactively => {
                    for _ in 0..3 {
                        self.print(&format!("{} ", t!("conn-password-prompt", target = &hop.label)));
                        let password = self.read_line(false)?;
                        if session.userauth_password(&user, &password).is_ok() && session.authenticated() {
                            return Ok(());
                        }
                        self.print(&format!("{}\n", t!("conn-denied")));
                    }
                }
                _ => {}
            }
        }
        let mut message = if hop.settings.auth_methods == AuthMethod::DEFAULT {
            t!("conn-auth-failed", user = &user, host = &hop.host, methods = &methods)
        } else {
            let allowed: Vec<&str> = hop.settings.auth_methods.iter().map(|m| m.name()).collect();
            t!("conn-auth-failed-preferred", user = &user, host = &hop.host, methods = &methods, preferred = allowed.join(","))
        };
        if agent_offered >= 3 && !plan.restrict {
            message.push('\n');
            message.push_str(&t!("conn-max-auth-tries", count = agent_offered));
        }
        Err(Stop::Failed(message))
    }

    /// Agent keys, then the plan's key files, then -- unless restricted --
    /// the default ones. Returns whether that authenticated, and how many
    /// agent keys were tried.
    fn try_public_keys(&mut self, session: &Session, user: &str, plan: &AuthPlan) -> Result<(bool, usize), Stop> {
        let (authenticated, agent_offered) = self.try_agent(session, user, plan);
        if authenticated {
            return Ok((true, agent_offered));
        }
        for key in &plan.files {
            if self.try_key(session, user, key, true)? {
                return Ok((true, agent_offered));
            }
        }
        if !plan.restrict && plan.files.is_empty() {
            for key in default_key_files() {
                if self.try_key(session, user, &key, false)? {
                    return Ok((true, agent_offered));
                }
            }
        }
        Ok((false, agent_offered))
    }

    /// Offer the agent's keys -- only the plan's ones if it's restricted.
    /// Returns whether that authenticated, and how many keys were tried.
    fn try_agent(&mut self, session: &Session, user: &str, plan: &AuthPlan) -> (bool, usize) {
        let socket = match &plan.agent {
            AgentSocket::Off => return (false, 0),
            AgentSocket::Env if std::env::var_os("SSH_AUTH_SOCK").is_none() => return (false, 0),
            AgentSocket::Env => None,
            AgentSocket::Path(path) => Some(path),
        };
        if plan.restrict && plan.agent_keys.is_empty() {
            return (false, 0);
        }
        let Ok(mut agent) = session.agent() else { return (false, 0) };
        if let Some(path) = socket
            && agent.set_identity_path(path).is_err()
        {
            return (false, 0);
        }
        if let Err(err) = agent.connect().and_then(|()| agent.list_identities()) {
            log::debug!("SSH agent unavailable: {err}");
            return (false, 0);
        }
        let identities = agent.identities().unwrap_or_default();
        let offered = identities
            .iter()
            .filter(|identity| !plan.restrict || plan.agent_keys.iter().any(|key| key.as_slice() == identity.blob()));
        let (mut authenticated, mut tried) = (false, 0);
        for identity in offered {
            tried += 1;
            if agent.userauth(user, identity).is_ok() && session.authenticated() {
                authenticated = true;
                break;
            }
        }
        let _ = agent.disconnect();
        (authenticated, tried)
    }

    /// `Ok(true)` once authenticated. Asks for the passphrase if the key
    /// turns out to be encrypted.
    fn try_key(&mut self, session: &Session, user: &str, key: &Path, explicit: bool) -> Result<bool, Stop> {
        if !key.exists() {
            if explicit {
                let message = t!("conn-key-file-missing", path = key.display().to_string());
                self.print(&format!("\x1b[33m{message}\x1b[0m\n"));
            }
            return Ok(false);
        }
        // With the public half next to it, libssh2 can offer the key without
        // decrypting it first -- so we only ask for a passphrase if the
        // server actually accepts this key.
        let mut public = key.as_os_str().to_owned();
        public.push(".pub");
        let public = Some(PathBuf::from(public)).filter(|p| p.exists());

        let encrypted = std::fs::read_to_string(key).is_ok_and(|text| key_is_encrypted(&text));
        let mut passphrase: Option<String> = None;
        for attempt in 0..=3 {
            match session.userauth_pubkey_file(user, public.as_deref(), key, passphrase.as_deref()) {
                Ok(()) => return Ok(session.authenticated()),
                // Only an encrypted key gets a passphrase prompt, and only
                // for these two failures; a key the server simply doesn't
                // accept fails differently and moves on without asking.
                Err(err)
                    if encrypted
                        && attempt < 3
                        && matches!(
                            err.code(),
                            ErrorCode::Session(LIBSSH2_ERROR_FILE | LIBSSH2_ERROR_PUBLICKEY_UNVERIFIED)
                        ) =>
                {
                    if passphrase.is_some() {
                        self.print(&format!("{}\n", t!("conn-wrong-passphrase")));
                    }
                    self.print(&format!("{} ", t!("conn-passphrase-prompt", path = key.display().to_string())));
                    passphrase = Some(self.read_line(false)?);
                }
                Err(err) => {
                    log::debug!("key {} not accepted: {err} ({:?})", key.display(), err.code());
                    return Ok(false);
                }
            }
        }
        Ok(false)
    }

    /// Relay bytes until the remote shell exits, the connection drops or
    /// the tab closes -- for the shell and every forwarded port.
    fn pump(
        &mut self,
        session: &Session,
        channel: &mut Channel,
        socket: RawFd,
        forwards: &mut Forwards,
    ) -> Result<End, Stop> {
        session.set_blocking(false);
        self.wake.set_nonblocking(true)?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut outgoing: Vec<u8> = Vec::new();
        let mut resize: Option<WindowSize> = None;
        let keepalive = self.target.settings.keepalive;
        let mut next_keepalive = keepalive.map(|(interval, _)| Instant::now() + secs(interval));
        // Keepalives sent since the server was last heard from.
        let mut unanswered = 0;

        loop {
            // The wakeup bytes carry no data; what matters is in the queue.
            while matches!((&self.wake).read(&mut buf), Ok(n) if n > 0) {}
            loop {
                match self.rx.try_recv() {
                    Ok(Msg::Input(bytes)) => outgoing.extend(bytes),
                    Ok(Msg::Resize(size)) => resize = Some(size),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(End::TabClosed),
                }
            }

            // Before the reads below: whatever libssh2 reads while sending
            // mustn't sit unnoticed until the next wakeup.
            let now = Instant::now();
            if let (Some(deadline), Some((interval, count_max))) = (next_keepalive, keepalive)
                && now >= deadline
            {
                if count_max > 0 && unanswered >= count_max {
                    return Err(Stop::Failed(t!("conn-keepalive-dead", target = &self.target.label, count = count_max)));
                }
                let to_next = session.keepalive_send().unwrap_or(1).max(1);
                // libssh2 sends only when one is due, and then says the full
                // interval until the next.
                if to_next >= interval {
                    unanswered += 1;
                }
                next_keepalive = Some(now + secs(to_next));
            }

            if let Some(size) = resize {
                self.size = size;
                let (cols, lines) = (u32::from(size.num_cols), u32::from(size.num_lines));
                let pixels = (cols * u32::from(size.cell_width), lines * u32::from(size.cell_height));
                match channel.request_pty_size(cols, lines, Some(pixels.0), Some(pixels.1)) {
                    Err(err) if matches!(err.code(), ErrorCode::Session(LIBSSH2_ERROR_EAGAIN)) => {}
                    // Done -- or refused for good; either way, don't retry.
                    _ => resize = None,
                }
            }

            while !outgoing.is_empty() {
                match channel.write(&outgoing) {
                    Ok(n) => drop(outgoing.drain(..n)),
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) => return Err(Stop::Failed(t!("conn-lost", err = err.to_string()))),
                }
            }

            let mut inbound = forwards.service(session);
            let mut processed = 0;
            while processed < MAX_READ_PER_LOOP {
                match channel.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        log::trace!("{}: read {n} bytes", self.target.label);
                        self.parser.advance(&mut *self.term.lock(), &buf[..n]);
                        processed += n;
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) => return Err(Stop::Failed(t!("conn-lost", err = err.to_string()))),
                }
            }
            // Last before sleeping; see `Forwards::accept_remote`.
            forwards.accept_remote();
            inbound += processed;
            if inbound > 0 {
                unanswered = 0;
            }
            // Same rule as alacritty: no redraw while all of it was inside a
            // synchronized update.
            if processed > 0 && self.parser.sync_bytes_count() < processed {
                self.listener.send_event(Event::Wakeup);
            }

            if channel.eof() {
                return Ok(End::Exited);
            }

            let now = Instant::now();
            let sync_deadline = self.parser.sync_timeout().sync_timeout();
            if sync_deadline.is_some_and(|deadline| now >= deadline) {
                self.parser.stop_sync(&mut *self.term.lock());
                self.listener.send_event(Event::Wakeup);
            }

            // Sleep until the server, the app, a forwarded socket or a timer
            // needs us. POLLOUT only when libssh2 is waiting to send -- a full
            // remote window is cleared by inbound data, not by the socket
            // being writable.
            let mut events = libc::POLLIN;
            if matches!(session.block_directions(), BlockDirections::Outbound | BlockDirections::Both) {
                events |= libc::POLLOUT;
            }
            let mut fds = vec![(socket, events), (self.wake.as_raw_fd(), libc::POLLIN)];
            forwards.poll_fds(&mut fds);
            // Data libssh2 already took off the socket for a channel (while
            // reading or writing another) doesn't wake poll().
            let waiting = processed >= MAX_READ_PER_LOOP || channel.read_window().available > 0 || forwards.buffered();
            let timeout = if waiting {
                Duration::ZERO
            } else {
                [sync_deadline, next_keepalive]
                    .into_iter()
                    .flatten()
                    .min()
                    .map_or(IDLE_POLL, |deadline| deadline.saturating_duration_since(now))
            };
            log::trace!("{}: poll for {timeout:?} (events {events:#x})", self.target.label);
            let revents = poll(&fds, timeout);
            if revents[0] & libc::POLLIN != 0 {
                unanswered = 0;
            }
            log::trace!("{}: poll woke", self.target.label);
        }
    }

    /// Write text into the tab as if the server had sent it.
    fn print(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        self.parser.advance(&mut *self.term.lock(), text.as_bytes());
        self.listener.send_event(Event::Wakeup);
    }

    /// One line typed into the tab; secrets (`echo == false`) stay hidden.
    fn read_line(&mut self, echo: bool) -> Result<String, Stop> {
        let mut line = String::new();
        loop {
            let bytes = match self.rx.recv() {
                Err(_) => return Err(Stop::TabClosed),
                Ok(Msg::Resize(size)) => {
                    self.size = size;
                    continue;
                }
                Ok(Msg::Input(bytes)) => bytes,
            };
            // Arrow keys and friends arrive as one escape sequence each.
            if bytes.first() == Some(&0x1b) {
                continue;
            }
            for c in String::from_utf8_lossy(&bytes).chars() {
                match c {
                    '\r' | '\n' => {
                        self.print("\n");
                        return Ok(line);
                    }
                    '\x03' | '\x04' => {
                        self.print("^C\n");
                        return Err(Stop::Cancelled);
                    }
                    '\x7f' | '\x08' => {
                        if line.pop().is_some() && echo {
                            self.print("\x08 \x08");
                        }
                    }
                    c if c.is_control() => {}
                    c => {
                        line.push(c);
                        if echo {
                            self.print(c.encode_utf8(&mut [0; 4]));
                        }
                    }
                }
            }
        }
    }

    /// `false` if the tab was closed instead.
    fn wait_for_key(&mut self) -> bool {
        loop {
            match self.rx.recv() {
                Ok(Msg::Input(_)) => return true,
                Ok(Msg::Resize(_)) => {}
                Err(_) => return false,
            }
        }
    }
}

/// keyboard-interactive auth (e.g. password + one-time code) prompts
/// in the tab, same as the password prompt.
struct Prompter<'w> {
    worker: &'w mut Worker,
    /// Set if the user cancelled or closed the tab mid-prompt.
    stop: Option<Stop>,
}

impl KeyboardInteractivePrompt for Prompter<'_> {
    fn prompt<'a>(&mut self, _username: &str, instructions: &str, prompts: &[Prompt<'a>]) -> Vec<String> {
        if !instructions.trim().is_empty() {
            self.worker.print(&format!("{}\n", instructions.trim()));
        }
        prompts
            .iter()
            .map(|prompt| {
                if self.stop.is_some() {
                    return String::new();
                }
                self.worker.print(&prompt.text);
                self.worker.read_line(prompt.echo).unwrap_or_else(|stop| {
                    self.stop = Some(stop);
                    String::new()
                })
            })
            .collect()
    }
}

/// Run one ProxyJump hop: move bytes between the jump host's
/// `direct-tcpip` channel and `local`, whose peer is the next session's
/// transport. Ends when either side closes; owning `session` keeps the
/// jump connection alive until then.
fn spawn_forwarder(
    session: Session,
    channel: Channel,
    socket: RawFd,
    local: UnixStream,
    keepalive: Option<u32>,
) -> io::Result<()> {
    std::thread::Builder::new().name("ssh jump".into()).spawn(move || {
        if let Err(err) = forward(&session, channel, socket, local, keepalive) {
            log::debug!("jump forwarding ended: {err}");
        }
        let _ = session.disconnect(None, &t!("conn-tunnel-closed"), None);
    })?;
    Ok(())
}

fn forward(
    session: &Session,
    mut channel: Channel,
    socket: RawFd,
    mut local: UnixStream,
    keepalive: Option<u32>,
) -> io::Result<()> {
    // Stderr on a tunnel is unheard of, but it would count as waiting data
    // below and never be read.
    channel.handle_extended_data(ExtendedData::Merge).map_err(io::Error::other)?;
    session.set_blocking(false);
    if let Some(interval) = keepalive {
        session.set_keepalive(true, interval);
    }
    local.set_nonblocking(true)?;
    let mut buf = vec![0u8; 32 * 1024];
    let (mut to_local, mut to_remote) = (Vec::<u8>::new(), Vec::<u8>::new());
    let mut local_closed = false;
    loop {
        // Remote → local. Read until libssh2 has nothing buffered, or the
        // local side has fallen too far behind.
        while to_local.len() < MAX_FORWARD_BUFFER {
            match channel.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => to_local.extend_from_slice(&buf[..n]),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        while !to_local.is_empty() {
            match local.write(&to_local) {
                Ok(n) => drop(to_local.drain(..n)),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }

        // Local → remote.
        while !local_closed && to_remote.len() < MAX_FORWARD_BUFFER {
            match local.read(&mut buf) {
                Ok(0) => local_closed = true,
                Ok(n) => to_remote.extend_from_slice(&buf[..n]),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }
        while !to_remote.is_empty() {
            match channel.write(&to_remote) {
                Ok(n) => drop(to_remote.drain(..n)),
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(err),
            }
        }

        if local_closed && to_remote.is_empty() {
            let _ = channel.send_eof();
            return Ok(());
        }
        if channel.eof() && to_local.is_empty() {
            return Ok(());
        }

        // Only wait for what can make progress, or a full buffer would turn
        // this into a busy loop.
        let mut remote_events = 0;
        if to_local.len() < MAX_FORWARD_BUFFER {
            remote_events |= libc::POLLIN;
        }
        if matches!(session.block_directions(), BlockDirections::Outbound | BlockDirections::Both) {
            remote_events |= libc::POLLOUT;
        }
        let mut local_events = 0;
        if !local_closed && to_remote.len() < MAX_FORWARD_BUFFER {
            local_events |= libc::POLLIN;
        }
        if !to_local.is_empty() {
            local_events |= libc::POLLOUT;
        }
        // Writing to the channel may have read data for it off the socket.
        let timeout = if to_local.len() < MAX_FORWARD_BUFFER && channel.read_window().available > 0 {
            Duration::ZERO
        } else {
            keepalive.map_or(IDLE_POLL, secs)
        };
        let revents = poll(&[(socket, remote_events), (local.as_raw_fd(), local_events)], timeout);
        if keepalive.is_some() && revents.iter().all(|&r| r == 0) {
            let _ = session.keepalive_send();
        }
    }
}

/// `KexAlgorithms`, `HostKeyAlgorithms`, `Ciphers`, `MACs` -- set before
/// the handshake, against what this libssh2 supports.
fn set_algorithms(session: &Session, hop: &SshTarget) -> Result<(), Stop> {
    for (kind, spec) in &hop.settings.algorithms {
        let methods: &[MethodType] = match kind {
            AlgorithmKind::Kex => &[MethodType::Kex],
            AlgorithmKind::HostKey => &[MethodType::HostKey],
            AlgorithmKind::Cipher => &[MethodType::CryptCs, MethodType::CryptSc],
            AlgorithmKind::Mac => &[MethodType::MacCs, MethodType::MacSc],
        };
        for method in methods {
            let supported = session.supported_algs(*method)?;
            let list = algorithm_list(spec, &supported)
                .map_err(|err| Stop::Failed(t!("conn-algorithms-failed", keyword = kind.keyword(), hop = &hop.label, err = err)))?;
            session.method_pref(*method, &list)?;
        }
    }
    Ok(())
}

/// Append one line rather than letting libssh2 rewrite the file, which
/// would drop comments and every line it didn't understand.
fn append_known_host(path: &Path, entry: &str, key_type: HostKeyType, key: &[u8]) -> io::Result<()> {
    let name = key_type_name(key_type).ok_or_else(|| io::Error::other(t!("conn-unknown-key-type")))?;
    if let Some(dir) = path.parent()
        && !dir.exists()
    {
        std::fs::create_dir_all(dir)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let missing_newline = std::fs::read(path).is_ok_and(|data| !data.is_empty() && !data.ends_with(b"\n"));
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}{entry} {name} {}", if missing_newline { "\n" } else { "" }, base64(key))
}

fn key_type_name(key_type: HostKeyType) -> Option<&'static str> {
    Some(match key_type {
        HostKeyType::Rsa => "ssh-rsa",
        HostKeyType::Dss => "ssh-dss",
        HostKeyType::Ecdsa256 => "ecdsa-sha2-nistp256",
        HostKeyType::Ecdsa384 => "ecdsa-sha2-nistp384",
        HostKeyType::Ecdsa521 => "ecdsa-sha2-nistp521",
        HostKeyType::Ed25519 => "ssh-ed25519",
        HostKeyType::Unknown => return None,
    })
}

/// Whether a private key file needs a passphrase. Legacy PEM keys say so
/// in a header; OpenSSH-format keys name their cipher ("none" if plain)
/// right after the magic at the start of the base64 body.
pub(super) fn key_is_encrypted(text: &str) -> bool {
    // "Proc-Type: 4,ENCRYPTED" or "BEGIN ENCRYPTED PRIVATE KEY".
    if text.contains("ENCRYPTED") {
        return true;
    }
    let Some(body) =
        text.split("-----BEGIN OPENSSH PRIVATE KEY-----").nth(1).and_then(|rest| rest.split("-----END").next())
    else {
        return false;
    };
    let Some(bytes) = base64_decode(body) else { return false };
    let Some(rest) = bytes.strip_prefix(b"openssh-key-v1\0") else { return false };
    let Some((len, rest)) = rest.split_first_chunk::<4>() else { return false };
    rest.get(..u32::from_be_bytes(*len) as usize).is_some_and(|cipher| cipher != b"none")
}

/// Inverse of [`base64`]; skips whitespace, `None` on other invalid input.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        acc = acc << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Standard base64 with padding -- all known_hosts and fingerprints need.
pub(super) fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Block until one of `fds` is ready or `timeout` passes. Returns each
/// fd's `revents`, all 0 on timeout -- or on an error (EINTR); the caller
/// just loops.
fn poll(fds: &[(RawFd, libc::c_short)], timeout: Duration) -> Vec<libc::c_short> {
    let mut pollfds: Vec<libc::pollfd> =
        fds.iter().map(|&(fd, events)| libc::pollfd { fd, events, revents: 0 }).collect();
    // Round up, so a sub-millisecond deadline doesn't turn into a busy loop.
    let millis = timeout.as_micros().div_ceil(1000).min(libc::c_int::MAX as u128) as libc::c_int;
    // SAFETY: `pollfds` is a valid, exclusively borrowed array of
    // `pollfds.len()` entries for the duration of the call.
    let ready = unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, millis) };
    if ready > 0 { pollfds.iter().map(|p| p.revents).collect() } else { vec![0; fds.len()] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_known_host_lines_without_touching_existing_ones() {
        let dir = std::env::temp_dir().join(format!("terminaal-known-hosts-{}", std::process::id()));
        let path = dir.join(".ssh/known_hosts");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // An existing entry libssh2 wouldn't understand, without a final newline.
        std::fs::write(&path, "@cert-authority *.example ssh-ed25519 AAAA").unwrap();

        append_known_host(&path, "[127.0.0.1]:2222", HostKeyType::Ed25519, b"Man").unwrap();
        append_known_host(&path, "example.com", HostKeyType::Rsa, b"Ma").unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            text,
            "@cert-authority *.example ssh-ed25519 AAAA\n\
             [127.0.0.1]:2222 ssh-ed25519 TWFu\n\
             example.com ssh-rsa TWE=\n"
        );
    }

    #[test]
    fn base64_decode_inverts_encode() {
        for data in [&b""[..], b"M", b"Ma", b"Man", &[0xff, 0xfe, 0xfd, 0x00]] {
            assert_eq!(base64_decode(&base64(data)).unwrap(), data);
        }
        assert_eq!(base64_decode("TW\nFu").unwrap(), b"Man");
        assert!(base64_decode("TW*u").is_none());
    }

    #[test]
    fn detects_encrypted_keys() {
        let openssh = |cipher: &str| {
            let mut body = b"openssh-key-v1\0".to_vec();
            body.extend((cipher.len() as u32).to_be_bytes());
            body.extend(cipher.as_bytes());
            body.extend(b"\0\0\0\x04rest");
            format!("-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----\n", base64(&body))
        };
        assert!(key_is_encrypted(&openssh("aes256-ctr")));
        assert!(!key_is_encrypted(&openssh("none")));
        assert!(key_is_encrypted("-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: x\n"));
        assert!(!key_is_encrypted("-----BEGIN RSA PRIVATE KEY-----\nMIIEow==\n-----END RSA PRIVATE KEY-----\n"));
        assert!(!key_is_encrypted("not a key"));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd, 0x00]), "//79AA==");
    }

    #[test]
    fn poll_reports_which_fd_is_ready() {
        // The forwarders need a live libssh2 channel; what can be checked
        // here is the poll helper they and the pump rely on.
        let (a, mut b) = UnixStream::pair().unwrap();
        let (c, _d) = UnixStream::pair().unwrap();
        let fds = [(a.as_raw_fd(), libc::POLLIN), (c.as_raw_fd(), libc::POLLIN)];
        assert_eq!(poll(&fds, Duration::from_millis(10)), [0, 0]);
        b.write_all(b"x").unwrap();
        let revents = poll(&fds, Duration::from_millis(10));
        assert!(revents[0] & libc::POLLIN != 0 && revents[1] == 0);
    }
}
