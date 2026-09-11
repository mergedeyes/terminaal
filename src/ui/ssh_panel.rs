//! The sidebar's SSH section: hosts saved in Terminaal (add, edit,
//! delete -- stored via `ssh::save_hosts`) and the hosts from
//! `~/.ssh/config` (read-only, but can be copied into the saved list).
//! Connecting opens a tab ([`SidebarAction::Connect`]) with a host's
//! default login or any of its others.
//!
//! The host form shows what most hosts need -- address, logins, jump
//! host. Port forwards and all other options fold out below it; a form
//! opens those fold-outs that have something set.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Align, CollapsingHeader, ComboBox, CornerRadius, Frame, Layout, Margin, RichText, TextEdit, Ui};

use crate::i18n::t;
use crate::ssh::options::{AddressFamily, Forward, ForwardKind, HostKeyCheck, Options, Target};
use crate::ssh::{self, Catalog, Host, Login, keys};
use crate::ui::sidebar::SidebarAction;
use crate::ui::theme;
use crate::ui::widgets::{Status, list_row, section_title, weak};

/// Hosts and keys, shared by the SSH and key sections -- renaming a key
/// has to reach the hosts that use it.
pub struct SshData {
    pub catalog: Catalog,
    /// Set if hosts.toml or keys.toml couldn't be read. Saving is refused
    /// then, so a broken file never gets overwritten with an empty list.
    pub hosts_error: Option<String>,
    pub keys_error: Option<String>,
}

impl SshData {
    pub fn load() -> Self {
        let (saved, hosts_error) = split(ssh::load_hosts());
        let (keys, keys_error) = split(keys::load_keys());
        Self { catalog: Catalog { saved, config: ssh::ssh_config_hosts(), keys }, hosts_error, keys_error }
    }

    pub fn reload(&mut self) {
        *self = Self::load();
    }

    /// Apply `change` to a copy of the saved hosts and write that. Only
    /// once it's on disk does the change reach `self.catalog`, so a failed
    /// save never leaves the list showing something that isn't saved.
    pub fn update_hosts(&mut self, change: impl FnOnce(&mut Vec<Host>)) -> Result<(), String> {
        if self.hosts_error.is_some() {
            return Err(t!("ssh-store-unreadable", file = "hosts.toml"));
        }
        let mut hosts = self.catalog.saved.clone();
        change(&mut hosts);
        ssh::save_hosts(&hosts)?;
        self.catalog.saved = hosts;
        Ok(())
    }

    /// Same as [`SshData::update_hosts`], for the key store.
    pub fn update_keys(&mut self, change: impl FnOnce(&mut Vec<keys::Key>)) -> Result<(), String> {
        if self.keys_error.is_some() {
            return Err(t!("ssh-store-unreadable", file = "keys.toml"));
        }
        let mut keys = self.catalog.keys.clone();
        change(&mut keys);
        keys::save_keys(&keys)?;
        self.catalog.keys = keys;
        Ok(())
    }
}

fn split<T: Default>(result: Result<T, String>) -> (T, Option<String>) {
    match result {
        Ok(value) => (value, None),
        Err(err) => (T::default(), Some(err)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    Saved(usize),
    Config(usize),
}

pub struct SshPanel {
    selected: Option<Selection>,
    editor: Option<HostEditor>,
    /// Saved host whose delete button was clicked once.
    confirm_delete: Option<usize>,
    status: Option<Status>,
}

/// What a forward row in the form is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RowKind {
    #[default]
    Local,
    Remote,
    /// `DynamicForward`: a SOCKS proxy here.
    Dynamic,
    /// `RemoteForward` without a target: a SOCKS proxy on the server.
    RemoteDynamic,
}

impl RowKind {
    const ALL: [RowKind; 4] = [RowKind::Local, RowKind::Remote, RowKind::Dynamic, RowKind::RemoteDynamic];

    fn of(forward: &Forward) -> Self {
        match (forward.kind(), &forward.target) {
            (ForwardKind::Local, _) => Self::Local,
            (ForwardKind::Dynamic, _) => Self::Dynamic,
            (ForwardKind::Remote, Some(_)) => Self::Remote,
            (ForwardKind::Remote, None) => Self::RemoteDynamic,
        }
    }

    /// The keyword it's saved under.
    fn forward_kind(self) -> ForwardKind {
        match self {
            Self::Local => ForwardKind::Local,
            Self::Remote | Self::RemoteDynamic => ForwardKind::Remote,
            Self::Dynamic => ForwardKind::Dynamic,
        }
    }

    fn has_target(self) -> bool {
        matches!(self, Self::Local | Self::Remote)
    }

    fn label(self) -> String {
        match self {
            Self::Local => t!("host-forward-local"),
            Self::Remote => t!("host-forward-remote"),
            Self::Dynamic => t!("host-forward-dynamic"),
            Self::RemoteDynamic => t!("host-forward-remote-dynamic"),
        }
    }

    fn hint(self) -> String {
        match self {
            Self::Local => t!("host-forward-local-hint"),
            Self::Remote => t!("host-forward-remote-hint"),
            Self::Dynamic => t!("host-forward-dynamic-hint"),
            Self::RemoteDynamic => t!("host-forward-remote-dynamic-hint"),
        }
    }

    fn listen_hint(self) -> String {
        match self {
            Self::Local => t!("host-local-listen-hint"),
            Self::Remote => t!("host-remote-listen-hint"),
            Self::Dynamic => t!("host-dynamic-listen-hint"),
            Self::RemoteDynamic => t!("host-remote-dynamic-listen-hint"),
        }
    }
}

/// A port forward as typed into the form.
#[derive(Clone, Default)]
struct ForwardRow {
    kind: RowKind,
    /// `[bind:]port` or a socket path.
    listen: String,
    /// `host:port` or a socket path; unused without a target.
    target: String,
}

impl ForwardRow {
    fn from_spec(kind: ForwardKind, spec: &str) -> Self {
        match Forward::parse(spec, kind) {
            Ok(forward) => Self {
                kind: RowKind::of(&forward),
                listen: forward.listen_spec(),
                target: forward.target.as_ref().map(Target::spec).unwrap_or_default(),
            },
            // Keep what's there, so saving reports what's wrong with it.
            Err(_) => {
                let spec = spec.trim();
                let (listen, target) = spec.split_once(char::is_whitespace).unwrap_or((spec, ""));
                let kind = match kind {
                    ForwardKind::Local => RowKind::Local,
                    ForwardKind::Dynamic => RowKind::Dynamic,
                    ForwardKind::Remote if target.trim().is_empty() => RowKind::RemoteDynamic,
                    ForwardKind::Remote => RowKind::Remote,
                };
                Self { kind, listen: listen.to_string(), target: target.trim().to_string() }
            }
        }
    }

    /// The target as it counts: none for the SOCKS kinds.
    fn target(&self) -> &str {
        if self.kind.has_target() { self.target.trim() } else { "" }
    }

    fn is_blank(&self) -> bool {
        self.listen.trim().is_empty() && self.target().is_empty()
    }
}

/// The folded-out options as typed; checked on saving.
#[derive(Clone, Default)]
struct Advanced {
    connect_timeout: String,
    alive_interval: String,
    alive_count_max: String,
    compression: bool,
    address_family: AddressFamily,
    proxy_command: String,
    /// `IdentityFile`s, one per line.
    identities: String,
    identities_only: bool,
    identity_agent: String,
    preferred_auth: String,
    host_key_check: HostKeyCheck,
    known_hosts: String,
    remote_command: String,
    /// `NAME=value`, one per line.
    set_env: String,
    /// Space-separated.
    send_env: String,
    kex: String,
    host_key_algorithms: String,
    ciphers: String,
    macs: String,
}

impl Advanced {
    fn from_host(host: &Host) -> Self {
        let options = &host.options;
        let number = |value: Option<u32>| value.map(|n| n.to_string()).unwrap_or_default();
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        Self {
            connect_timeout: number(options.connect_timeout),
            alive_interval: number(options.server_alive_interval),
            alive_count_max: number(options.server_alive_count_max),
            compression: options.compression,
            address_family: options.address_family,
            proxy_command: text(&options.proxy_command),
            identities: host.identities.join("\n"),
            identities_only: host.identities_only,
            identity_agent: text(&host.identity_agent),
            preferred_auth: text(&options.preferred_authentications),
            host_key_check: options.strict_host_key_checking,
            known_hosts: text(&options.user_known_hosts_file),
            remote_command: text(&options.remote_command),
            set_env: options.set_env.join("\n"),
            send_env: options.send_env.join(" "),
            kex: text(&options.kex_algorithms),
            host_key_algorithms: text(&options.host_key_algorithms),
            ciphers: text(&options.ciphers),
            macs: text(&options.macs),
        }
    }

    /// How many options are set, for the fold-out's title.
    fn count_set(&self) -> usize {
        let texts = [
            &self.connect_timeout,
            &self.alive_interval,
            &self.alive_count_max,
            &self.proxy_command,
            &self.identities,
            &self.identity_agent,
            &self.preferred_auth,
            &self.known_hosts,
            &self.remote_command,
            &self.set_env,
            &self.send_env,
            &self.kex,
            &self.host_key_algorithms,
            &self.ciphers,
            &self.macs,
        ];
        texts.iter().filter(|text| !text.trim().is_empty()).count()
            + usize::from(self.compression)
            + usize::from(self.identities_only)
            + usize::from(self.address_family != AddressFamily::Any)
            + usize::from(self.host_key_check != HostKeyCheck::Ask)
    }
}

struct HostEditor {
    /// Tells this form's fold-outs apart from earlier forms', so each
    /// form opens those that have something set.
    id: u64,
    /// Index in the saved list while editing; `None` for a new host.
    original: Option<usize>,
    name: String,
    host: String,
    port: String,
    /// The default login first; never empty.
    logins: Vec<Login>,
    /// `ProxyJump`; empty for none.
    jump: String,
    forwards: Vec<ForwardRow>,
    advanced: Advanced,
    error: Option<String>,
    focus_pending: bool,
}

impl HostEditor {
    fn new() -> Self {
        Self::from_host(&Host::new("", ""), None)
    }

    fn from_host(host: &Host, original: Option<usize>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let options = &host.options;
        let forwards = [
            (ForwardKind::Local, &options.local_forward),
            (ForwardKind::Remote, &options.remote_forward),
            (ForwardKind::Dynamic, &options.dynamic_forward),
        ]
        .into_iter()
        .flat_map(|(kind, specs)| specs.iter().map(move |spec| ForwardRow::from_spec(kind, spec)))
        .collect();
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            original,
            name: host.name.clone(),
            host: host.host.clone(),
            port: host.port.to_string(),
            logins: host.all_logins(),
            jump: host.jump().unwrap_or_default().to_string(),
            forwards,
            advanced: Advanced::from_host(host),
            error: None,
            focus_pending: true,
        }
    }

    /// The host the form describes, checked as far as that's possible
    /// without the rest of the catalog.
    fn to_host(&self, data: &SshData) -> Result<Host, String> {
        let host = self.host.trim();
        if host.is_empty() {
            return Err(t!("host-missing"));
        }
        if host.contains(char::is_whitespace) {
            return Err(t!("host-has-spaces"));
        }
        let port = self.port.trim().parse::<u16>().ok().filter(|&p| p > 0).ok_or_else(|| t!("host-bad-port"))?;
        let name = match self.name.trim() {
            "" => host.to_string(),
            name => name.to_string(),
        };
        if data.catalog.saved.iter().enumerate().any(|(i, h)| h.name == name && Some(i) != self.original) {
            return Err(t!("host-name-taken", name = &name));
        }

        let mut logins: Vec<Login> = Vec::new();
        for login in &self.logins {
            let user = login.user.trim();
            if user.contains(char::is_whitespace) || user.contains('@') {
                return Err(t!("host-bad-user"));
            }
            let login = Login { user: user.to_string(), key: login.key.clone() };
            if logins.contains(&login) {
                return Err(t!("host-login-twice", login = login.label()));
            }
            logins.push(login);
        }
        let default = if logins.is_empty() { Login::default() } else { logins.remove(0) };

        let advanced = &self.advanced;
        let jump = self.jump.trim();
        if jump == name {
            return Err(t!("host-jumps-itself"));
        }
        if !jump.is_empty() && !advanced.proxy_command.trim().is_empty() {
            return Err(t!("host-jump-and-proxy"));
        }

        let (mut local_forward, mut remote_forward, mut dynamic_forward) = (Vec::new(), Vec::new(), Vec::new());
        for (i, row) in self.forwards.iter().enumerate().filter(|(_, row)| !row.is_blank()) {
            let (listen, target) = (row.listen.trim(), row.target());
            if listen.is_empty() {
                return Err(t!("host-forward-missing-port", row = i + 1));
            }
            if row.kind.has_target() && target.is_empty() {
                return Err(t!("host-forward-missing-target", row = i + 1));
            }
            let spec = if target.is_empty() { listen.to_string() } else { format!("{listen} {target}") };
            let kind = row.kind.forward_kind();
            // Checked here already, so the message names the row.
            Forward::parse(&spec, kind).map_err(|err| t!("host-forward-invalid", row = i + 1, err = err))?;
            match kind {
                ForwardKind::Local => &mut local_forward,
                ForwardKind::Remote => &mut remote_forward,
                ForwardKind::Dynamic => &mut dynamic_forward,
            }
            .push(spec);
        }

        let options = Options {
            connect_timeout: number(&advanced.connect_timeout, &t!("adv-connect-timeout-name"))?,
            server_alive_interval: number(&advanced.alive_interval, &t!("adv-alive-interval-name"))?,
            server_alive_count_max: number(&advanced.alive_count_max, &t!("adv-alive-count-name"))?,
            compression: advanced.compression,
            address_family: advanced.address_family,
            proxy_command: text(&advanced.proxy_command),
            preferred_authentications: text(&advanced.preferred_auth),
            strict_host_key_checking: advanced.host_key_check,
            user_known_hosts_file: text(&advanced.known_hosts),
            remote_command: text(&advanced.remote_command),
            set_env: lines(&advanced.set_env),
            send_env: advanced.send_env.split_whitespace().map(str::to_string).collect(),
            local_forward,
            remote_forward,
            dynamic_forward,
            kex_algorithms: text(&advanced.kex),
            host_key_algorithms: text(&advanced.host_key_algorithms),
            ciphers: text(&advanced.ciphers),
            macs: text(&advanced.macs),
        };
        Ok(Host {
            name,
            host: host.to_string(),
            port,
            user: default.user,
            key: default.key,
            identities: lines(&advanced.identities),
            identities_only: advanced.identities_only,
            identity_agent: text(&advanced.identity_agent),
            proxy_jump: Some(jump.to_string()).filter(|j| !j.is_empty()),
            options,
            logins,
        })
    }
}

/// Empty: unset.
fn number(value: &str, label: &str) -> Result<Option<u32>, String> {
    match value.trim() {
        "" => Ok(None),
        value => value.parse().map(Some).map_err(|_| t!("host-not-a-number", label = label, value = value)),
    }
}

fn text(value: &str) -> Option<String> {
    Some(value.trim().to_string()).filter(|v| !v.is_empty())
}

fn lines(value: &str) -> Vec<String> {
    value.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect()
}

/// Address, plus the jump host if there is one.
fn subtitle(host: &Host) -> String {
    match host.jump() {
        Some(jump) => t!("ssh-via", address = host.address(), jump = jump),
        None => host.address(),
    }
}

/// The default login's key, and how many more logins there are.
fn badge(host: &Host) -> Option<String> {
    match (&host.key, host.logins.len()) {
        (Some(key), 0) => Some(key.clone()),
        (Some(key), more) => Some(format!("{key} +{more}")),
        (None, 0) => None,
        (None, more) => Some(t!("ssh-more-logins", count = more)),
    }
}

impl SshPanel {
    pub fn new(data: &SshData) -> Self {
        let selected = if !data.catalog.saved.is_empty() {
            Some(Selection::Saved(0))
        } else if !data.catalog.config.is_empty() {
            Some(Selection::Config(0))
        } else {
            None
        };
        Self { selected, editor: None, confirm_delete: None, status: None }
    }

    pub fn report(&mut self, result: Result<String, String>) {
        self.status = Some(Status::from_result(result));
    }

    /// Open a tab to `host`, logged in with its login number `login`.
    fn connect(&mut self, data: &SshData, host: &Host, login: usize, actions: &mut Vec<SidebarAction>) {
        let Some(host) = host.with_login(login) else { return };
        match data.catalog.target(&host) {
            Ok(target) => actions.push(SidebarAction::Connect(Box::new(target))),
            Err(err) => self.report(Err(err)),
        }
    }

    pub fn show(&mut self, ui: &mut Ui, data: &mut SshData, actions: &mut Vec<SidebarAction>) {
        let mut clicked = None;

        section_title(ui, &t!("ssh-saved-hosts"));
        if let Some(err) = data.hosts_error.clone() {
            ui.label(RichText::new(err).color(theme::ERROR));
            if ui.button(t!("common-reload")).clicked() {
                data.reload();
            }
        }
        let saved = data.catalog.saved.clone();
        for (i, host) in saved.iter().enumerate() {
            let selected = self.selected == Some(Selection::Saved(i));
            let badge = badge(host);
            let row = list_row(ui, &host.name, &subtitle(host), selected, badge.as_deref(), &t!("ssh-double-click-connects"));
            if row.double_clicked() {
                self.connect(data, host, 0, actions);
            }
            if row.clicked() {
                clicked = Some(Selection::Saved(i));
            }
        }
        if saved.is_empty() && data.hosts_error.is_none() && self.editor.is_none() {
            ui.label(weak(t!("ssh-no-hosts")));
        }
        if let Some(Selection::Saved(i)) = self.selected
            && let Some(host) = saved.get(i)
            && self.editor.is_none()
        {
            self.saved_host_buttons(ui, data, i, host, actions);
        }

        ui.add_space(4.0);
        if self.editor.is_some() {
            self.editor_form(ui, data);
        } else if data.hosts_error.is_none() && ui.button(t!("ssh-add-host")).clicked() {
            self.editor = Some(HostEditor::new());
            self.confirm_delete = None;
            self.status = None;
        }

        let mut adopt = None;
        let config = data.catalog.config.clone();
        if !config.is_empty() {
            ui.add_space(18.0);
            section_title(ui, &t!("ssh-from-config"));
            for (i, host) in config.iter().enumerate() {
                let selected = self.selected == Some(Selection::Config(i));
                let row = list_row(ui, &host.name, &subtitle(host), selected, None, &t!("ssh-double-click-connects"));
                if row.double_clicked() {
                    self.connect(data, host, 0, actions);
                }
                if row.clicked() {
                    clicked = Some(Selection::Config(i));
                }
            }
            if let Some(Selection::Config(i)) = self.selected
                && let Some(host) = config.get(i)
            {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button(t!("ssh-connect")).clicked() {
                        self.connect(data, host, 0, actions);
                    }
                    let enabled = data.hosts_error.is_none() && self.editor.is_none();
                    let copy = ui.add_enabled(enabled, egui::Button::new(t!("ssh-adopt")));
                    if copy.on_hover_text(t!("ssh-adopt-hint")).clicked() {
                        adopt = Some(host.clone());
                    }
                });
            }
        }

        if let Some(selection) = clicked
            && self.selected != Some(selection)
        {
            self.selected = Some(selection);
            self.confirm_delete = None;
            self.status = None;
        }
        if let Some(host) = adopt {
            self.editor = Some(HostEditor::from_host(&host, None));
            self.status = None;
        }

        if let Some(status) = &self.status {
            ui.add_space(8.0);
            status.show(ui);
        }
        ui.add_space(14.0);
        ui.label(weak(t!("ssh-secrets-note")).size(11.0));
    }

    fn saved_host_buttons(
        &mut self,
        ui: &mut Ui,
        data: &mut SshData,
        index: usize,
        host: &Host,
        actions: &mut Vec<SidebarAction>,
    ) {
        let confirming = self.confirm_delete == Some(index);
        let logins = host.all_logins();
        let mut connect = None;
        let (mut edit, mut ask_delete, mut delete, mut cancel_delete) = (false, false, false, false);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if logins.len() == 1 {
                if ui.button(t!("ssh-connect")).clicked() {
                    connect = Some(0);
                }
            } else {
                // One button per login; the first is the default one.
                for (i, login) in logins.iter().enumerate() {
                    let user = if login.user.is_empty() { ssh::local_user() } else { login.user.clone() };
                    let button = ui.button(format!("▶  {user}")).on_hover_text(t!("ssh-connect-as", login = login.label()));
                    if button.clicked() {
                        connect = Some(i);
                    }
                }
            }
            if confirming {
                delete = ui.button(RichText::new(t!("common-delete")).color(theme::ERROR)).clicked();
                cancel_delete = ui.button(t!("common-no")).clicked();
            } else {
                edit = ui.button("✏").on_hover_text(t!("common-edit")).clicked();
                ask_delete = ui.button("🗑").on_hover_text(t!("common-delete")).clicked();
            }
        });

        if let Some(login) = connect {
            self.connect(data, host, login, actions);
        }
        if edit {
            self.editor = Some(HostEditor::from_host(host, Some(index)));
            self.status = None;
        }
        if ask_delete {
            self.confirm_delete = Some(index);
        }
        if cancel_delete {
            self.confirm_delete = None;
        }
        if delete {
            self.confirm_delete = None;
            let jumping: Vec<&str> = data
                .catalog
                .saved
                .iter()
                .filter(|h| h.jump().is_some_and(|j| j.split(',').any(|hop| hop.trim() == host.name)))
                .map(|h| h.name.as_str())
                .collect();
            if !jumping.is_empty() {
                let message = t!("ssh-is-jump-host", name = &host.name, hosts = jumping.join(", "));
                self.report(Err(message));
                return;
            }
            let result = data
                .update_hosts(|saved| {
                    saved.remove(index);
                })
                .map(|()| t!("common-deleted", name = &host.name));
            if result.is_ok() {
                self.selected = None;
            }
            self.report(result);
        }
    }

    fn editor_form(&mut self, ui: &mut Ui, data: &mut SshData) {
        let key_names: Vec<String> = data.catalog.keys.iter().map(|k| k.name.clone()).collect();
        let mut host_names: Vec<String> = Vec::new();
        for host in data.catalog.saved.iter().chain(&data.catalog.config) {
            if !host_names.contains(&host.name) {
                host_names.push(host.name.clone());
            }
        }

        let Some(editor) = self.editor.as_mut() else { return };
        let (mut save, mut cancel) = (false, false);
        Frame::group(ui.style()).fill(theme::ROW_BG).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(if editor.original.is_some() { t!("host-edit") } else { t!("host-new") }).strong());
            ui.add_space(2.0);

            let host = text_field(ui, "Host", &mut editor.host, &t!("host-host-hint"));
            if editor.focus_pending {
                host.request_focus();
                editor.focus_pending = false;
            }
            text_field(ui, &t!("host-name-optional"), &mut editor.name, &t!("host-name-hint"));
            text_field(ui, "Port", &mut editor.port, "22");

            ui.add_space(4.0);
            logins_ui(ui, editor, &key_names);

            ui.add_space(4.0);
            ui.label(weak(t!("host-jump")));
            let own_name = if editor.name.trim().is_empty() { editor.host.trim() } else { editor.name.trim() }.to_string();
            let none = t!("host-jump-none");
            let selected = if editor.jump.is_empty() { none.clone() } else { editor.jump.clone() };
            ComboBox::from_id_salt(("ssh-host-jump", editor.id))
                .width(ui.available_width())
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut editor.jump, String::new(), none);
                    if !editor.jump.is_empty() && !host_names.contains(&editor.jump) {
                        let current = editor.jump.clone();
                        ui.selectable_value(&mut editor.jump, current.clone(), current);
                    }
                    for name in host_names.iter().filter(|n| **n != own_name) {
                        ui.selectable_value(&mut editor.jump, name.clone(), name);
                    }
                });

            ui.add_space(4.0);
            forwards_ui(ui, editor);
            advanced_ui(ui, editor);

            if let Some(err) = &editor.error {
                ui.label(RichText::new(err).color(theme::ERROR));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                save = ui.button(t!("common-save")).clicked();
                cancel = ui.button(t!("common-cancel")).clicked();
            });
        });

        if cancel {
            self.editor = None;
        } else if save {
            match self.commit(data) {
                Ok(message) => {
                    self.editor = None;
                    self.report(Ok(message));
                }
                Err(err) => {
                    if let Some(editor) = self.editor.as_mut() {
                        editor.error = Some(err);
                    }
                }
            }
        }
    }

    /// Validate the editor's contents and save them to `hosts.toml`.
    fn commit(&mut self, data: &mut SshData) -> Result<String, String> {
        let editor = self.editor.as_ref().ok_or_else(|| t!("host-no-host-open"))?;
        let entry = editor.to_host(data)?;

        let mut hosts = data.catalog.saved.clone();
        let index = match editor.original {
            Some(i) if i < hosts.len() => {
                hosts[i] = entry.clone();
                i
            }
            _ => {
                hosts.push(entry.clone());
                hosts.len() - 1
            }
        };
        // Catch unknown keys, bad jump specs, jump loops and invalid options
        // -- for every login -- before saving.
        let catalog = Catalog { saved: hosts.clone(), ..data.catalog.clone() };
        for login in 0..entry.all_logins().len() {
            catalog.target(&entry.with_login(login).expect("login exists"))?;
        }

        let name = entry.name.clone();
        data.update_hosts(|saved| *saved = hosts)?;
        self.selected = Some(Selection::Saved(index));
        Ok(t!("common-saved", name = &name))
    }
}

/// The host's logins, each a user name and a key; the first is the
/// default (double click, jump host use).
fn logins_ui(ui: &mut Ui, editor: &mut HostEditor, key_names: &[String]) {
    ui.label(weak(t!("host-login")));
    // "Automatic" means the host's `IdentityFile`s if it has any.
    let auto = if editor.advanced.identities.trim().is_empty() { t!("ssh-auto-key") } else { t!("ssh-files-key") };
    let (id, count) = (editor.id, editor.logins.len());
    let (mut remove, mut promote) = (None, None);
    for (i, login) in editor.logins.iter_mut().enumerate() {
        Frame::new().fill(theme::BG).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if count > 1 {
                ui.horizontal(|ui| {
                    ui.label(weak(if i == 0 { t!("common-default") } else { t!("host-login-more", index = i) }).size(11.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("🗑").on_hover_text(t!("host-remove-login")).clicked() {
                            remove = Some(i);
                        }
                        let make_default = t!("host-make-default-login");
                        if i > 0 && ui.small_button("★").on_hover_text(make_default).clicked() {
                            promote = Some(i);
                        }
                    });
                });
            }
            ui.add(
                TextEdit::singleline(&mut login.user)
                    .desired_width(f32::INFINITY)
                    .hint_text(t!("host-user-hint", user = ssh::local_user())),
            );
            let selected = login.key.clone().unwrap_or_else(|| auto.clone());
            ComboBox::from_id_salt(("ssh-login-key", id, i)).width(ui.available_width()).selected_text(selected).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(&mut login.key, None, auto.as_str());
                    for name in key_names {
                        ui.selectable_value(&mut login.key, Some(name.clone()), name);
                    }
                },
            );
        });
        ui.add_space(2.0);
    }
    if let Some(i) = remove {
        editor.logins.remove(i);
    }
    if let Some(i) = promote {
        let login = editor.logins.remove(i);
        editor.logins.insert(0, login);
    }
    let more = ui.button(t!("host-add-login")).on_hover_text(t!("host-add-login-hint"));
    if more.clicked() {
        editor.logins.push(Login::default());
    }
    if key_names.is_empty() {
        ui.label(weak(t!("host-keys-hint")).size(11.0));
    }
}

fn forwards_ui(ui: &mut Ui, editor: &mut HostEditor) {
    let count = editor.forwards.iter().filter(|row| !row.is_blank()).count();
    let title = t!("host-forwards", count = count);
    let id = editor.id;
    CollapsingHeader::new(title).id_salt(("ssh-forwards", id)).default_open(count > 0).show(ui, |ui| {
        let mut remove = None;
        for (i, row) in editor.forwards.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ComboBox::from_id_salt(("ssh-forward-kind", id, i))
                    .width(140.0)
                    .selected_text(row.kind.label())
                    .show_ui(ui, |ui| {
                        for kind in RowKind::ALL {
                            ui.selectable_value(&mut row.kind, kind, kind.label()).on_hover_text(kind.hint());
                        }
                    });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("🗑").on_hover_text(t!("host-remove-forward")).clicked() {
                        remove = Some(i);
                    }
                });
            });
            ui.add(TextEdit::singleline(&mut row.listen).desired_width(f32::INFINITY).hint_text(row.kind.listen_hint()));
            if row.kind.has_target() {
                let target_hint =
                    if row.kind == RowKind::Remote { t!("host-remote-target-hint") } else { t!("host-local-target-hint") };
                ui.add(TextEdit::singleline(&mut row.target).desired_width(f32::INFINITY).hint_text(target_hint));
            }
            ui.add_space(4.0);
        }
        if let Some(i) = remove {
            editor.forwards.remove(i);
        }
        if ui.button(t!("host-add-forward")).clicked() {
            editor.forwards.push(ForwardRow::default());
        }
        ui.label(weak(t!("host-forwards-note")).size(11.0));
    });
}

/// Everything else, grouped; the ssh_config keyword next to each label.
fn advanced_ui(ui: &mut Ui, editor: &mut HostEditor) {
    let count = editor.advanced.count_set();
    let title = t!("host-advanced", count = count);
    let id = editor.id;
    let a = &mut editor.advanced;
    let default = t!("common-default");
    CollapsingHeader::new(title).id_salt(("ssh-advanced", id)).default_open(count > 0).show(ui, |ui| {
        group(ui, &t!("adv-connection"));
        option_field(ui, &t!("adv-connect-timeout"), "ConnectTimeout", &mut a.connect_timeout, "10");
        option_field(ui, &t!("adv-alive-interval"), "ServerAliveInterval", &mut a.alive_interval, "30");
        option_field(ui, &t!("adv-alive-count"), "ServerAliveCountMax", &mut a.alive_count_max, "3");
        ui.checkbox(&mut a.compression, t!("adv-compression")).on_hover_text(t!("adv-compression-hint"));
        option_label(ui, &t!("adv-address-family"), "AddressFamily");
        ComboBox::from_id_salt(("ssh-family", id)).width(ui.available_width()).selected_text(a.address_family.label()).show_ui(
            ui,
            |ui| {
                for family in AddressFamily::ALL {
                    ui.selectable_value(&mut a.address_family, family, family.label());
                }
            },
        );
        option_field(ui, &t!("adv-proxy-command"), "ProxyCommand", &mut a.proxy_command, &t!("adv-proxy-command-hint"));

        group(ui, &t!("adv-auth"));
        option_label(ui, &t!("adv-identity-files"), "IdentityFile");
        ui.add(TextEdit::multiline(&mut a.identities).desired_rows(2).desired_width(f32::INFINITY).hint_text("~/.ssh/id_ed25519"));
        ui.label(weak(t!("adv-identity-files-note")).size(11.0));
        ui.checkbox(&mut a.identities_only, t!("adv-identities-only")).on_hover_text(t!("adv-identities-only-hint"));
        option_field(ui, &t!("adv-agent-socket"), "IdentityAgent", &mut a.identity_agent, &t!("adv-agent-socket-hint"));
        option_field(
            ui,
            &t!("adv-methods"),
            "PreferredAuthentications",
            &mut a.preferred_auth,
            "publickey,keyboard-interactive,password",
        );

        group(ui, &t!("adv-host-key"));
        option_label(ui, &t!("adv-unknown-host-keys"), "StrictHostKeyChecking");
        ComboBox::from_id_salt(("ssh-host-key-check", id))
            .width(ui.available_width())
            .selected_text(a.host_key_check.label())
            .show_ui(ui, |ui| {
                for check in HostKeyCheck::ALL {
                    ui.selectable_value(&mut a.host_key_check, check, check.label());
                }
            });
        ui.label(weak(t!("adv-changed-host-key-note")).size(11.0));
        option_field(ui, &t!("adv-known-hosts-file"), "UserKnownHostsFile", &mut a.known_hosts, "~/.ssh/known_hosts");

        group(ui, &t!("adv-session"));
        option_field(ui, &t!("adv-remote-command"), "RemoteCommand", &mut a.remote_command, &t!("adv-remote-command-hint"));
        option_label(ui, &t!("adv-set-env"), "SetEnv");
        ui.add(
            TextEdit::multiline(&mut a.set_env).desired_rows(2).desired_width(f32::INFINITY).hint_text(t!("adv-set-env-hint")),
        );
        option_field(ui, &t!("adv-send-env"), "SendEnv", &mut a.send_env, "LANG LC_*");
        ui.label(weak(t!("adv-env-note")).size(11.0));

        group(ui, &t!("adv-algorithms"));
        option_field(ui, &t!("adv-kex"), "KexAlgorithms", &mut a.kex, &default);
        option_field(ui, &t!("adv-host-key-types"), "HostKeyAlgorithms", &mut a.host_key_algorithms, &default);
        option_field(ui, &t!("adv-ciphers"), "Ciphers", &mut a.ciphers, &default);
        option_field(ui, &t!("adv-macs"), "MACs", &mut a.macs, &default);
        ui.label(weak(t!("adv-algorithms-note")).size(11.0));
    });
}

fn group(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong().size(12.0));
}

/// A label with the matching ssh_config keyword next to it.
fn option_label(ui: &mut Ui, label: &str, keyword: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(weak(label));
        ui.label(RichText::new(keyword).monospace().size(10.0).color(theme::TEXT_WEAK));
    });
}

fn option_field(ui: &mut Ui, label: &str, keyword: &str, value: &mut String, hint: &str) -> egui::Response {
    option_label(ui, label, keyword);
    ui.add(TextEdit::singleline(value).desired_width(f32::INFINITY).hint_text(hint))
}

fn text_field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> egui::Response {
    ui.label(weak(label));
    ui.add(TextEdit::singleline(value).desired_width(f32::INFINITY).hint_text(hint))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saving is refused (`hosts_error` set), so nothing here can touch
    /// the real ~/.config/terminaal even by accident.
    fn data() -> SshData {
        let mut config_host = Host::new("cfg", "cfg.example");
        config_host.identities = vec!["~/.ssh/id_cfg".into()];
        config_host.identities_only = true;
        config_host.options.local_forward = vec!["8080 localhost:80".into()];
        config_host.options.compression = true;
        let mut saved = Host::new("a", "a.example");
        saved.key = Some("work".into());
        saved.logins = vec![Login { user: "root".into(), key: None }];
        let key = keys::Key { name: "work".into(), file: Some("/nonexistent/key".into()), agent_key: None, generated: false };
        SshData {
            catalog: Catalog { saved: vec![saved], config: vec![config_host], keys: vec![key] },
            hosts_error: Some("test".into()),
            keys_error: None,
        }
    }

    #[test]
    fn renders_every_state_headless() {
        let ctx = egui::Context::default();
        let mut data = data();
        let mut panel = SshPanel::new(&data);
        let mut actions = Vec::new();
        let states: [(Option<Selection>, Option<HostEditor>); 5] = [
            (Some(Selection::Saved(0)), None),
            (Some(Selection::Config(0)), None),
            (None, Some(HostEditor::new())),
            // Forwards and options set: both fold-outs open and lay out.
            (None, Some(HostEditor::from_host(&data.catalog.config[0].clone(), None))),
            (None, Some(HostEditor::from_host(&data.catalog.saved[0].clone(), Some(0)))),
        ];
        for (selected, editor) in states {
            panel.selected = selected;
            panel.editor = editor;
            for _ in 0..2 {
                ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui, &mut data, &mut actions))
                    .drop_without_applying_deltas();
            }
        }
        assert!(actions.is_empty());
    }

    #[test]
    fn form_round_trips_every_field() {
        let data = data();
        let mut host = Host::new("full", "full.example");
        host.port = 2222;
        host.user = "me".into();
        host.key = Some("work".into());
        host.logins = vec![Login { user: "root".into(), key: None }];
        host.identities = vec!["~/.ssh/a".into(), "~/.ssh/b".into()];
        host.identities_only = true;
        host.identity_agent = Some("none".into());
        host.proxy_jump = Some("a".into());
        host.options = Options {
            connect_timeout: Some(5),
            server_alive_interval: Some(0),
            server_alive_count_max: Some(2),
            compression: true,
            address_family: AddressFamily::Inet,
            preferred_authentications: Some("publickey".into()),
            strict_host_key_checking: HostKeyCheck::Yes,
            user_known_hosts_file: Some("~/.ssh/kh".into()),
            remote_command: Some("tmux attach".into()),
            set_env: vec!["A=1".into(), "B=x y".into()],
            send_env: vec!["LANG".into(), "LC_*".into()],
            local_forward: vec!["127.0.0.1:8080 localhost:80".into(), "~/db.sock /run/pg.sock".into()],
            remote_forward: vec!["9000 [::1]:9000".into(), "*:1080".into(), "9001 ~/app.sock".into()],
            dynamic_forward: vec!["[::1]:1080".into()],
            kex_algorithms: Some("-diffie-hellman-group1-sha1".into()),
            host_key_algorithms: Some("^ssh-ed25519".into()),
            ciphers: Some("aes256-ctr".into()),
            macs: Some("hmac-sha2-256".into()),
            ..Options::default()
        };
        assert_eq!(HostEditor::from_host(&host, None).to_host(&data).unwrap(), host);
        let cfg = data.catalog.config[0].clone();
        assert_eq!(HostEditor::from_host(&cfg, None).to_host(&data).unwrap(), cfg);
    }

    #[test]
    fn commit_rejects_bad_hosts_before_saving() {
        let mut data = data();
        let mut panel = SshPanel::new(&data);
        let mut try_commit = |edit: &dyn Fn(&mut HostEditor)| {
            let mut editor = HostEditor::new();
            editor.host = "b.example".into();
            edit(&mut editor);
            panel.editor = Some(editor);
            panel.commit(&mut data)
        };
        let forward = |listen: &str, target: &str| ForwardRow { kind: RowKind::Local, listen: listen.into(), target: target.into() };

        assert!(try_commit(&|e| e.jump = "b.example".into()).unwrap_err().contains("sich selbst"));
        assert!(try_commit(&|e| e.port = "0".into()).unwrap_err().contains("Port"));
        assert!(try_commit(&|e| e.logins[0].key = Some("fehlt".into())).unwrap_err().contains("fehlt"));
        // An unknown key on a further login is caught too.
        assert!(try_commit(&|e| e.logins.push(Login { user: "x".into(), key: Some("weg".into()) })).unwrap_err().contains("weg"));
        assert!(try_commit(&|e| e.logins.push(Login::default())).unwrap_err().contains("doppelt"));
        assert!(try_commit(&|e| e.logins.push(Login { user: "a b".into(), key: None })).unwrap_err().contains("Leerzeichen"));
        assert!(try_commit(&|e| e.forwards.push(forward("8080", ""))).unwrap_err().contains("Ziel"));
        assert!(try_commit(&|e| e.forwards.push(forward("x", "h:1"))).unwrap_err().contains("Weiterleitung 1"));
        assert!(try_commit(&|e| e.advanced.connect_timeout = "zehn".into()).unwrap_err().contains("Zahl"));
        assert!(try_commit(&|e| e.advanced.set_env = "X Y".into()).unwrap_err().contains("SetEnv"));
        assert!(
            try_commit(&|e| {
                e.jump = "a".into();
                e.advanced.proxy_command = "nc %h %p".into();
            })
            .unwrap_err()
            .contains("ProxyCommand")
        );
        // A blank forward row is ignored; the rest is valid, so only the
        // (refused) save fails.
        assert!(try_commit(&|e| e.forwards.push(ForwardRow::default())).unwrap_err().contains("unlesbar"));
        // Adopting the ~/.ssh/config host keeps its IdentityFile/IdentitiesOnly
        // -- valid, so it only fails at the (refused) save.
        let cfg = data.catalog.config[0].clone();
        panel.editor = Some(HostEditor::from_host(&cfg, None));
        assert!(panel.commit(&mut data).unwrap_err().contains("unlesbar"));
        assert_eq!(data.catalog.saved.len(), 1, "nothing was added");
    }

    #[test]
    fn badges_show_the_key_and_further_logins() {
        let mut host = Host::new("h", "h.example");
        assert_eq!(badge(&host), None);
        host.logins = vec![Login::default(), Login::default()];
        assert_eq!(badge(&host).as_deref(), Some("+2 Logins"));
        host.key = Some("work".into());
        assert_eq!(badge(&host).as_deref(), Some("work +2"));
    }
}
