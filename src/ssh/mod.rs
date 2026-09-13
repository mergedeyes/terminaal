//! SSH hosts and how to reach them.
//!
//! Hosts come from two places: the ones saved in Terminaal
//! (`~/.config/terminaal/hosts.toml`, edited in the sidebar) and the
//! concrete `Host` entries of `~/.ssh/config` (read-only; see
//! [`parse_ssh_config`] for what's understood there). A [`Catalog`] of
//! both plus the key store ([`keys`]) turns a [`Host`] into an
//! [`SshTarget`] -- jump hosts resolved, the exact keys to offer worked
//! out -- which [`connection`] then connects to inside a tab.
//!
//! A host can have several logins (user + key); the first one is the
//! default. Everything else a host can be configured with lives in
//! [`options`].

mod agent_forward;
pub mod connection;
pub mod forward;
mod socks;
pub mod keys;
pub mod known_hosts;
pub mod options;

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::i18n::t;
use options::{AddressFamily, HostKeyCheck, Options, Settings};

/// `Include` nesting limit (OpenSSH uses 16 as well).
const MAX_INCLUDE_DEPTH: usize = 16;
/// ProxyJump nesting limit -- also what stops a jump host that
/// (indirectly) jumps via itself.
const MAX_JUMP_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    /// Display name in the sidebar; also what `ProxyJump` refers to.
    pub name: String,
    /// Hostname or IP address to connect to.
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Login name; empty means the local user name, like `ssh` does.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    /// Name of a key from the key store -- the only key offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Private key files (`IdentityFile`, `~/` allowed). Older
    /// hosts.toml files have a single `identity = "…"`, which still loads.
    #[serde(default, rename = "identity", deserialize_with = "one_or_many", skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<String>,
    /// Offer only `identities`, never other agent keys (`IdentitiesOnly`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub identities_only: bool,
    /// Agent socket (`IdentityAgent`): a path, `SSH_AUTH_SOCK` or `none`.
    /// Unset: `$SSH_AUTH_SOCK`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_agent: Option<String>,
    /// Jump hosts (`ProxyJump`): comma-separated host names (saved or from
    /// `~/.ssh/config`) or `[user@]host[:port]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_jump: Option<String>,
    #[serde(flatten)]
    pub options: Options,
    /// More logins besides the default one (`user`/`key`) -- e.g. a
    /// personal and an admin account. Jump hosts always use the default.
    #[serde(default, rename = "login", skip_serializing_if = "Vec::is_empty")]
    pub logins: Vec<Login>,
}

/// A user name and the key to log in with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Login {
    /// Empty means the local user name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    /// Name of a key from the key store; `None`: the host's
    /// `IdentityFile`s, or whatever the agent and `~/.ssh/id_*` offer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl Login {
    /// `user` (or the local user) and the key, for menus.
    pub fn label(&self) -> String {
        let user = if self.user.is_empty() { local_user() } else { self.user.clone() };
        match &self.key {
            Some(key) => format!("{user} · {key}"),
            None => user,
        }
    }
}

fn default_port() -> u16 {
    22
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn one_or_many<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(one) => vec![one],
        OneOrMany::Many(many) => many,
    })
}

impl Host {
    pub fn new(name: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            host: host.into(),
            port: 22,
            user: String::new(),
            key: None,
            identities: Vec::new(),
            identities_only: false,
            identity_agent: None,
            proxy_jump: None,
            options: Options::default(),
            logins: Vec::new(),
        }
    }

    /// Every login: the default one (`user`/`key`) first, then `logins`.
    pub fn all_logins(&self) -> Vec<Login> {
        let mut all = vec![Login { user: self.user.clone(), key: self.key.clone() }];
        all.extend(self.logins.iter().cloned());
        all
    }

    /// The host as it logs in with login `index` of [`Host::all_logins`].
    pub fn with_login(&self, index: usize) -> Option<Host> {
        let login = self.all_logins().into_iter().nth(index)?;
        Some(Host { user: login.user, key: login.key, logins: Vec::new(), ..self.clone() })
    }

    /// Index (in [`Host::all_logins`]) of the first login as `user`.
    pub fn login_index(&self, user: &str) -> Option<usize> {
        self.all_logins().iter().position(|login| login.user == user)
    }

    /// Whether any login uses the key store's key `name`.
    pub fn uses_key(&self, name: &str) -> bool {
        self.all_logins().iter().any(|login| login.key.as_deref() == Some(name))
    }

    pub fn rename_key(&mut self, old: &str, new: &str) {
        for key in std::iter::once(&mut self.key).chain(self.logins.iter_mut().map(|login| &mut login.key)) {
            if key.as_deref() == Some(old) {
                *key = Some(new.to_string());
            }
        }
    }

    /// `user@host:port` for display, leaving out whatever is default.
    pub fn address(&self) -> String {
        let mut address = String::new();
        if !self.user.is_empty() {
            address.push_str(&self.user);
            address.push('@');
        }
        address.push_str(&self.host);
        if self.port != 22 {
            address.push_str(&format!(":{}", self.port));
        }
        address
    }

    /// The `ProxyJump` setting, unless it's empty or `none`.
    pub fn jump(&self) -> Option<&str> {
        self.proxy_jump.as_deref().map(str::trim).filter(|j| !j.is_empty() && !j.eq_ignore_ascii_case("none"))
    }
}

/// Everything a connection needs, resolved.
#[derive(Clone, Debug)]
pub struct SshTarget {
    /// `user@host` (plus `:port` if it isn't 22) -- also the tab title.
    pub label: String,
    /// The host's name in the sidebar (saved or `~/.ssh/config`).
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthPlan,
    pub settings: Settings,
    /// Hosts to tunnel through first (ProxyJump), in order. Jump hosts'
    /// own jumps are flattened into this list; the entries' `jumps` are
    /// always empty.
    pub jumps: Vec<SshTarget>,
}

/// Which keys to offer, in which order: agent keys first (like ssh),
/// then key files, then -- unless restricted -- the default
/// `~/.ssh/id_*` files.
#[derive(Clone, Debug, Default)]
pub struct AuthPlan {
    /// Private key files to try.
    pub files: Vec<PathBuf>,
    /// Public key blobs (SSH wire format) the agent may sign with, when
    /// `restrict` is set.
    pub agent_keys: Vec<Vec<u8>>,
    /// Offer nothing but the keys above: no other agent keys, no default
    /// files. Set for a chosen key or `IdentitiesOnly`; it keeps servers
    /// from hitting `MaxAuthTries` when the agent holds many keys.
    pub restrict: bool,
    pub agent: AgentSocket,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AgentSocket {
    /// `$SSH_AUTH_SOCK`.
    #[default]
    Env,
    Path(PathBuf),
    Off,
}

/// Saved hosts, `~/.ssh/config` hosts and keys -- everything a host's
/// settings can refer to.
#[derive(Clone, Default)]
pub struct Catalog {
    pub saved: Vec<Host>,
    pub config: Vec<Host>,
    pub keys: Vec<keys::Key>,
}

impl Catalog {
    /// Read everything from disk; errors are for display.
    pub fn load() -> Result<Self, String> {
        Ok(Self { saved: load_hosts()?, config: ssh_config_hosts(), keys: keys::load_keys()? })
    }

    /// Saved hosts win over `~/.ssh/config` ones of the same name.
    pub fn find(&self, name: &str) -> Option<&Host> {
        self.saved.iter().chain(&self.config).find(|host| host.name == name)
    }

    pub fn target(&self, host: &Host) -> Result<SshTarget, String> {
        self.resolve(host, 0)
    }

    fn resolve(&self, host: &Host, depth: usize) -> Result<SshTarget, String> {
        let user = if host.user.is_empty() { local_user() } else { host.user.clone() };
        let label = if host.port == 22 { format!("{user}@{}", host.host) } else { format!("{user}@{}:{}", host.host, host.port) };
        let expand = |value: &str| expand_tokens(value, &host.name, &host.host, &user, host.port);
        let settings = host.options.settings(&expand).map_err(|err| format!("{}: {err}", host.name))?;
        let mut jumps = Vec::new();
        if let Some(spec) = host.jump() {
            if settings.proxy_command.is_some() {
                return Err(t!("catalog-jump-and-proxy", name = &host.name));
            }
            if depth >= MAX_JUMP_DEPTH {
                return Err(t!("catalog-jump-loop", name = &host.name));
            }
            for hop in spec.split(',').map(str::trim).filter(|hop| !hop.is_empty()) {
                let hop_host = match self.find(hop) {
                    Some(known) => known.clone(),
                    None => parse_jump(hop)?,
                };
                let mut hop_target = self.resolve(&hop_host, depth + 1)?;
                hop_target.settings = hop_target.settings.for_jump();
                jumps.append(&mut hop_target.jumps);
                jumps.push(hop_target);
            }
        }
        Ok(SshTarget {
            label,
            name: host.name.clone(),
            host: host.host.clone(),
            port: host.port,
            user,
            auth: self.auth_plan(host)?,
            settings,
            jumps,
        })
    }

    fn auth_plan(&self, host: &Host) -> Result<AuthPlan, String> {
        let agent = match host.identity_agent.as_deref().map(str::trim) {
            None | Some("" | "SSH_AUTH_SOCK") => AgentSocket::Env,
            Some(value) if value.eq_ignore_ascii_case("none") => AgentSocket::Off,
            Some(path) => AgentSocket::Path(expand_tilde(path)),
        };
        if let Some(name) = &host.key {
            let key = self
                .keys
                .iter()
                .find(|key| &key.name == name)
                .ok_or_else(|| t!("catalog-key-gone", key = name, host = &host.name))?;
            return Ok(AuthPlan {
                files: key.file_path().into_iter().collect(),
                // A file key held by the agent too gets signed there -- no
                // passphrase prompt.
                agent_keys: key.public_blob().into_iter().collect(),
                restrict: true,
                agent,
            });
        }
        let mut files: Vec<PathBuf> = host.identities.iter().map(|file| expand_tilde(file)).collect();
        // Like ssh: IdentitiesOnly without IdentityFile means the default files.
        if host.identities_only && files.is_empty() {
            files = default_key_files();
        }
        let agent_keys =
            if host.identities_only { files.iter().filter_map(|file| keys::public_blob_of_file(file).ok()).collect() } else { Vec::new() };
        Ok(AuthPlan { files, agent_keys, restrict: host.identities_only, agent })
    }
}

/// `[user@]host[:port]`, with `[address]:port` for IPv6.
fn parse_jump(spec: &str) -> Result<Host, String> {
    let invalid = || t!("catalog-bad-jump", spec = spec);
    let (user, rest) = spec.rsplit_once('@').unwrap_or(("", spec));
    let (host, port) = match rest.strip_prefix('[') {
        Some(inner) => {
            let (address, after) = inner.split_once(']').ok_or_else(invalid)?;
            (address, after.strip_prefix(':'))
        }
        None => match rest.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => (host, Some(port)),
            _ => (rest, None),
        },
    };
    let port = match port {
        Some(port) => port.parse::<u16>().ok().filter(|&p| p > 0).ok_or_else(invalid)?,
        None => 22,
    };
    if host.is_empty() {
        return Err(invalid());
    }
    Ok(Host { user: user.to_string(), port, ..Host::new(spec, host) })
}

pub fn local_user() -> String {
    std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_else(|_| "root".into())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

pub fn expand_tilde(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), home()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

pub fn ssh_dir() -> Option<PathBuf> {
    Some(home()?.join(".ssh"))
}

/// The key files ssh tries when none is configured, those that exist.
pub fn default_key_files() -> Vec<PathBuf> {
    let Some(dir) = ssh_dir() else { return Vec::new() };
    ["id_ed25519", "id_ecdsa", "id_rsa"].into_iter().map(|name| dir.join(name)).filter(|path| path.exists()).collect()
}

/// `/home/jan/x` → `~/x`, for messages.
pub fn display_path(path: &Path) -> String {
    match home().as_deref().and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn hosts_path() -> Option<PathBuf> {
    Some(crate::config::Config::dir()?.join("hosts.toml"))
}

#[derive(Default, Serialize, Deserialize)]
struct HostsFile {
    #[serde(default, rename = "host")]
    hosts: Vec<Host>,
}

/// Errors are meant for display in the UI.
pub fn load_hosts() -> Result<Vec<Host>, String> {
    let Some(path) = hosts_path() else { return Ok(Vec::new()) };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    toml::from_str::<HostsFile>(&text)
        .map(|file| file.hosts)
        .map_err(|err| t!("common-file-invalid", path = path.display().to_string(), err = err.to_string()))
}

pub fn save_hosts(hosts: &[Host]) -> Result<(), String> {
    let path = hosts_path().ok_or_else(|| t!("common-home-unset"))?;
    let text = toml::to_string_pretty(&HostsFile { hosts: hosts.to_vec() }).map_err(|err| err.to_string())?;
    write_atomically(&path, &text).map_err(|err| format!("{}: {err}", path.display()))
}

/// Write-then-rename, so a crash can't leave a truncated file.
pub(crate) fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.to_path_buf().into_os_string();
    tmp.push(".tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// The concrete hosts in `~/.ssh/config`; none if it's missing or unreadable.
pub fn ssh_config_hosts() -> Vec<Host> {
    let Some(path) = ssh_dir().map(|dir| dir.join("config")) else { return Vec::new() };
    std::fs::read_to_string(path).map(|text| parse_ssh_config(&text, &include_files)).unwrap_or_default()
}

/// The contents of the files an `Include` pattern names. Relative paths
/// are relative to `~/.ssh`, as in ssh; wildcards work in the file name.
fn include_files(pattern: &str) -> Vec<String> {
    let path = expand_tilde(pattern);
    let path = match ssh_dir() {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path,
    };
    let (Some(dir), Some(file_pattern)) = (path.parent(), path.file_name().map(|n| n.to_string_lossy().into_owned())) else {
        return Vec::new();
    };
    if !file_pattern.contains(['*', '?']) {
        return std::fs::read_to_string(&path).into_iter().collect();
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.file_name().is_some_and(|n| glob(&file_pattern, &n.to_string_lossy())))
        .collect();
    files.sort();
    files.into_iter().filter_map(|file| std::fs::read_to_string(file).ok()).collect()
}

/// One `Host` or `Match` block (or the implicit global one at the top)
/// and the options in it.
struct Block {
    condition: Condition,
    options: Vec<(String, String)>,
}

enum Condition {
    Host(Vec<String>),
    Match(Vec<Criterion>),
}

struct Criterion {
    negated: bool,
    kind: String,
    patterns: Vec<String>,
}

impl Criterion {
    /// `None` for criteria we can't evaluate (`exec`, `canonical`,
    /// `tagged`, …) -- a block with one of those never applies, negated
    /// or not. `exec` in particular would mean running arbitrary commands.
    fn holds(&self, alias: &str, hostname: &str, user: &str) -> Option<bool> {
        let result = match self.kind.as_str() {
            // One pass over the config is "the final pass" for us.
            "all" | "final" => true,
            "host" => host_matches(&self.patterns, hostname),
            "originalhost" => host_matches(&self.patterns, alias),
            "user" => host_matches(&self.patterns, user),
            "localuser" => host_matches(&self.patterns, &local_user()),
            _ => return None,
        };
        Some(result != self.negated)
    }
}

impl Block {
    fn applies(&self, alias: &str, hostname: &str, user: &str) -> bool {
        match &self.condition {
            Condition::Host(patterns) => host_matches(patterns, alias),
            Condition::Match(criteria) => criteria.iter().all(|c| c.holds(alias, hostname, user) == Some(true)),
        }
    }
}

/// Understands `Host` and `Match` blocks, `Include`, the options in
/// [`Host`] (`HostName`, `User`, `Port`, `IdentityFile` (repeatable),
/// `IdentitiesOnly`, `IdentityAgent`, `ProxyJump`) and those in
/// [`Options`] -- with the `%h`, `%n`, `%p`, `%r`, `%u`, `%d` tokens and
/// `${VAR}` in paths. As in ssh, the first value found for an option wins
/// (`IdentityFile`, `SetEnv`, `SendEnv` and the forwards accumulate),
/// `ProxyJump` and `ProxyCommand` exclude each other (the first one
/// wins), and options before the first block apply everywhere. `include`
/// returns the contents of the files an `Include` pattern names.
fn parse_ssh_config(text: &str, include: &dyn Fn(&str) -> Vec<String>) -> Vec<Host> {
    let mut options = Vec::new();
    collect_options(text, 0, include, &mut options);

    let mut blocks = vec![Block { condition: Condition::Host(vec!["*".into()]), options: Vec::new() }];
    for (key, value) in options {
        match key.as_str() {
            "host" => blocks.push(Block { condition: Condition::Host(words(&value)), options: Vec::new() }),
            "match" => blocks.push(Block { condition: Condition::Match(parse_criteria(&value)), options: Vec::new() }),
            _ => blocks.last_mut().expect("starts with the global block").options.push((key, value)),
        }
    }

    let mut names: Vec<&str> = Vec::new();
    for block in &blocks {
        if let Condition::Host(patterns) = &block.condition {
            for pattern in patterns {
                if !pattern.contains(['*', '?']) && !pattern.starts_with('!') && !names.contains(&pattern.as_str()) {
                    names.push(pattern);
                }
            }
        }
    }
    names.into_iter().map(|name| resolve_config_host(&blocks, name)).collect()
}

fn collect_options(text: &str, depth: usize, include: &dyn Fn(&str) -> Vec<String>, out: &mut Vec<(String, String)>) {
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (key, value) = split_option(line);
        if key != "include" {
            out.push((key, value));
        } else if depth < MAX_INCLUDE_DEPTH {
            for pattern in words(&value) {
                for included in include(&pattern) {
                    collect_options(&included, depth + 1, include, out);
                }
            }
        }
    }
}

fn parse_criteria(value: &str) -> Vec<Criterion> {
    let mut criteria = Vec::new();
    let mut tokens = value.split_whitespace();
    while let Some(token) = tokens.next() {
        let (negated, kind) = match token.strip_prefix('!') {
            Some(kind) => (true, kind),
            None => (false, token),
        };
        let kind = kind.to_lowercase();
        let patterns = if matches!(kind.as_str(), "all" | "canonical" | "final") {
            Vec::new()
        } else {
            tokens.next().unwrap_or_default().split(',').map(str::to_string).collect()
        };
        criteria.push(Criterion { negated, kind, patterns });
    }
    criteria
}

fn resolve_config_host(blocks: &[Block], alias: &str) -> Host {
    let (mut hostname, mut user, mut port) = (None::<String>, None::<String>, None::<u16>);
    let (mut identities, mut identities_only, mut agent) = (Vec::<String>::new(), None, None);
    // `ProxyJump` (true) or `ProxyCommand` (false), whichever came first.
    let mut proxy: Option<(bool, String)> = None;
    let mut options = Options::default();
    let (mut compression, mut family, mut strict, mut known_hosts) = (None, None, None, None::<String>);
    for block in blocks {
        let current_host = hostname.clone().unwrap_or_else(|| alias.to_string());
        let current_user = user.clone().unwrap_or_else(local_user);
        if !block.applies(alias, &current_host, &current_user) {
            continue;
        }
        for (key, raw) in &block.options {
            let value = unquote(raw).to_string();
            let set = |slot: &mut Option<String>| {
                if slot.is_none() {
                    *slot = Some(value.clone());
                }
            };
            match key.as_str() {
                "hostname" if hostname.is_none() => hostname = Some(value.replace("%h", alias)),
                "user" => set(&mut user),
                "port" if port.is_none() => port = value.parse().ok(),
                "identityfile" if !identities.contains(&value) => identities.push(value),
                "identitiesonly" if identities_only.is_none() => identities_only = Some(yes(&value)),
                "identityagent" => set(&mut agent),
                "proxyjump" if proxy.is_none() => proxy = Some((true, value)),
                "proxycommand" if proxy.is_none() => proxy = Some((false, value)),
                "connecttimeout" if options.connect_timeout.is_none() => options.connect_timeout = value.parse().ok(),
                "serveraliveinterval" if options.server_alive_interval.is_none() => {
                    options.server_alive_interval = value.parse().ok();
                }
                "serveralivecountmax" if options.server_alive_count_max.is_none() => {
                    options.server_alive_count_max = value.parse().ok();
                }
                "compression" if compression.is_none() => compression = Some(yes(&value)),
                "addressfamily" if family.is_none() => family = AddressFamily::parse(&value),
                "stricthostkeychecking" if strict.is_none() => strict = HostKeyCheck::parse(&value),
                "userknownhostsfile" if known_hosts.is_none() => known_hosts = words(raw).into_iter().next(),
                "preferredauthentications" => set(&mut options.preferred_authentications),
                "forwardagent" => set(&mut options.forward_agent),
                "remotecommand" => set(&mut options.remote_command),
                // Per variable, the first value wins.
                "setenv" => {
                    for entry in words(raw) {
                        let name = entry.split('=').next().unwrap_or_default();
                        if !options.set_env.iter().any(|e| e.split('=').next() == Some(name)) {
                            options.set_env.push(entry);
                        }
                    }
                }
                "sendenv" => options.send_env.extend(words(raw).into_iter().filter(|w| !w.starts_with('-'))),
                "localforward" => options.local_forward.push(words(raw).join(" ")),
                "remoteforward" => options.remote_forward.push(words(raw).join(" ")),
                "dynamicforward" => options.dynamic_forward.push(words(raw).join(" ")),
                "kexalgorithms" => set(&mut options.kex_algorithms),
                "hostkeyalgorithms" => set(&mut options.host_key_algorithms),
                "ciphers" => set(&mut options.ciphers),
                "macs" => set(&mut options.macs),
                _ => {}
            }
        }
    }
    let host = hostname.unwrap_or_else(|| alias.to_string());
    let port = port.unwrap_or(22);
    let remote_user = user.clone().unwrap_or_else(local_user);
    let expand = |value: &str| expand_tokens(value, alias, &host, &remote_user, port);
    let (proxy_jump, proxy_command) = match proxy {
        Some((true, jump)) => (Some(jump), None),
        Some((false, command)) => (None, Some(command)),
        None => (None, None),
    };
    // ProxyCommand and RemoteCommand keep their tokens until the
    // connection is resolved ([`Catalog::target`]), as for saved hosts.
    options.proxy_command = proxy_command;
    options.compression = compression.unwrap_or(false);
    // A first `no` has won as well; unset is the same.
    options.forward_agent = options.forward_agent.take().filter(|value| !value.eq_ignore_ascii_case("no"));
    options.address_family = family.unwrap_or_default();
    options.strict_host_key_checking = strict.unwrap_or_default();
    options.user_known_hosts_file = known_hosts.map(|file| expand(&file));
    Host {
        identities: identities.iter().map(|file| expand(file)).collect(),
        identities_only: identities_only.unwrap_or(false),
        identity_agent: agent.map(|a| expand(&a)),
        proxy_jump,
        port,
        user: user.unwrap_or_default(),
        options,
        ..Host::new(alias, host.clone())
    }
}

fn yes(value: &str) -> bool {
    value.eq_ignore_ascii_case("yes")
}

/// ssh's `%` tokens (the ones that make sense here) and `${VAR}`.
fn expand_tokens(value: &str, alias: &str, hostname: &str, user: &str, port: u16) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('d') => out.push_str(&home().map(|h| h.display().to_string()).unwrap_or_default()),
            Some('u') => out.push_str(&local_user()),
            Some('h') => out.push_str(hostname),
            Some('n') => out.push_str(alias),
            Some('p') => out.push_str(&port.to_string()),
            Some('r') => out.push_str(user),
            Some(other) => out.extend(['%', other]),
            None => out.push('%'),
        }
    }
    while let Some(start) = out.find("${") {
        let Some(len) = out[start..].find('}') else { break };
        let value = std::env::var(&out[start + 2..start + len]).unwrap_or_default();
        out.replace_range(start..=start + len, &value);
    }
    out
}

/// `Keyword value`, `Keyword=value` or `Keyword = "value"`; the keyword is
/// case-insensitive, so it comes back lowercased. The value stays as
/// written -- quotes and all -- for [`words`]; see [`unquote`].
fn split_option(line: &str) -> (String, String) {
    let end = line.find(|c: char| c.is_whitespace() || c == '=').unwrap_or(line.len());
    let (key, rest) = line.split_at(end);
    let value = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '=').trim();
    (key.to_lowercase(), value.to_string())
}

/// A single-valued option's value without surrounding quotes.
fn unquote(value: &str) -> &str {
    value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(value)
}

/// Whitespace-separated words, where `"…"` may contain spaces -- for the
/// options that take several values.
fn words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut in_word) = (false, false);
    for c in value.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            c => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

/// Like ssh: any positive match and no negated one (case-insensitive).
fn host_matches(patterns: &[String], name: &str) -> bool {
    let name = name.to_lowercase();
    let mut matched = false;
    for pattern in patterns.iter().map(|p| p.to_lowercase()) {
        match pattern.strip_prefix('!') {
            Some(negated) if glob(negated, &name) => return false,
            Some(_) => {}
            None => matched |= glob(&pattern, &name),
        }
    }
    matched
}

/// `*` matches any run of characters, `?` exactly one.
pub(crate) fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            backtrack = Some((pi, ti));
            pi += 1;
        } else if let Some((star, matched)) = backtrack {
            pi = star + 1;
            ti = matched + 1;
            backtrack = Some((star, matched + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_includes(_: &str) -> Vec<String> {
        Vec::new()
    }

    const CONFIG: &str = r#"
Host web web2
    HostName %h.example.com
    Port=2222

Host db
    HostName 10.0.0.5
    User = "admin"
    IdentityFile ~/.ssh/db_key
    IdentitiesOnly yes

Host *.internal !secret.internal
    User internal

Host secret.internal jump.internal
    Port 2200

Match host 10.0.0.* user admin
    ProxyJump bastion
    IdentityFile ~/.ssh/%r_%h

Match exec "true"
    User never

Host *
    User fallback
    IdentityFile ~/.ssh/id_default
    IdentityAgent ${TERMINAAL_TEST_AGENT}/agent.sock
"#;

    fn parse(text: &str) -> Vec<Host> {
        // SAFETY: tests in this module are the only ones touching this variable.
        unsafe { std::env::set_var("TERMINAAL_TEST_AGENT", "/run/agent") };
        parse_ssh_config(text, &no_includes)
    }

    #[test]
    fn parses_concrete_hosts_with_first_match_wins() {
        let hosts = parse(CONFIG);
        let names: Vec<_> = hosts.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["web", "web2", "db", "secret.internal", "jump.internal"]);

        let web = &hosts[1];
        assert_eq!((web.host.as_str(), web.port, web.user.as_str()), ("web2.example.com", 2222, "fallback"));
        assert_eq!(web.identities, ["~/.ssh/id_default"]);
        assert_eq!(web.identity_agent.as_deref(), Some("/run/agent/agent.sock"));
        assert!(!web.identities_only && web.proxy_jump.is_none());

        // `!secret.internal` excludes it from the *.internal block.
        assert_eq!((hosts[3].user.as_str(), hosts[3].port), ("fallback", 2200));
        assert_eq!((hosts[4].user.as_str(), hosts[4].port), ("internal", 2200));
    }

    #[test]
    fn match_blocks_see_hostname_and_user_so_far() {
        let db = &parse(CONFIG)[2];
        assert_eq!((db.host.as_str(), db.user.as_str()), ("10.0.0.5", "admin"));
        assert!(db.identities_only);
        assert_eq!(db.proxy_jump.as_deref(), Some("bastion"));
        // Accumulated, tokens expanded; `Match exec` never applies.
        assert_eq!(db.identities, ["~/.ssh/db_key", "~/.ssh/admin_10.0.0.5", "~/.ssh/id_default"]);
    }

    #[test]
    fn includes_are_spliced_in_place() {
        let include = |pattern: &str| match pattern {
            "config.d/*" => vec!["Host extra\n  HostName extra.example\n  Include nested\n".to_string()],
            "nested" => vec!["  Port 2022\n".to_string()],
            _ => Vec::new(),
        };
        let hosts = parse_ssh_config("Include config.d/*\nHost main\n  User me\n", &include);
        assert_eq!(hosts[0].name, "extra");
        assert_eq!((hosts[0].host.as_str(), hosts[0].port), ("extra.example", 2022));
        assert_eq!(hosts[1].user, "me");
    }

    #[test]
    fn options_before_the_first_host_win_like_in_ssh() {
        let hosts = parse("User early\nHost a\n    User late\n    Port 2022\n");
        assert_eq!((hosts[0].user.as_str(), hosts[0].port), ("early", 2022));
    }

    #[test]
    fn globs() {
        assert!(glob("*.example.com", "a.b.example.com"));
        assert!(glob("h?st", "host"));
        assert!(!glob("h?st", "hoost"));
        assert!(glob("*", ""));
        assert!(!glob("web", "web2"));
    }

    #[test]
    fn parses_jump_specs() {
        let hop = parse_jump("admin@[2001:db8::1]:2200").unwrap();
        assert_eq!((hop.user.as_str(), hop.host.as_str(), hop.port), ("admin", "2001:db8::1", 2200));
        let hop = parse_jump("bastion.example").unwrap();
        assert_eq!((hop.user.as_str(), hop.host.as_str(), hop.port), ("", "bastion.example", 22));
        assert!(parse_jump("x:notaport").is_err());
    }

    #[test]
    fn resolves_jump_chains_and_named_keys() {
        let mut bastion = Host::new("bastion", "bastion.example");
        bastion.proxy_jump = Some("gate@gate.example:2222".into());
        let mut target = Host::new("app", "10.0.0.7");
        target.user = "deploy".into();
        target.proxy_jump = Some("bastion".into());
        target.key = Some("work".into());
        let catalog = Catalog {
            saved: vec![target.clone()],
            config: vec![bastion],
            keys: vec![keys::Key {
                name: "work".into(),
                file: Some("/nonexistent/key".into()),
                agent_key: None,
                generated: false,
            }],
        };

        let resolved = catalog.target(&target).unwrap();
        assert_eq!(resolved.label, "deploy@10.0.0.7");
        let hops: Vec<_> = resolved.jumps.iter().map(|j| j.label.as_str()).collect();
        assert_eq!(hops.len(), 2);
        assert_eq!(hops[0], "gate@gate.example:2222");
        assert!(hops[1].ends_with("@bastion.example"));
        assert!(resolved.auth.restrict);
        assert_eq!(resolved.auth.files, [PathBuf::from("/nonexistent/key")]);

        target.key = Some("gone".into());
        assert!(catalog.target(&target).is_err());

        let mut looping = Host::new("loop", "loop.example");
        looping.proxy_jump = Some("loop".into());
        let catalog = Catalog { saved: vec![looping.clone()], ..Catalog::default() };
        assert!(catalog.target(&looping).is_err());
    }

    #[test]
    fn hosts_file_round_trips_and_reads_the_old_identity_string() {
        let mut full = Host::new("b", "b.example");
        full.port = 2222;
        full.user = "me".into();
        full.key = Some("work".into());
        full.identities = vec!["~/.ssh/k".into()];
        full.identities_only = true;
        full.identity_agent = Some("~/.1password/agent.sock".into());
        full.proxy_jump = Some("a".into());
        let hosts = vec![Host::new("a", "a.example"), full];
        let text = toml::to_string_pretty(&HostsFile { hosts: hosts.clone() }).unwrap();
        assert!(text.contains("[[host]]"));
        assert_eq!(toml::from_str::<HostsFile>(&text).unwrap().hosts, hosts);

        let old: HostsFile = toml::from_str("[[host]]\nname = \"o\"\nhost = \"o.example\"\nidentity = \"~/.ssh/old\"\n").unwrap();
        assert_eq!(old.hosts[0].identities, ["~/.ssh/old"]);
    }

    #[test]
    fn hosts_file_round_trips_logins_and_options() {
        let mut host = Host::new("full", "full.example");
        host.user = "me".into();
        host.key = Some("work".into());
        host.logins = vec![Login { user: "root".into(), key: Some("admin".into()) }, Login::default()];
        host.options = Options {
            connect_timeout: Some(5),
            server_alive_interval: Some(0),
            compression: true,
            address_family: AddressFamily::Inet6,
            strict_host_key_checking: HostKeyCheck::AcceptNew,
            set_env: vec!["LANG=de_DE.UTF-8".into()],
            local_forward: vec!["8080 localhost:80".into()],
            remote_forward: vec!["9000 localhost:9000".into()],
            ciphers: Some("^aes256-ctr".into()),
            ..Options::default()
        };
        let hosts = vec![host, Host::new("plain", "plain.example")];
        let text = toml::to_string_pretty(&HostsFile { hosts: hosts.clone() }).unwrap();
        assert!(text.contains("[[host.login]]"), "{text}");
        assert!(text.contains("address_family = \"inet6\"") && text.contains("strict_host_key_checking = \"accept-new\""), "{text}");
        // Defaults aren't written.
        assert_eq!(text.matches("compression").count(), 1, "{text}");
        assert_eq!(toml::from_str::<HostsFile>(&text).unwrap().hosts, hosts);
    }

    #[test]
    fn logins_pick_user_and_key() {
        let mut host = Host::new("web", "web.example");
        host.user = "jan".into();
        host.key = Some("work".into());
        host.logins = vec![Login { user: "root".into(), key: Some("admin".into()) }];
        assert_eq!(host.all_logins().len(), 2);
        let root = host.with_login(1).unwrap();
        assert_eq!((root.user.as_str(), root.key.as_deref()), ("root", Some("admin")));
        assert!(root.logins.is_empty());
        assert!(host.with_login(2).is_none());
        assert_eq!(host.login_index("root"), Some(1));
        assert!(host.uses_key("admin") && host.uses_key("work") && !host.uses_key("other"));
        host.rename_key("admin", "root-key");
        assert_eq!(host.logins[0].key.as_deref(), Some("root-key"));
        assert_eq!(host.key.as_deref(), Some("work"));
    }

    #[test]
    fn reads_connection_options_from_ssh_config() {
        let hosts = parse(
            "Host a\n  ProxyCommand nc %h %p\n  ProxyJump ignored\n  Compression yes\n  ForwardAgent yes\n  ServerAliveInterval 15\n  \
             StrictHostKeyChecking no\n  UserKnownHostsFile ~/.ssh/kh_%n /dev/null\n  SetEnv FOO=1 \"BAR=a b\"\n  \
             SetEnv FOO=2\n  SendEnv LANG LC_*\n  LocalForward 8080 localhost:80\n  LocalForward=\"8443 localhost:443\"\n  \
             RemoteForward 9000 localhost:9000\n  DynamicForward 1080\n  Ciphers ^aes256-ctr\n  AddressFamily inet\n  RemoteCommand tmux attach -t %n\n\
             Host b\n  ProxyJump a\n  ProxyCommand ignored\n  IdentityFile \"~/.ssh/my key\"\n  ForwardAgent no\n  ForwardAgent yes\n",
        );
        let a = &hosts[0];
        assert!(a.proxy_jump.is_none());
        let o = &a.options;
        assert_eq!(o.proxy_command.as_deref(), Some("nc %h %p"));
        assert!(o.compression);
        assert_eq!((o.server_alive_interval, o.address_family), (Some(15), AddressFamily::Inet));
        assert_eq!(o.strict_host_key_checking, HostKeyCheck::AcceptNew);
        assert_eq!(o.user_known_hosts_file.as_deref(), Some("~/.ssh/kh_a"));
        assert_eq!(o.set_env, ["FOO=1", "BAR=a b"]);
        assert_eq!(o.send_env, ["LANG", "LC_*"]);
        assert_eq!(o.local_forward, ["8080 localhost:80", "8443 localhost:443"]);
        assert_eq!(o.remote_forward, ["9000 localhost:9000"]);
        assert_eq!(o.dynamic_forward, ["1080"]);
        assert_eq!(o.ciphers.as_deref(), Some("^aes256-ctr"));
        assert_eq!(o.forward_agent.as_deref(), Some("yes"));

        let b = &hosts[1];
        assert_eq!(b.proxy_jump.as_deref(), Some("a"));
        assert!(b.options.proxy_command.is_none());
        assert_eq!(b.identities, ["~/.ssh/my key"]);
        // The first value wins, a `no` as well.
        assert!(b.options.forward_agent.is_none());

        // Tokens in ProxyCommand/RemoteCommand are expanded per connection;
        // a jump host keeps its ProxyCommand but loses what only the
        // target does (forwards, agent, command, environment).
        let catalog = Catalog { config: hosts.clone(), ..Catalog::default() };
        let target = catalog.target(a).unwrap();
        assert_eq!(target.settings.proxy_command.as_deref(), Some("nc a 22"));
        assert_eq!(target.settings.remote_command.as_deref(), Some("tmux attach -t a"));
        assert_eq!(target.settings.forward_agent, options::ForwardAgent::Login);
        let via = catalog.target(b).unwrap();
        let hop = &via.jumps[0].settings;
        assert!(hop.proxy_command.is_some() && hop.forwards.is_empty() && hop.remote_command.is_none());
        assert_eq!(hop.forward_agent, options::ForwardAgent::Off);

        let mut both = Host::new("both", "both.example");
        both.proxy_jump = Some("a".into());
        both.options.proxy_command = Some("nc %h %p".into());
        assert!(catalog.target(&both).unwrap_err().contains("ProxyCommand"));
    }

    #[test]
    fn splits_words_with_quotes() {
        assert_eq!(words("a  \"b c\" d\"e f\""), ["a", "b c", "de f"]);
        assert!(words("   ").is_empty());
        assert_eq!(unquote("\"x y\""), "x y");
    }
}
