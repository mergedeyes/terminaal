//! The sidebar's key section: SSH keys under names of the user's
//! choosing, which hosts then pick by name (`ssh::keys`). Keys can be
//! generated (Ed25519, optionally with a passphrase), added from an
//! existing file or taken over from the SSH agent; the public half can be
//! copied for a server's `authorized_keys`.

use std::path::PathBuf;

use egui::{Frame, Margin, RichText, TextEdit, Ui};

use crate::i18n::t;
use crate::ssh::keys::{self, Key, KeyInfo};
use crate::ssh::{self, local_user};
use crate::ui::ssh_panel::SshData;
use crate::ui::theme;
use crate::ui::widgets::{Status, list_row, section_title, tilde, weak};

pub struct KeysPanel {
    /// Info per key in `data.catalog.keys` (same order), and the keys it
    /// was computed for -- recomputed only when those change, since it
    /// reads files.
    infos: Vec<Result<KeyInfo, String>>,
    infos_for: Vec<Key>,
    selected: Option<usize>,
    form: Option<Form>,
    /// Removing a key in progress.
    delete: Option<Delete>,
    status: Option<Status>,
}

/// Removing a key asks one question after the other.
enum Delete {
    /// Remove button clicked once: really remove the entry?
    Confirm(usize),
    /// Yes -- and these files of a key generated here, delete them too?
    Files(usize, Vec<PathBuf>),
}

enum Form {
    Generate(GenerateForm),
    AddFile { name: String, path: String, error: Option<String> },
    FromAgent { identities: Result<Vec<KeyInfo>, String>, chosen: Option<usize>, name: String, error: Option<String> },
    Rename { index: usize, name: String, error: Option<String> },
}

struct GenerateForm {
    name: String,
    comment: String,
    path: String,
    /// Once edited by hand, the path no longer follows the name.
    path_touched: bool,
    passphrase: String,
    repeat: String,
    error: Option<String>,
}

impl GenerateForm {
    fn new() -> Self {
        let host = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default();
        let comment = if host.is_empty() { local_user() } else { format!("{}@{host}", local_user()) };
        Self {
            name: String::new(),
            comment,
            path: default_path(""),
            path_touched: false,
            passphrase: String::new(),
            repeat: String::new(),
            error: None,
        }
    }
}

impl Form {
    fn set_error(&mut self, err: String) {
        match self {
            Form::Generate(form) => form.error = Some(err),
            Form::AddFile { error, .. } | Form::FromAgent { error, .. } | Form::Rename { error, .. } => *error = Some(err),
        }
    }
}

/// `~/.ssh/id_ed25519_<name>`, with the name reduced to safe characters.
fn default_path(name: &str) -> String {
    let slug: String =
        name.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let slug = slug.trim_matches('_');
    format!("~/.ssh/id_ed25519_{}", if slug.is_empty() { "terminaal" } else { slug })
}

/// `ssh-ed25519` → `ED25519` etc.
fn short_algorithm(algorithm: &str) -> String {
    match algorithm {
        "ssh-rsa" => "RSA".into(),
        a if a.starts_with("ecdsa-") => "ECDSA".into(),
        a if a.starts_with("sk-") => "FIDO".into(),
        a => a.trim_start_matches("ssh-").to_uppercase(),
    }
}

fn short_fingerprint(fingerprint: &str) -> String {
    let short: String = fingerprint.chars().take(18).collect();
    if short.len() < fingerprint.len() { format!("{short}…") } else { short }
}

impl KeysPanel {
    pub fn new() -> Self {
        Self { infos: Vec::new(), infos_for: Vec::new(), selected: None, form: None, delete: None, status: None }
    }

    pub fn report(&mut self, result: Result<String, String>) {
        self.status = Some(Status::from_result(result));
    }

    pub fn show(&mut self, ui: &mut Ui, data: &mut SshData) {
        if self.infos_for != data.catalog.keys {
            self.infos = data.catalog.keys.iter().map(Key::info).collect();
            self.infos_for = data.catalog.keys.clone();
        }

        section_title(ui, &t!("sidebar-keys"));
        if let Some(err) = data.keys_error.clone() {
            ui.label(RichText::new(err).color(theme::colors().error));
            if ui.button(t!("common-reload")).clicked() {
                data.reload();
            }
        }
        let keys = data.catalog.keys.clone();
        let mut clicked = None;
        let hint = t!("keys-click-details");
        for (i, key) in keys.iter().enumerate() {
            let subtitle = match &self.infos[i] {
                Ok(info) => format!("{} · {}", short_algorithm(&info.algorithm), short_fingerprint(&info.fingerprint)),
                Err(_) => t!("keys-unreadable"),
            };
            let badge = if key.agent_key.is_some() { t!("keys-badge-agent") } else { t!("keys-badge-file") };
            if list_row(ui, &key.name, &subtitle, self.selected == Some(i), Some(&badge), &hint).clicked() {
                clicked = Some(i);
            }
        }
        if keys.is_empty() && data.keys_error.is_none() && self.form.is_none() {
            ui.label(weak(t!("keys-none")));
        }
        if let Some(i) = clicked
            && self.selected != Some(i)
        {
            self.selected = Some(i);
            self.delete = None;
            self.status = None;
        }
        if let Some(i) = self.selected
            && let Some(key) = keys.get(i)
            && self.form.is_none()
        {
            self.details(ui, data, i, key);
        }

        ui.add_space(6.0);
        if self.form.is_some() {
            self.form_ui(ui, data);
        } else if data.keys_error.is_none() {
            ui.horizontal_wrapped(|ui| {
                if ui.button(t!("keys-generate")).clicked() {
                    self.form = Some(Form::Generate(GenerateForm::new()));
                    self.status = None;
                }
                if ui.button(t!("keys-add-file")).on_hover_text(t!("keys-add-file-hint")).clicked() {
                    self.form = Some(Form::AddFile { name: String::new(), path: String::new(), error: None });
                    self.status = None;
                }
                if ui.button(t!("keys-from-agent")).on_hover_text(t!("keys-from-agent-hint")).clicked() {
                    let identities = keys::agent_identities();
                    // Preselect the first key that isn't taken over yet, so
                    // saving right away uses the one that shows as chosen.
                    let chosen = identities.as_ref().ok().and_then(|list| {
                        list.iter().position(|info| {
                            !data.catalog.keys.iter().any(|k| k.agent_key.as_deref() == Some(info.public_openssh.as_str()))
                        })
                    });
                    let name = chosen
                        .and_then(|i| identities.as_ref().ok()?.get(i))
                        .map(|info| info.comment.clone())
                        .unwrap_or_default();
                    self.form = Some(Form::FromAgent { identities, chosen, name, error: None });
                    self.status = None;
                }
            });
        }

        if let Some(status) = &self.status {
            ui.add_space(8.0);
            status.show(ui);
        }
        ui.add_space(14.0);
        ui.label(weak(t!("keys-note")).size(11.0));
    }

    fn details(&mut self, ui: &mut Ui, data: &mut SshData, index: usize, key: &Key) {
        let info = self.infos.get(index).cloned().unwrap_or_else(|| Err(t!("keys-unknown")));
        let users: Vec<String> =
            data.catalog.saved.iter().filter(|h| h.uses_key(&key.name)).map(|h| h.name.clone()).collect();

        ui.add_space(4.0);
        Frame::new().fill(theme::colors().row).corner_radius(egui::CornerRadius::same(4)).inner_margin(Margin::same(8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &info {
                Ok(info) => {
                    ui.label(t!("keys-type", algorithm = short_algorithm(&info.algorithm)));
                    ui.add(egui::Label::new(RichText::new(&info.fingerprint).monospace().size(11.0)).wrap());
                    if !info.comment.is_empty() {
                        ui.label(weak(t!("keys-comment-line", comment = &info.comment)));
                    }
                }
                Err(err) => {
                    ui.label(RichText::new(err).color(theme::colors().error));
                }
            }
            match &key.file {
                Some(file) => ui.label(weak(t!("keys-file-line", file = file))),
                None => ui.label(weak(t!("keys-agent-only"))),
            };
            if !users.is_empty() {
                ui.label(weak(t!("keys-used-by", hosts = users.join(", "))));
            }
        });

        let confirming = matches!(self.delete, Some(Delete::Confirm(i)) if i == index);
        let files = match &self.delete {
            Some(Delete::Files(i, files)) if *i == index => Some(files.clone()),
            _ => None,
        };
        let (mut copy, mut rename, mut ask, mut confirm, mut cancel) = (false, false, false, false, false);
        // Answer to the file question: delete the files too?
        let mut with_files = None;
        ui.add_space(4.0);
        if let Some(files) = &files {
            ui.label(t!("keys-delete-files-question", name = &key.name));
            for file in files {
                ui.label(RichText::new(tilde(file)).monospace().size(11.0).color(theme::colors().text_weak));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(RichText::new(t!("keys-delete-files")).color(theme::colors().error)).clicked() {
                    with_files = Some(true);
                }
                if ui.button(t!("keys-keep-files")).clicked() {
                    with_files = Some(false);
                }
                cancel = ui.button(t!("common-cancel")).clicked();
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                copy = ui.add_enabled(info.is_ok(), egui::Button::new(t!("keys-copy-public"))).clicked();
                if confirming {
                    confirm = ui.button(RichText::new(t!("common-remove")).color(theme::colors().error)).clicked();
                    cancel = ui.button(t!("common-no")).clicked();
                } else {
                    rename = ui.button("✏").on_hover_text(t!("keys-rename")).clicked();
                    ask = ui.button("🗑").on_hover_text(t!("common-remove")).clicked();
                }
            });
        }

        if copy && let Ok(info) = &info {
            ui.ctx().copy_text(info.public_openssh.clone());
            self.report(Ok(t!("keys-copied", name = &key.name)));
        }
        if rename {
            self.form = Some(Form::Rename { index, name: key.name.clone(), error: None });
            self.status = None;
        }
        if ask {
            self.delete = Some(Delete::Confirm(index));
        }
        if cancel {
            self.delete = None;
        }
        if confirm {
            self.delete = None;
            if !users.is_empty() {
                self.report(Err(t!("keys-still-used", name = &key.name, hosts = users.join(", "))));
            } else {
                let files = files_to_offer(data, index, key);
                if files.is_empty() {
                    self.remove(data, index, key, &[]);
                } else {
                    self.delete = Some(Delete::Files(index, files));
                    self.status = None;
                }
            }
        }
        if let (Some(with_files), Some(files)) = (with_files, &files) {
            self.delete = None;
            self.remove(data, index, key, if with_files { files } else { &[] });
        }
    }

    /// Remove the key's entry and, once that's saved, delete `files`.
    fn remove(&mut self, data: &mut SshData, index: usize, key: &Key, files: &[PathBuf]) {
        let result = data
            .update_keys(|keys| {
                keys.remove(index);
            })
            .and_then(|()| match (files.is_empty(), &key.file) {
                (false, _) => keys::delete_files(files)
                    .map(|()| t!("keys-removed-with-files", name = &key.name))
                    .map_err(|err| t!("keys-removed-but", name = &key.name, err = err)),
                (true, Some(_)) => Ok(t!("keys-removed-file-kept", name = &key.name)),
                (true, None) => Ok(t!("keys-removed", name = &key.name)),
            });
        self.selected = None;
        self.report(result);
    }

    fn form_ui(&mut self, ui: &mut Ui, data: &mut SshData) {
        let Some(form) = self.form.as_mut() else { return };
        let (mut save, mut cancel) = (false, false);
        Frame::group(ui.style()).fill(theme::colors().row).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match form {
                Form::Generate(form) => {
                    ui.label(RichText::new(t!("keys-new-title")).strong());
                    if text_field(ui, "Name", &mut form.name, &t!("keys-name-hint")).changed() && !form.path_touched {
                        form.path = default_path(&form.name);
                    }
                    text_field(ui, &t!("keys-comment"), &mut form.comment, "");
                    if text_field(ui, &t!("keys-location"), &mut form.path, "~/.ssh/id_ed25519_…").changed() {
                        form.path_touched = true;
                    }
                    ui.label(weak(t!("keys-passphrase-optional")));
                    ui.add(TextEdit::singleline(&mut form.passphrase).password(true).desired_width(f32::INFINITY));
                    ui.label(weak(t!("keys-passphrase-repeat")));
                    ui.add(TextEdit::singleline(&mut form.repeat).password(true).desired_width(f32::INFINITY));
                    if form.passphrase.is_empty() {
                        ui.label(weak(t!("keys-no-passphrase-note")).size(11.0));
                    }
                    show_error(ui, &form.error);
                }
                Form::AddFile { name, path, error } => {
                    ui.label(RichText::new(t!("keys-add-file-title")).strong());
                    text_field(ui, "Name", name, &t!("keys-name-hint"));
                    text_field(ui, &t!("keys-private-file"), path, "~/.ssh/id_ed25519");
                    show_error(ui, error);
                }
                Form::FromAgent { identities, chosen, name, error } => {
                    ui.label(RichText::new(t!("keys-agent-title")).strong());
                    match identities {
                        Err(err) => {
                            ui.label(RichText::new(err.as_str()).color(theme::colors().error));
                        }
                        Ok(list) if list.is_empty() => {
                            ui.label(weak(t!("keys-agent-empty")));
                        }
                        Ok(list) => {
                            for (i, info) in list.iter().enumerate() {
                                let label = if info.comment.is_empty() { t!("keys-no-comment") } else { info.comment.clone() };
                                let text = format!("{label} · {}", short_fingerprint(&info.fingerprint));
                                if ui.selectable_label(*chosen == Some(i), text).clicked() {
                                    // The name follows the choice until it's
                                    // been typed in by hand.
                                    let prefilled = chosen.and_then(|c| list.get(c)).map(|c| c.comment.as_str());
                                    if name.is_empty() || Some(name.as_str()) == prefilled {
                                        name.clone_from(&info.comment);
                                    }
                                    *chosen = Some(i);
                                }
                            }
                        }
                    }
                    text_field(ui, "Name", name, "1Password");
                    show_error(ui, error);
                }
                Form::Rename { name, error, .. } => {
                    ui.label(RichText::new(t!("keys-rename-title")).strong());
                    text_field(ui, &t!("keys-new-name"), name, "");
                    ui.label(weak(t!("keys-rename-note")).size(11.0));
                    show_error(ui, error);
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                save = ui.button(t!("common-save")).clicked();
                cancel = ui.button(t!("common-cancel")).clicked();
            });
        });

        if cancel {
            self.form = None;
        } else if save {
            match self.commit(data) {
                Ok(message) => {
                    self.form = None;
                    self.report(Ok(message));
                }
                Err(err) => {
                    if let Some(form) = self.form.as_mut() {
                        form.set_error(err);
                    }
                }
            }
        }
    }

    /// Carry out the open form; the key store (and, for a rename, the
    /// hosts) are saved right away.
    fn commit(&mut self, data: &mut SshData) -> Result<String, String> {
        let form = self.form.as_ref().ok_or_else(|| t!("keys-no-form"))?;
        let message = match form {
            Form::Generate(form) => {
                let name = unique_name(&form.name, data, None)?;
                if form.passphrase != form.repeat {
                    return Err(t!("keys-passphrase-mismatch"));
                }
                let path_text = form.path.trim();
                if path_text.is_empty() {
                    return Err(t!("keys-location-missing"));
                }
                keys::generate(&ssh::expand_tilde(path_text), form.comment.trim(), &form.passphrase)?;
                data.update_keys(|keys| {
                    keys.push(Key { name: name.clone(), file: Some(path_text.to_string()), agent_key: None, generated: true });
                })?;
                t!("keys-generated", name = &name, path = path_text)
            }
            Form::AddFile { name, path, .. } => {
                let name = unique_name(name, data, None)?;
                let path_text = path.trim();
                if path_text.is_empty() {
                    return Err(t!("keys-file-missing"));
                }
                keys::public_key_of_file(&ssh::expand_tilde(path_text))?;
                data.update_keys(|keys| {
                    keys.push(Key { name: name.clone(), file: Some(path_text.to_string()), agent_key: None, generated: false });
                })?;
                t!("keys-added", name = &name)
            }
            Form::FromAgent { identities, chosen, name, .. } => {
                let info = identities
                    .as_ref()
                    .ok()
                    .and_then(|list| list.get((*chosen)?))
                    .ok_or_else(|| t!("keys-choose-from-list"))?;
                let name = unique_name(name, data, None)?;
                if let Some(existing) = data.catalog.keys.iter().find(|k| k.agent_key.as_deref() == Some(info.public_openssh.as_str())) {
                    return Err(t!("keys-agent-duplicate", name = &existing.name));
                }
                data.update_keys(|keys| {
                    keys.push(Key {
                        name: name.clone(),
                        file: None,
                        agent_key: Some(info.public_openssh.clone()),
                        generated: false,
                    });
                })?;
                t!("keys-taken-over", name = &name)
            }
            Form::Rename { index, name, .. } => {
                let index = *index;
                let new_name = unique_name(name, data, Some(index))?;
                let old_name = data.catalog.keys.get(index).ok_or_else(|| t!("keys-gone"))?.name.clone();
                data.update_keys(|keys| keys[index].name.clone_from(&new_name))?;
                if data.catalog.saved.iter().any(|h| h.uses_key(&old_name)) {
                    data.update_hosts(|hosts| {
                        for host in hosts.iter_mut() {
                            host.rename_key(&old_name, &new_name);
                        }
                    })?;
                }
                t!("keys-renamed", old = &old_name, new = &new_name)
            }
        };
        if !matches!(form, Form::Rename { .. }) {
            self.selected = Some(data.catalog.keys.len() - 1);
        }
        Ok(message)
    }
}

/// The files a key generated here still has -- none if another entry uses
/// the same file, which then has to stay.
fn files_to_offer(data: &SshData, index: usize, key: &Key) -> Vec<PathBuf> {
    let Some(path) = key.file_path().filter(|_| key.generated) else { return Vec::new() };
    let shared = data.catalog.keys.iter().enumerate().any(|(i, k)| i != index && k.file_path().as_ref() == Some(&path));
    if shared { Vec::new() } else { keys::existing_files(&path) }
}

/// Trimmed, non-empty and not taken by another key.
fn unique_name(name: &str, data: &SshData, except: Option<usize>) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(t!("common-name-missing"));
    }
    if data.catalog.keys.iter().enumerate().any(|(i, k)| k.name == name && Some(i) != except) {
        return Err(t!("keys-name-taken", name = name));
    }
    Ok(name.to_string())
}

fn text_field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> egui::Response {
    ui.label(weak(label));
    ui.add(TextEdit::singleline(value).desired_width(f32::INFINITY).hint_text(hint))
}

fn show_error(ui: &mut Ui, error: &Option<String>) {
    if let Some(err) = error {
        ui.label(RichText::new(err).color(theme::colors().error));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::{Catalog, Host};

    /// Saving is refused (`*_error` set), so nothing here can touch the
    /// real ~/.config/terminaal even by accident.
    fn data() -> SshData {
        let mut host = Host::new("srv", "srv.example");
        host.key = Some("work".into());
        let key = Key { name: "work".into(), file: Some("/nonexistent/key".into()), agent_key: None, generated: false };
        SshData {
            catalog: Catalog { saved: vec![host], config: Vec::new(), keys: vec![key] },
            hosts_error: Some("test".into()),
            keys_error: Some("test".into()),
        }
    }

    /// Lays the section out in headless egui passes -- a key selected,
    /// then each form open -- to catch panics the type checker can't.
    #[test]
    fn renders_every_state_headless() {
        let ctx = egui::Context::default();
        let mut data = data();
        let mut panel = KeysPanel::new();
        let forms = [
            None,
            Some(Form::Generate(GenerateForm::new())),
            Some(Form::AddFile { name: String::new(), path: String::new(), error: Some("x".into()) }),
            Some(Form::FromAgent { identities: Err("kein Agent".into()), chosen: None, name: String::new(), error: None }),
            Some(Form::Rename { index: 0, name: "work".into(), error: None }),
        ];
        for form in forms {
            panel.selected = Some(0);
            panel.form = form;
            ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui, &mut data)).drop_without_applying_deltas();
        }
        panel.form = None;
        for delete in [Delete::Confirm(0), Delete::Files(0, vec!["/nonexistent/key".into()])] {
            panel.delete = Some(delete);
            ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui, &mut data)).drop_without_applying_deltas();
        }
        assert!(panel.infos[0].is_err(), "the key file doesn't exist");
    }

    #[test]
    fn offers_to_delete_only_unshared_files_of_generated_keys() {
        let dir = std::env::temp_dir().join(format!("terminaal-keys-panel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("id_gen");
        keys::generate(&path, "", "").unwrap();
        let file = Some(path.display().to_string());
        let generated = Key { name: "gen".into(), file: file.clone(), agent_key: None, generated: true };
        let added = Key { name: "added".into(), file, agent_key: None, generated: false };

        let mut data = data();
        data.catalog.keys = vec![generated.clone()];
        assert_eq!(files_to_offer(&data, 0, &generated), vec![path.clone(), dir.join("id_gen.pub")]);
        // Added from an existing file: never offered.
        data.catalog.keys = vec![added.clone()];
        assert!(files_to_offer(&data, 0, &added).is_empty());
        // Generated, but another entry uses the same file.
        data.catalog.keys = vec![generated.clone(), added];
        assert!(files_to_offer(&data, 0, &generated).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn default_paths_and_labels() {
        assert_eq!(default_path("Arbeit Laptop"), "~/.ssh/id_ed25519_arbeit_laptop");
        assert_eq!(default_path("  "), "~/.ssh/id_ed25519_terminaal");
        assert_eq!(short_algorithm("ssh-ed25519"), "ED25519");
        assert_eq!(short_algorithm("ecdsa-sha2-nistp256"), "ECDSA");
    }
}
