//! Per-host SSH options beyond address, logins and jump hosts -- the
//! `ssh_config` keywords Terminaal understands besides the ones in
//! [`Host`](super::Host) itself. [`Options`] is how they're stored
//! (flattened into a host's table in hosts.toml, or read from
//! `~/.ssh/config`); [`Settings`] is their checked, ready-to-connect form
//! inside an [`SshTarget`](super::SshTarget).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{expand_tilde, glob};
use crate::i18n::t;

/// `ConnectTimeout` if unset, in seconds.
pub const DEFAULT_CONNECT_TIMEOUT: u32 = 10;
/// `ServerAliveInterval` if unset, in seconds. OpenSSH's default is off,
/// but a tab left open for hours is exactly what NAT and firewall
/// timeouts cut, so Terminaal keeps the connection alive by default.
pub const DEFAULT_ALIVE_INTERVAL: u32 = 30;
/// `ServerAliveCountMax` if unset (as in OpenSSH).
pub const DEFAULT_ALIVE_COUNT_MAX: u32 = 3;

/// Unset fields mean "the default"; see each keyword's OpenSSH meaning.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// `ConnectTimeout`, in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_timeout: Option<u32>,
    /// `ServerAliveInterval`, in seconds; 0 turns keepalives off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_alive_interval: Option<u32>,
    /// `ServerAliveCountMax`: unanswered keepalives before the connection
    /// counts as dead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_alive_count_max: Option<u32>,
    /// `Compression`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub compression: bool,
    /// `AddressFamily`.
    #[serde(skip_serializing_if = "AddressFamily::is_any")]
    pub address_family: AddressFamily,
    /// `ProxyCommand`, run with `sh -c`; its stdin/stdout become the
    /// connection. `%h`, `%p`, `%r`, `%n` are expanded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_command: Option<String>,
    /// `PreferredAuthentications`: comma-separated, in order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_authentications: Option<String>,
    /// `ForwardAgent`: `yes`, a socket path, or `$VAR` naming the variable
    /// that holds one; unset or `no`: off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_agent: Option<String>,
    /// `StrictHostKeyChecking`.
    #[serde(skip_serializing_if = "HostKeyCheck::is_ask")]
    pub strict_host_key_checking: HostKeyCheck,
    /// `UserKnownHostsFile` (`~/` allowed); unset: `~/.ssh/known_hosts`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_known_hosts_file: Option<String>,
    /// `RemoteCommand`: run instead of the login shell, still in a PTY.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_command: Option<String>,
    /// `SetEnv`: `NAME=value` each. `TERM` sets the PTY's terminal type.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub set_env: Vec<String>,
    /// `SendEnv`: names of local variables to pass on; `*`/`?` allowed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub send_env: Vec<String>,
    /// `LocalForward`: `[bind:]port host:hostport` each; either side may
    /// be a Unix socket path instead.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub local_forward: Vec<String>,
    /// `RemoteForward`: same syntax, the other way round (the server side
    /// can't be a socket). Just `[bind:]port`: a SOCKS proxy on the server.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remote_forward: Vec<String>,
    /// `DynamicForward`: `[bind:]port` each, a SOCKS proxy here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dynamic_forward: Vec<String>,
    /// `KexAlgorithms`, `HostKeyAlgorithms`, `Ciphers`, `MACs`: lists in
    /// ssh's syntax, including the `+`/`-`/`^` prefixes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kex_algorithms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_key_algorithms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ciphers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macs: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddressFamily {
    #[default]
    Any,
    /// IPv4 only.
    Inet,
    /// IPv6 only.
    Inet6,
}

impl AddressFamily {
    pub const ALL: [AddressFamily; 3] = [AddressFamily::Any, AddressFamily::Inet, AddressFamily::Inet6];

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "any" => Some(Self::Any),
            "inet" => Some(Self::Inet),
            "inet6" => Some(Self::Inet6),
            _ => None,
        }
    }

    pub fn allows(self, addr: &SocketAddr) -> bool {
        match self {
            Self::Any => true,
            Self::Inet => addr.is_ipv4(),
            Self::Inet6 => addr.is_ipv6(),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Any => t!("opt-family-any"),
            Self::Inet => t!("opt-family-inet"),
            Self::Inet6 => t!("opt-family-inet6"),
        }
    }

    fn is_any(&self) -> bool {
        *self == Self::Any
    }
}

/// What to do about a host key that isn't in known_hosts yet. A *changed*
/// key always ends the connection -- unlike OpenSSH's `no`, which lets it
/// through with restrictions; `no`/`off` read as `accept-new` here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostKeyCheck {
    /// Ask in the tab, then remember it.
    #[default]
    Ask,
    /// Remember it without asking.
    AcceptNew,
    /// Refuse to connect.
    Yes,
}

impl HostKeyCheck {
    pub const ALL: [HostKeyCheck; 3] = [HostKeyCheck::Ask, HostKeyCheck::AcceptNew, HostKeyCheck::Yes];

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "ask" => Some(Self::Ask),
            "accept-new" | "no" | "off" => Some(Self::AcceptNew),
            "yes" => Some(Self::Yes),
            _ => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Ask => t!("opt-check-ask"),
            Self::AcceptNew => t!("opt-check-accept-new"),
            Self::Yes => t!("opt-check-yes"),
        }
    }

    fn is_ask(&self) -> bool {
        *self == Self::Ask
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMethod {
    PublicKey,
    KeyboardInteractive,
    Password,
}

impl AuthMethod {
    pub const DEFAULT: [AuthMethod; 3] = [AuthMethod::PublicKey, AuthMethod::KeyboardInteractive, AuthMethod::Password];

    /// The name servers use (and `PreferredAuthentications` lists).
    pub fn name(self) -> &'static str {
        match self {
            Self::PublicKey => "publickey",
            Self::KeyboardInteractive => "keyboard-interactive",
            Self::Password => "password",
        }
    }
}

/// `PreferredAuthentications`. Methods libssh2 can't do (`gssapi-with-mic`,
/// `hostbased`) are skipped, as long as something is left.
fn parse_auth_methods(spec: &str) -> Result<Vec<AuthMethod>, String> {
    let mut methods = Vec::new();
    for name in spec.split(',').map(str::trim).filter(|name| !name.is_empty()) {
        let method = match name.to_lowercase().as_str() {
            "publickey" => AuthMethod::PublicKey,
            "keyboard-interactive" => AuthMethod::KeyboardInteractive,
            "password" => AuthMethod::Password,
            "gssapi-with-mic" | "hostbased" => continue,
            _ => return Err(t!("opt-unknown-method", name = name)),
        };
        if !methods.contains(&method) {
            methods.push(method);
        }
    }
    if methods.is_empty() {
        return Err(t!("opt-no-method"));
    }
    Ok(methods)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlgorithmKind {
    Kex,
    HostKey,
    Cipher,
    Mac,
}

impl AlgorithmKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Kex => "KexAlgorithms",
            Self::HostKey => "HostKeyAlgorithms",
            Self::Cipher => "Ciphers",
            Self::Mac => "MACs",
        }
    }
}

/// Turn an algorithm setting into the plain list libssh2 takes, given
/// what it supports (in its default order): a plain list is taken as is,
/// `+list` appends to the defaults, `-list` removes from them, `^list`
/// puts in front. Entries may use `*`/`?`. Unsupported names are dropped
/// -- OpenSSH knows more algorithms than libssh2, and a config written
/// for it shouldn't make the host unreachable -- unless nothing is left.
pub fn algorithm_list(spec: &str, supported: &[&str]) -> Result<String, String> {
    let spec = spec.trim();
    let (prefix, list) = match spec.chars().next() {
        Some(c @ ('+' | '-' | '^')) => (Some(c), &spec[1..]),
        _ => (None, spec),
    };
    let patterns: Vec<&str> = list.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
    let matching = |algorithm: &str| patterns.iter().any(|pattern| glob(pattern, algorithm));
    let mut named: Vec<&str> = Vec::new();
    for pattern in &patterns {
        for algorithm in supported.iter().filter(|a| glob(pattern, a)) {
            if !named.contains(algorithm) {
                named.push(algorithm);
            }
        }
    }
    let result: Vec<&str> = match prefix {
        None => named,
        // libssh2 enables everything it supports by default, so appending
        // changes nothing but can't hurt either.
        Some('+') => supported.to_vec(),
        Some('-') => supported.iter().copied().filter(|a| !matching(a)).collect(),
        _ => named.iter().copied().chain(supported.iter().copied().filter(|a| !matching(a))).collect(),
    };
    if result.is_empty() {
        return Err(t!("opt-no-algorithm", spec = spec, supported = supported.join(",")));
    }
    Ok(result.join(","))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}

impl ForwardKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Local => "LocalForward",
            Self::Remote => "RemoteForward",
            Self::Dynamic => "DynamicForward",
        }
    }
}

/// Where a forward listens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listen {
    /// Unset address: loopback only (locally), `localhost` (remotely);
    /// `*` or empty: all interfaces. Port 0 lets the server choose
    /// (remote only).
    Port { bind: Option<String>, port: u16 },
    /// A Unix socket; local only -- libssh2 can't ask the server to
    /// listen on one (`streamlocal-forward@openssh.com`).
    Unix(String),
}

/// Where a forward's connections go: seen from the server for a local
/// forward, from here for a remote one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Tcp { host: String, port: u16 },
    Unix(String),
}

/// A `LocalForward`, `RemoteForward` or `DynamicForward`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forward {
    /// `RemoteForward`: the server listens, connections come back here.
    pub remote: bool,
    pub listen: Listen,
    /// `None`: dynamic -- each connection names its target via SOCKS.
    pub target: Option<Target>,
}

impl Forward {
    /// `listen target` -- or both joined by `:`, as with `ssh -L`. Each
    /// side is `[bind:]port`/`host:port` or a Unix socket path (anything
    /// with a `/`); IPv6 addresses go in brackets. Dynamic forwards, and
    /// remote ones meant as a SOCKS proxy, have no target.
    pub fn parse(spec: &str, kind: ForwardKind) -> Result<Self, String> {
        let invalid = |reason: String| t!("opt-forward-invalid", keyword = kind.keyword(), spec = spec, reason = reason);
        let syntax = || {
            invalid(if kind == ForwardKind::Dynamic { t!("opt-forward-dynamic-syntax") } else { t!("opt-forward-syntax") })
        };
        let spec = spec.trim();
        let remote = kind == ForwardKind::Remote;
        let (listen, target) = match spec.split_once(char::is_whitespace) {
            Some((listen, target)) => (split_endpoint(listen), split_endpoint(target.trim())),
            None => {
                let mut parts = split_colons(spec);
                // The target is at the end: a socket path is one part,
                // `host:port` two; without one, it's all listen address.
                let target_len = match (kind, parts.as_slice()) {
                    (ForwardKind::Dynamic, _) | (_, [_]) => 0,
                    (_, [.., last]) if is_path(last) => 1,
                    (ForwardKind::Remote, [_, _]) => 0,
                    _ => 2,
                };
                let target = parts.split_off(parts.len() - target_len);
                (parts, target)
            }
        };

        let port = |text: &str| text.parse::<u16>().map_err(|_| invalid(t!("opt-forward-port")));
        let listen = match listen.as_slice() {
            [path] if is_path(path) && remote => return Err(invalid(t!("opt-forward-remote-unix"))),
            [path] if is_path(path) => Listen::Unix(path.clone()),
            [text] => Listen::Port { bind: None, port: port(text)? },
            [bind, text] => Listen::Port { bind: Some(bind.clone()), port: port(text)? },
            _ => return Err(syntax()),
        };
        if matches!(listen, Listen::Port { port: 0, .. }) && !remote {
            return Err(invalid(t!("opt-forward-port-zero")));
        }

        let target = match target.as_slice() {
            [] if kind == ForwardKind::Local => return Err(invalid(t!("opt-forward-local-no-target"))),
            [] => None,
            _ if kind == ForwardKind::Dynamic => return Err(syntax()),
            [path] if is_path(path) => Some(Target::Unix(path.clone())),
            [host, text] => {
                let port = text.parse::<u16>().ok().filter(|&p| p > 0).ok_or_else(|| invalid(t!("opt-forward-target-port")))?;
                if host.is_empty() {
                    return Err(invalid(t!("opt-forward-target-missing")));
                }
                Some(Target::Tcp { host: host.clone(), port })
            }
            _ => return Err(syntax()),
        };
        Ok(Self { remote, listen, target })
    }

    /// Which keyword this is written with.
    pub fn kind(&self) -> ForwardKind {
        match (self.remote, &self.target) {
            (true, _) => ForwardKind::Remote,
            (false, None) => ForwardKind::Dynamic,
            (false, Some(_)) => ForwardKind::Local,
        }
    }

    /// Where it listens, e.g. `localhost:8080`, `Server:8080` or a path.
    pub fn listen_label(&self) -> String {
        let (bind, port) = match &self.listen {
            Listen::Unix(path) => return path.clone(),
            Listen::Port { bind, port } => (bind.as_deref(), port),
        };
        let bind = match bind {
            None => if self.remote { "Server" } else { "localhost" }.to_string(),
            Some("" | "*") => if self.remote { "Server:*" } else { "*" }.to_string(),
            Some(bind) if self.remote => format!("Server:{bind}"),
            Some(bind) => bind.to_string(),
        };
        format!("{bind}:{port}")
    }

    /// Where connections end up, e.g. `db.internal:5432`, a socket path
    /// or `SOCKS`.
    pub fn target_label(&self) -> String {
        match &self.target {
            None => "SOCKS".to_string(),
            Some(target) => target.spec(),
        }
    }

    /// The listen side as written in a spec: `[bind:]port` or the path.
    pub fn listen_spec(&self) -> String {
        match &self.listen {
            Listen::Unix(path) => path.clone(),
            Listen::Port { bind: Some(bind), port } if bind.contains(':') => format!("[{bind}]:{port}"),
            Listen::Port { bind: Some(bind), port } => format!("{bind}:{port}"),
            Listen::Port { bind: None, port } => port.to_string(),
        }
    }
}

impl Target {
    /// As written in a spec: `host:port` or the path.
    pub fn spec(&self) -> String {
        match self {
            Target::Unix(path) => path.clone(),
            Target::Tcp { host, port } if host.contains(':') => format!("[{host}]:{port}"),
            Target::Tcp { host, port } => format!("{host}:{port}"),
        }
    }
}

/// Socket paths are told apart from addresses by their `/`, as by ssh.
fn is_path(text: &str) -> bool {
    text.contains('/')
}

/// One side of a forward: a path stays whole (it may contain `:`).
fn split_endpoint(text: &str) -> Vec<String> {
    if is_path(text) { vec![text.to_string()] } else { split_colons(text) }
}

/// Split on `:` outside of `[…]`, dropping the brackets.
fn split_colons(text: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut bracketed = false;
    for c in text.chars() {
        match c {
            '[' => bracketed = true,
            ']' => bracketed = false,
            ':' if !bracketed => parts.push(String::new()),
            c => parts.last_mut().expect("never empty").push(c),
        }
    }
    parts
}

/// Checked options, ready for a connection.
#[derive(Clone, Debug)]
pub struct Settings {
    pub connect_timeout: Duration,
    /// Keepalive interval in seconds and how many may go unanswered;
    /// `None`: no keepalives.
    pub keepalive: Option<(u32, u32)>,
    pub compression: bool,
    pub address_family: AddressFamily,
    /// Expanded, ready for `sh -c`.
    pub proxy_command: Option<String>,
    pub auth_methods: Vec<AuthMethod>,
    pub host_key_check: HostKeyCheck,
    /// Unset: `~/.ssh/known_hosts`.
    pub known_hosts: Option<PathBuf>,
    pub remote_command: Option<String>,
    /// PTY terminal type (`SetEnv TERM=…`).
    pub term: String,
    /// Variables to set in the remote session, in order.
    pub env: Vec<(String, String)>,
    pub forwards: Vec<Forward>,
    pub forward_agent: ForwardAgent,
    pub algorithms: Vec<(AlgorithmKind, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(DEFAULT_CONNECT_TIMEOUT.into()),
            keepalive: Some((DEFAULT_ALIVE_INTERVAL, DEFAULT_ALIVE_COUNT_MAX)),
            compression: false,
            address_family: AddressFamily::Any,
            proxy_command: None,
            auth_methods: AuthMethod::DEFAULT.to_vec(),
            host_key_check: HostKeyCheck::Ask,
            known_hosts: None,
            remote_command: None,
            term: "xterm-256color".into(),
            env: Vec::new(),
            forwards: Vec::new(),
            forward_agent: ForwardAgent::Off,
            algorithms: Vec::new(),
        }
    }
}

impl Settings {
    /// What applies to a host used as a jump host: no forwards, no agent,
    /// no command, no environment -- as with OpenSSH, whose jump
    /// connections only tunnel.
    pub fn for_jump(mut self) -> Self {
        self.remote_command = None;
        self.env.clear();
        self.forwards.clear();
        self.forward_agent = ForwardAgent::Off;
        self
    }
}

/// `ForwardAgent`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ForwardAgent {
    #[default]
    Off,
    /// `yes`: the agent used to log in (`IdentityAgent`, else
    /// `SSH_AUTH_SOCK`).
    Login,
    /// A socket path, or `$VAR` naming the variable that holds one.
    Socket(String),
}

impl ForwardAgent {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => Self::Off,
            Some(v) if v.eq_ignore_ascii_case("no") => Self::Off,
            Some(v) if v.eq_ignore_ascii_case("yes") => Self::Login,
            Some(socket) => Self::Socket(socket.to_string()),
        }
    }
}

impl Options {
    /// Check and resolve. `expand` does the `%` tokens of `ProxyCommand`
    /// and `RemoteCommand`.
    pub fn settings(&self, expand: &dyn Fn(&str) -> String) -> Result<Settings, String> {
        let defaults = Settings::default();
        let interval = self.server_alive_interval.unwrap_or(DEFAULT_ALIVE_INTERVAL);
        let count_max = self.server_alive_count_max.unwrap_or(DEFAULT_ALIVE_COUNT_MAX);

        // SendEnv first, so SetEnv wins for a variable named in both.
        let mut env: Vec<(String, String)> = Vec::new();
        if !self.send_env.is_empty() {
            let mut local: Vec<(String, String)> = std::env::vars().collect();
            local.sort();
            env.extend(local.into_iter().filter(|(name, _)| name != "TERM" && self.send_env.iter().any(|p| glob(p, name))));
        }
        let mut term = defaults.term.clone();
        for entry in &self.set_env {
            let (name, value) = parse_env(entry)?;
            if name == "TERM" {
                term = value;
            } else {
                env.retain(|(existing, _)| *existing != name);
                env.push((name, value));
            }
        }

        let mut forwards = Vec::new();
        for (kind, specs) in [
            (ForwardKind::Local, &self.local_forward),
            (ForwardKind::Remote, &self.remote_forward),
            (ForwardKind::Dynamic, &self.dynamic_forward),
        ] {
            for spec in specs {
                forwards.push(Forward::parse(spec, kind)?);
            }
        }

        let mut algorithms = Vec::new();
        for (kind, spec) in [
            (AlgorithmKind::Kex, &self.kex_algorithms),
            (AlgorithmKind::HostKey, &self.host_key_algorithms),
            (AlgorithmKind::Cipher, &self.ciphers),
            (AlgorithmKind::Mac, &self.macs),
        ] {
            if let Some(spec) = non_empty(spec) {
                if spec.trim_start_matches(['+', '-', '^']).trim().is_empty() {
                    return Err(t!("opt-list-empty", keyword = kind.keyword()));
                }
                algorithms.push((kind, spec.to_string()));
            }
        }

        Ok(Settings {
            connect_timeout: match self.connect_timeout {
                Some(0) | None => defaults.connect_timeout,
                Some(secs) => Duration::from_secs(secs.into()),
            },
            keepalive: (interval > 0).then_some((interval, count_max)),
            compression: self.compression,
            address_family: self.address_family,
            proxy_command: non_empty(&self.proxy_command).filter(|c| !c.eq_ignore_ascii_case("none")).map(expand),
            auth_methods: match non_empty(&self.preferred_authentications) {
                Some(spec) => parse_auth_methods(spec)?,
                None => defaults.auth_methods,
            },
            host_key_check: self.strict_host_key_checking,
            known_hosts: non_empty(&self.user_known_hosts_file).map(expand_tilde),
            remote_command: non_empty(&self.remote_command).map(expand),
            term,
            env,
            forwards,
            forward_agent: ForwardAgent::parse(self.forward_agent.as_deref()),
            algorithms,
        })
    }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// `NAME=value`; the name as the shell allows it.
fn parse_env(entry: &str) -> Result<(String, String), String> {
    let (name, value) = entry.split_once('=').ok_or_else(|| t!("opt-setenv-syntax", entry = entry))?;
    let name = name.trim();
    let valid = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(t!("opt-setenv-name", entry = entry, name = name));
    }
    Ok((name.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_expand(value: &str) -> String {
        value.to_string()
    }

    fn port(bind: Option<&str>, port: u16) -> Listen {
        Listen::Port { bind: bind.map(str::to_string), port }
    }

    fn tcp(host: &str, port: u16) -> Target {
        Target::Tcp { host: host.into(), port }
    }

    #[test]
    fn parses_forwards_like_ssh() {
        use ForwardKind::{Local, Remote};
        let f = Forward::parse("8080 db.internal:5432", Local).unwrap();
        assert_eq!((&f.listen, &f.target), (&port(None, 8080), &Some(tcp("db.internal", 5432))));
        assert_eq!((f.listen_label().as_str(), f.target_label().as_str()), ("localhost:8080", "db.internal:5432"));

        let f = Forward::parse("[::1]:2222:[2001:db8::5]:22", Local).unwrap();
        assert_eq!((&f.listen, &f.target), (&port(Some("::1"), 2222), &Some(tcp("2001:db8::5", 22))));
        assert_eq!((f.listen_spec().as_str(), f.target_label().as_str()), ("[::1]:2222", "[2001:db8::5]:22"));

        let f = Forward::parse("*:0   localhost:3000", Remote).unwrap();
        assert_eq!(f.listen, port(Some("*"), 0));
        assert_eq!(f.listen_label(), "Server:*:0");

        assert!(Forward::parse("0 localhost:80", Local).unwrap_err().contains("Port 0"));
        assert!(Forward::parse("8080 host:0", Local).is_err());
        assert!(Forward::parse("x host:80", Local).is_err());
    }

    #[test]
    fn parses_unix_socket_forwards() {
        let unix = |path: &str| path.to_string();
        for (spec, listen, target) in [
            ("8080 /run/app.sock", port(None, 8080), Target::Unix(unix("/run/app.sock"))),
            ("127.0.0.1:8080:/run/app.sock", port(Some("127.0.0.1"), 8080), Target::Unix(unix("/run/app.sock"))),
            ("~/db.sock localhost:5432", Listen::Unix(unix("~/db.sock")), tcp("localhost", 5432)),
            ("/tmp/a.sock:db:5432", Listen::Unix(unix("/tmp/a.sock")), tcp("db", 5432)),
            ("/tmp/a.sock:/run/b.sock", Listen::Unix(unix("/tmp/a.sock")), Target::Unix(unix("/run/b.sock"))),
        ] {
            let f = Forward::parse(spec, ForwardKind::Local).unwrap();
            assert_eq!((f.listen, f.target), (listen, Some(target)), "{spec}");
        }
        // The server forwards to a socket here, but can't listen on one.
        let f = Forward::parse("9000 /run/user/1000/app.sock", ForwardKind::Remote).unwrap();
        assert_eq!(f.target_label(), "/run/user/1000/app.sock");
        assert!(Forward::parse("/tmp/x.sock localhost:80", ForwardKind::Remote).unwrap_err().contains("Unix"));
    }

    #[test]
    fn parses_dynamic_forwards() {
        use ForwardKind::{Dynamic, Local, Remote};
        let f = Forward::parse("1080", Dynamic).unwrap();
        assert_eq!((f.listen_label().as_str(), f.target_label().as_str(), f.kind()), ("localhost:1080", "SOCKS", Dynamic));
        assert_eq!(Forward::parse("[::1]:1080", Dynamic).unwrap().listen, port(Some("::1"), 1080));
        // `RemoteForward` with only a port: a SOCKS proxy on the server.
        let f = Forward::parse("*:1080", Remote).unwrap();
        assert_eq!((&f.listen, &f.target, f.kind()), (&port(Some("*"), 1080), &None, Remote));
        assert!(Forward::parse("1080", Local).unwrap_err().contains("DynamicForward"));
        assert!(Forward::parse("1080 host:80", Dynamic).is_err());
        assert!(Forward::parse("0", Dynamic).unwrap_err().contains("Port 0"));
    }

    #[test]
    fn resolves_algorithm_lists_with_ssh_prefixes() {
        let supported = ["curve25519-sha256", "ecdh-sha2-nistp256", "diffie-hellman-group14-sha1", "diffie-hellman-group1-sha1"];
        assert_eq!(algorithm_list("ecdh-sha2-nistp256,made-up", &supported).unwrap(), "ecdh-sha2-nistp256");
        assert_eq!(algorithm_list("+diffie-hellman-group1-sha1", &supported).unwrap(), supported.join(","));
        assert_eq!(
            algorithm_list("-diffie-hellman-*", &supported).unwrap(),
            "curve25519-sha256,ecdh-sha2-nistp256"
        );
        assert_eq!(
            algorithm_list("^diffie-hellman-group14-sha1", &supported).unwrap(),
            "diffie-hellman-group14-sha1,curve25519-sha256,ecdh-sha2-nistp256,diffie-hellman-group1-sha1"
        );
        assert!(algorithm_list("only-openssh@openssh.com", &supported).is_err());
    }

    #[test]
    fn settings_check_and_resolve_everything() {
        let options = Options {
            connect_timeout: Some(3),
            server_alive_interval: Some(0),
            preferred_authentications: Some("password, publickey,hostbased".into()),
            proxy_command: Some("nc %h %p".into()),
            set_env: vec!["TERM=xterm".into(), "LANG=de_DE.UTF-8".into()],
            local_forward: vec!["8080 localhost:80".into()],
            remote_forward: vec!["9000 localhost:9000".into()],
            dynamic_forward: vec!["1080".into()],
            ciphers: Some("^aes256-gcm@openssh.com".into()),
            forward_agent: Some("$MY_AGENT".into()),
            ..Options::default()
        };
        let expand = |value: &str| value.replace("%h", "example.org").replace("%p", "22");
        let settings = options.settings(&expand).unwrap();
        assert_eq!(settings.connect_timeout, Duration::from_secs(3));
        assert_eq!(settings.keepalive, None);
        assert_eq!(settings.auth_methods, [AuthMethod::Password, AuthMethod::PublicKey]);
        assert_eq!(settings.proxy_command.as_deref(), Some("nc example.org 22"));
        assert_eq!(settings.term, "xterm");
        assert_eq!(settings.env, [("LANG".to_string(), "de_DE.UTF-8".to_string())]);
        let kinds: Vec<ForwardKind> = settings.forwards.iter().map(Forward::kind).collect();
        assert_eq!(kinds, [ForwardKind::Local, ForwardKind::Remote, ForwardKind::Dynamic]);
        assert_eq!(settings.algorithms, [(AlgorithmKind::Cipher, "^aes256-gcm@openssh.com".to_string())]);
        assert_eq!(settings.forward_agent, ForwardAgent::Socket("$MY_AGENT".into()));
        assert_eq!(settings.for_jump().forward_agent, ForwardAgent::Off);

        let defaults = Options::default().settings(&no_expand).unwrap();
        assert_eq!(defaults.keepalive, Some((DEFAULT_ALIVE_INTERVAL, DEFAULT_ALIVE_COUNT_MAX)));
        assert_eq!(defaults.auth_methods, AuthMethod::DEFAULT);
        assert!(defaults.proxy_command.is_none() && defaults.forwards.is_empty());
        assert_eq!(defaults.forward_agent, ForwardAgent::Off);
        for (value, parsed) in [("Yes", ForwardAgent::Login), (" no ", ForwardAgent::Off), ("", ForwardAgent::Off)] {
            assert_eq!(ForwardAgent::parse(Some(value)), parsed, "{value:?}");
        }

        let bad = |options: Options| options.settings(&no_expand).unwrap_err();
        assert!(bad(Options { set_env: vec!["1X=y".into()], ..Options::default() }).contains("Variablenname"));
        assert!(bad(Options { preferred_authentications: Some("gssapi-with-mic".into()), ..Options::default() }).contains("keine Methode"));
        assert!(bad(Options { preferred_authentications: Some("magic".into()), ..Options::default() }).contains("magic"));
        assert!(bad(Options { macs: Some("-".into()), ..Options::default() }).contains("MACs"));
    }

    #[test]
    fn send_env_passes_matching_local_variables() {
        // SAFETY: only this test uses these variable names.
        unsafe {
            std::env::set_var("TERMINAAL_SENDENV_A", "1");
            std::env::set_var("TERMINAAL_SENDENV_B", "2");
        }
        let options = Options {
            send_env: vec!["TERMINAAL_SENDENV_*".into(), "TERM".into()],
            set_env: vec!["TERMINAAL_SENDENV_B=override".into()],
            ..Options::default()
        };
        let env = options.settings(&no_expand).unwrap().env;
        let ours: Vec<_> = env.iter().filter(|(n, _)| n.starts_with("TERMINAAL_SENDENV_")).collect();
        assert_eq!(ours.len(), 2);
        assert_eq!(ours[0], &("TERMINAAL_SENDENV_A".to_string(), "1".to_string()));
        assert_eq!(ours[1], &("TERMINAAL_SENDENV_B".to_string(), "override".to_string()));
        assert!(!env.iter().any(|(n, _)| n == "TERM"), "TERM goes with the PTY request");
    }
}
