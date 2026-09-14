//! The built-in commands, under the aliases in the sidebar's shell
//! section: one button per chore (`crate::commands`), tailored to the
//! system the active tab is on. Below them your own commands
//! (`crate::snippets`): those for this tab's system and host as buttons,
//! all of them to add, edit and delete under "Manage".
//!
//! A click hands the line back as [`SidebarAction::RunCommand`]; whether
//! it is run or only typed into the prompt is `commands_run`. Before the
//! first one is run, the warning below says so -- once per installation
//! (`commands_warned`).

use egui::{Align, ComboBox, CornerRadius, Frame, Layout, Margin, RichText, TextEdit, TextStyle, Ui};

use crate::commands::{self, Family, Group, Target};
use crate::config::{Config, Setting};
use crate::i18n::t;
use crate::snippets::{self, Autorun, Snippet};
use crate::ui::sidebar::SidebarAction;
use crate::ui::theme;
use crate::ui::widgets::{Status, section_title, weak};

#[derive(Default)]
pub struct CommandsPanel {
    /// The command line waiting for the warning to be acknowledged.
    pending: Option<String>,
    snippets: Vec<Snippet>,
    /// snippets.toml couldn't be read: nothing is saved over it.
    load_error: Option<String>,
    /// The list of all snippets is open.
    managing: bool,
    editor: Option<SnippetEditor>,
    /// Snippet whose delete button was clicked once.
    confirm_delete: Option<usize>,
    status: Option<Status>,
}

/// Where a snippet applies, as the form picks it.
#[derive(Clone, PartialEq, Eq)]
enum Place {
    Everywhere,
    Local,
    Host(String),
}

impl Place {
    fn of(snippet: &Snippet) -> Self {
        match (&snippet.host, snippet.local) {
            (Some(host), _) => Self::Host(host.clone()),
            (None, true) => Self::Local,
            (None, false) => Self::Everywhere,
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Everywhere => t!("snip-all-hosts"),
            Self::Local => t!("snip-local-only"),
            Self::Host(host) => host.clone(),
        }
    }
}

/// The form for a new or changed snippet.
struct SnippetEditor {
    /// Index while editing; `None` for a new one.
    original: Option<usize>,
    name: String,
    command: String,
    system: Option<Family>,
    place: Place,
    autorun: Option<Autorun>,
    hidden: bool,
    error: Option<String>,
    focus_pending: bool,
}

impl SnippetEditor {
    fn new(original: Option<usize>, snippet: &Snippet) -> Self {
        Self {
            original,
            name: snippet.name.clone(),
            command: snippet.command.clone(),
            system: snippet.family(),
            place: Place::of(snippet),
            autorun: snippet.autorun,
            hidden: snippet.hidden,
            error: None,
            focus_pending: true,
        }
    }

    fn to_snippet(&self) -> Result<Snippet, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(t!("common-name-missing"));
        }
        let command = self.command.trim_end();
        if command.trim().is_empty() {
            return Err(t!("snip-command-missing"));
        }
        if self.place == Place::Local && self.autorun == Some(Autorun::Login) {
            return Err(t!("snip-local-login"));
        }
        Ok(Snippet {
            name: name.to_string(),
            command: command.to_string(),
            system: self.system.map(|family| family.key().to_string()),
            host: match &self.place {
                Place::Host(host) => Some(host.clone()),
                Place::Everywhere | Place::Local => None,
            },
            local: self.place == Place::Local,
            autorun: self.autorun,
            hidden: self.hidden,
        })
    }
}

impl CommandsPanel {
    /// The snippets as saved.
    pub fn snippets(&self) -> &[Snippet] {
        &self.snippets
    }

    /// With the snippets from snippets.toml.
    pub fn load() -> Self {
        match snippets::load() {
            Ok(snippets) => Self { snippets, ..Self::default() },
            Err(err) => Self { load_error: Some(err), ..Self::default() },
        }
    }

    /// `target` is `None` while no terminal tab is showing (the settings
    /// tab is): there'd be no shell to send anything to. `hosts` are the
    /// names a snippet can be bound to.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        config: &Config,
        target: Option<&Target>,
        hosts: &[String],
        actions: &mut Vec<SidebarAction>,
    ) {
        section_title(ui, &t!("cmd-title"));
        let Some(target) = target else {
            ui.label(weak(t!("cmd-no-tab")));
            return;
        };
        if target.terminals > 1 {
            ui.label(RichText::new(t!("cmd-broadcast", count = target.terminals)).color(theme::colors().error));
            ui.add_space(4.0);
        }

        match target.system {
            // An SSH session that hasn't got to the probe yet.
            None => {
                ui.label(weak(t!("cmd-system-probing")));
            }
            Some(system) => self.builtins(ui, config, target, system, actions),
        }

        ui.add_space(6.0);
        self.snippets_ui(ui, config, target, hosts, actions);

        if self.pending.is_some() {
            self.warning(ui, actions);
        }
        if let Some(status) = &self.status {
            ui.add_space(6.0);
            status.show(ui);
        }
    }

    fn builtins(
        &mut self,
        ui: &mut Ui,
        config: &Config,
        target: &Target,
        system: commands::System,
        actions: &mut Vec<SidebarAction>,
    ) {
        if system.family == Family::Unknown {
            ui.label(weak(t!("cmd-system-unknown")));
        } else {
            let hint = match (&target.host, target.configured) {
                _ if target.configured => t!("settings-key-hint", key = "system"),
                (Some(host), _) => t!("cmd-system-remote-hint", host = host),
                (None, _) => t!("cmd-system-local-hint"),
            };
            ui.label(weak(t!("cmd-system", system = system.family.label()))).on_hover_text(hint);
        }
        ui.add_space(6.0);

        let commands = commands::catalog(system, config.commands_assume_yes);
        for group in Group::ALL {
            let mut of_group = commands.iter().filter(|command| command.builtin.group() == group).peekable();
            if of_group.peek().is_none() {
                continue;
            }
            ui.label(weak(group.label()).size(11.0));
            ui.horizontal_wrapped(|ui| {
                for command in of_group {
                    let label = RichText::new(command.builtin.label());
                    let label = if command.changes { label.color(theme::colors().accent) } else { label };
                    let hint = if config.commands_run {
                        t!("cmd-run-hint", line = &command.line)
                    } else {
                        t!("cmd-type-hint", line = &command.line)
                    };
                    if ui.button(label).on_hover_text(hint).clicked() {
                        self.activate(&command.line, config, actions);
                    }
                }
            });
            ui.add_space(4.0);
        }
    }

    /// The snippets for this tab as buttons; under "Manage" all of them.
    fn snippets_ui(
        &mut self,
        ui: &mut Ui,
        config: &Config,
        target: &Target,
        hosts: &[String],
        actions: &mut Vec<SidebarAction>,
    ) {
        ui.horizontal(|ui| {
            ui.label(weak(t!("snip-title")).size(11.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if self.managing { t!("snip-manage-done") } else { t!("snip-manage") };
                if ui.small_button(label).clicked() {
                    self.managing = !self.managing;
                    self.editor = None;
                    self.confirm_delete = None;
                    self.status = None;
                }
            });
        });
        if let Some(err) = &self.load_error {
            ui.label(RichText::new(err).color(theme::colors().error));
        }

        let applicable: Vec<Snippet> =
            self.snippets.iter().filter(|snippet| snippet.applies(target) && !snippet.hidden).cloned().collect();
        if applicable.is_empty() && !self.managing {
            let hidden = self.snippets.iter().filter(|snippet| snippet.applies(target) && snippet.hidden).count();
            let hint = if hidden > 0 {
                t!("snip-all-hidden", count = hidden)
            } else if self.snippets.is_empty() {
                t!("snip-none")
            } else {
                t!("snip-none-here")
            };
            ui.label(weak(hint).size(11.0));
        }
        ui.horizontal_wrapped(|ui| {
            for snippet in &applicable {
                let hint = if config.commands_run {
                    t!("cmd-run-hint", line = &snippet.command)
                } else {
                    t!("cmd-type-hint", line = &snippet.command)
                };
                if ui.button(&snippet.name).on_hover_text(hint).clicked() {
                    self.activate(&snippet.command, config, actions);
                }
            }
        });

        if self.managing {
            ui.add_space(4.0);
            self.manage(ui, config, target, hosts, actions);
        }
    }

    fn manage(&mut self, ui: &mut Ui, config: &Config, target: &Target, hosts: &[String], actions: &mut Vec<SidebarAction>) {
        let (mut edit, mut ask_delete, mut delete, mut cancel_delete) = (None, None, None, false);
        let mut run = None;
        for (i, snippet) in self.snippets.iter().enumerate() {
            let confirming = self.confirm_delete == Some(i);
            Frame::new().fill(theme::colors().row).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(8, 6)).show(
                ui,
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&snippet.name).strong().color(theme::colors().text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if confirming {
                                cancel_delete |= ui.small_button(t!("common-no")).clicked();
                                if ui.small_button(RichText::new(t!("common-delete")).color(theme::colors().error)).clicked() {
                                    delete = Some(i);
                                }
                            } else {
                                if ui.small_button("🗑").on_hover_text(t!("common-delete")).clicked() {
                                    ask_delete = Some(i);
                                }
                                if ui.small_button("✏").on_hover_text(t!("common-edit")).clicked() {
                                    edit = Some(i);
                                }
                                // Hidden ones run from here; any that fits this tab.
                                if snippet.applies(target) && ui.small_button("▶").on_hover_text(t!("snip-run")).clicked() {
                                    run = Some(i);
                                }
                            }
                        });
                    });
                    let mut lines = snippet.command.lines();
                    let mut preview = lines.next().unwrap_or_default().to_string();
                    if lines.next().is_some() {
                        preview.push_str("  …");
                    }
                    ui.add(egui::Label::new(RichText::new(preview).monospace().size(12.0).color(theme::colors().text_weak)).truncate());
                    let scope = scope_label(snippet);
                    ui.label(weak(scope).size(11.0));
                },
            );
            ui.add_space(2.0);
        }
        if let Some(i) = run {
            let command = self.snippets[i].command.clone();
            self.activate(&command, config, actions);
        }
        if let Some(i) = edit {
            self.editor = Some(SnippetEditor::new(Some(i), &self.snippets[i]));
            self.confirm_delete = None;
            self.status = None;
        }
        if let Some(i) = ask_delete {
            self.confirm_delete = Some(i);
        }
        if cancel_delete {
            self.confirm_delete = None;
        }
        if let Some(i) = delete {
            self.confirm_delete = None;
            let name = self.snippets[i].name.clone();
            let result = self.update(|snippets| {
                snippets.remove(i);
            });
            self.status = Some(Status::from_result(result.map(|()| t!("common-deleted", name = &name))));
        }

        ui.add_space(4.0);
        if self.editor.is_some() {
            self.editor_form(ui, hosts);
        } else if ui.button(t!("snip-add")).clicked() {
            self.editor = Some(SnippetEditor::new(None, &Snippet::default()));
            self.confirm_delete = None;
            self.status = None;
        }
    }

    fn editor_form(&mut self, ui: &mut Ui, hosts: &[String]) {
        let Some(editor) = self.editor.as_mut() else { return };
        let (mut save, mut cancel) = (false, false);
        Frame::group(ui.style()).fill(theme::colors().row).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(if editor.original.is_some() { t!("snip-edit") } else { t!("snip-new") }).strong());
            ui.label(weak("Name"));
            let name = ui.add(TextEdit::singleline(&mut editor.name).desired_width(f32::INFINITY).hint_text(t!("snip-name-hint")));
            if editor.focus_pending {
                name.request_focus();
                editor.focus_pending = false;
            }
            ui.label(weak(t!("shells-command")));
            ui.add(
                TextEdit::multiline(&mut editor.command)
                    .font(TextStyle::Monospace)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("journalctl -f"),
            );
            ui.label(weak(t!("snip-command-note")).size(11.0));

            ui.label(weak(t!("snip-system")));
            let all_systems = t!("snip-all-systems");
            ComboBox::from_id_salt("snippet-system")
                .width(ui.available_width())
                .selected_text(editor.system.map_or_else(|| all_systems.clone(), Family::label))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut editor.system, None, all_systems.as_str());
                    for family in Family::ALL.into_iter().filter(|family| *family != Family::Unknown) {
                        ui.selectable_value(&mut editor.system, Some(family), family.label());
                    }
                });
            ui.label(weak(t!("snip-host")));
            ComboBox::from_id_salt("snippet-host")
                .width(ui.available_width())
                .selected_text(editor.place.label())
                .show_ui(ui, |ui| {
                    for place in [Place::Everywhere, Place::Local] {
                        let label = place.label();
                        ui.selectable_value(&mut editor.place, place, label);
                    }
                    ui.separator();
                    // A host that's gone from the list still shows.
                    let current = match &editor.place {
                        Place::Host(host) if !hosts.contains(host) => Some(host.clone()),
                        _ => None,
                    };
                    for host in current.iter().chain(hosts) {
                        ui.selectable_value(&mut editor.place, Place::Host(host.clone()), host);
                    }
                });

            ui.label(weak(t!("snip-autorun")));
            let never = t!("snip-autorun-never");
            ComboBox::from_id_salt("snippet-autorun")
                .width(ui.available_width())
                .selected_text(editor.autorun.map_or_else(|| never.clone(), Autorun::label))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut editor.autorun, None, never.as_str());
                    for autorun in [Autorun::Shell, Autorun::Login] {
                        ui.selectable_value(&mut editor.autorun, Some(autorun), autorun.label());
                    }
                });
            if editor.autorun.is_some() {
                ui.label(weak(t!("snip-autorun-note")).size(11.0));
            }
            ui.checkbox(&mut editor.hidden, t!("snip-hidden")).on_hover_text(t!("snip-hidden-hint"));

            if let Some(err) = &editor.error {
                ui.label(RichText::new(err).color(theme::colors().error));
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
            match self.commit() {
                Ok(message) => {
                    self.editor = None;
                    self.status = Some(Status::from_result(Ok(message)));
                }
                Err(err) => {
                    if let Some(editor) = self.editor.as_mut() {
                        editor.error = Some(err);
                    }
                }
            }
        }
    }

    /// Check the form and save the snippet.
    fn commit(&mut self) -> Result<String, String> {
        let editor = self.editor.as_ref().ok_or_else(|| t!("shells-no-entry-open"))?;
        let snippet = editor.to_snippet()?;
        let original = editor.original;
        if self.snippets.iter().enumerate().any(|(i, other)| other.name == snippet.name && Some(i) != original) {
            return Err(t!("snip-name-taken", name = &snippet.name));
        }
        let name = snippet.name.clone();
        self.update(|snippets| match original {
            Some(i) if i < snippets.len() => snippets[i] = snippet,
            _ => snippets.push(snippet),
        })?;
        Ok(t!("common-saved", name = &name))
    }

    /// Change a copy, write it, then take it over.
    fn update(&mut self, change: impl FnOnce(&mut Vec<Snippet>)) -> Result<(), String> {
        if self.load_error.is_some() {
            return Err(t!("ssh-store-unreadable", file = "snippets.toml"));
        }
        let mut snippets = self.snippets.clone();
        change(&mut snippets);
        snippets::save(&snippets)?;
        self.snippets = snippets;
        Ok(())
    }

    /// A button was clicked: off it goes, unless the warning still has to
    /// be acknowledged. That warning is about running things unasked, so
    /// it only applies while that's what a click does.
    fn activate(&mut self, line: &str, config: &Config, actions: &mut Vec<SidebarAction>) {
        if config.commands_run && !config.commands_warned {
            self.pending = Some(line.to_string());
        } else {
            actions.push(SidebarAction::RunCommand(line.to_string()));
        }
    }

    /// Shown before the first command is run: they go straight to the
    /// shell, and where to change that.
    fn warning(&mut self, ui: &mut Ui, actions: &mut Vec<SidebarAction>) {
        let mut confirmed = false;
        let mut cancelled = false;
        ui.add_space(4.0);
        Frame::group(ui.style()).fill(theme::colors().row).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(10)).show(
            ui,
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(t!("cmd-warn-title")).strong().color(theme::colors().accent));
                ui.label(t!("cmd-warn-body"));
                if let Some(line) = &self.pending {
                    ui.add_space(4.0);
                    ui.label(RichText::new(line).monospace().size(12.0).color(theme::colors().text_weak));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    confirmed = ui.button(t!("cmd-warn-run")).clicked();
                    cancelled = ui.button(t!("common-cancel")).clicked();
                });
            },
        );
        if confirmed && let Some(line) = self.pending.take() {
            actions.push(SidebarAction::ChangeSetting { setting: Setting::CommandsWarned(true), save: true });
            actions.push(SidebarAction::RunCommand(line));
        }
        if cancelled {
            self.pending = None;
        }
    }
}

/// Where a snippet shows: everywhere, or which system and host.
fn scope_label(snippet: &Snippet) -> String {
    let system = snippet.system.as_ref().map(|key| snippet.family().map_or_else(|| key.clone(), Family::label));
    let scope = match (system, &snippet.host) {
        (None, None) if snippet.local => t!("snip-local-only"),
        (Some(system), None) if snippet.local => format!("{system} · {}", t!("snip-local-only")),
        (None, None) => t!("snip-everywhere"),
        (Some(system), None) => system,
        (None, Some(host)) => t!("snip-on-host", host = host),
        (Some(system), Some(host)) => format!("{system} · {}", t!("snip-on-host", host = host)),
    };
    let scope = match snippet.autorun {
        Some(autorun) => format!("{scope} · {}", autorun.label()),
        None => scope,
    };
    if snippet.hidden { format!("{scope} · {}", t!("snip-hidden-short")) } else { scope }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::System;
    use crate::snippets::Snippet;

    fn target(family: Family) -> Target {
        Target { system: Some(System { family, root: false, ..System::default() }), ..Target::default() }
    }

    /// Lays the section out in headless egui passes -- no tab, a system
    /// we know, one we don't, and the warning -- to catch panics the type
    /// checker can't.
    #[test]
    fn renders_every_state_headless() {
        let ctx = egui::Context::default();
        let config = Config::default();
        let mut panel = CommandsPanel { load_error: Some("test".into()), ..CommandsPanel::default() };
        panel.snippets = vec![
            Snippet { name: "Logs".into(), command: "journalctl -f".into(), system: Some("arch".into()), ..Snippet::default() },
            Snippet {
                name: "Deploy".into(),
                command: "cd /srv\n./deploy".into(),
                host: Some("web1".into()),
                autorun: Some(Autorun::Login),
                ..Snippet::default()
            },
            // Only under "Manage"; alone where it's the only one that fits.
            Snippet { name: "tmux".into(), command: "tmux".into(), system: Some("debian".into()), hidden: true, ..Snippet::default() },
        ];
        let hosts = vec!["web1".to_string()];
        let broadcast = Target { terminals: 3, ..target(Family::Arch) };
        for (managing, editor) in [(false, None), (true, None), (true, Some(0))] {
            panel.managing = managing;
            panel.editor = editor.map(|i| SnippetEditor::new(Some(i), &panel.snippets[i]));
            for target in [
                None,
                Some(target(Family::Arch)),
                Some(target(Family::Debian)),
                Some(target(Family::Unknown)),
                Some(Target::default()),
                Some(broadcast.clone()),
            ] {
                let mut actions = Vec::new();
                ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui, &config, target.as_ref(), &hosts, &mut actions))
                    .drop_without_applying_deltas();
                assert!(actions.is_empty(), "nothing was clicked");
            }
        }
        panel.pending = Some(commands::catalog(System { family: Family::Debian, root: false, ..System::default() }, false).remove(0).line);
        let mut actions = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui, &config, Some(&target(Family::Debian)), &hosts, &mut actions))
            .drop_without_applying_deltas();
        assert!(panel.pending.is_some(), "the warning is still waiting");
    }

    /// The first click asks before anything runs; once acknowledged --
    /// and whenever commands are only typed out -- it goes straight
    /// through.
    #[test]
    fn the_warning_comes_before_the_first_run() {
        let command = commands::catalog(System { family: Family::Arch, root: false, ..System::default() }, false).remove(0);
        let mut panel = CommandsPanel::default();
        let mut config = Config::default();
        assert!(config.commands_run && !config.commands_warned, "as it is out of the box");

        let mut actions = Vec::new();
        panel.activate(&command.line, &config, &mut actions);
        assert!(actions.is_empty(), "nothing runs before the warning is acknowledged");
        assert_eq!(panel.pending.as_ref(), Some(&command.line));

        // Acknowledging it runs the command and remembers the answer.
        let mut ui_actions = Vec::new();
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ui_actions.clear();
            ctx.run_ui(click_input(), |ui| panel.warning(ui, &mut ui_actions)).drop_without_applying_deltas();
        }
        assert!(
            matches!(
                ui_actions.as_slice(),
                [
                    SidebarAction::ChangeSetting { setting: Setting::CommandsWarned(true), save: true },
                    SidebarAction::RunCommand(line),
                ] if *line == command.line
            ),
            "acknowledging should save that and run the command"
        );
        assert!(panel.pending.is_none());

        config.commands_warned = true;
        let mut actions = Vec::new();
        panel.activate(&command.line, &config, &mut actions);
        assert!(matches!(actions.as_slice(), [SidebarAction::RunCommand(_)]));

        // Typing it out is harmless, so it never asks.
        config.commands_warned = false;
        config.commands_run = false;
        let mut actions = Vec::new();
        panel.activate(&command.line, &config, &mut actions);
        assert!(matches!(actions.as_slice(), [SidebarAction::RunCommand(_)]));
    }

    #[test]
    fn snippet_form_checks_before_saving() {
        let mut panel = CommandsPanel {
            load_error: Some("test".into()),
            snippets: vec![Snippet { name: "Logs".into(), command: "journalctl -f".into(), ..Snippet::default() }],
            ..CommandsPanel::default()
        };
        let mut commit = |name: &str, command: &str, original| {
            let mut editor = SnippetEditor::new(original, &Snippet::default());
            (editor.name, editor.command) = (name.into(), command.into());
            editor.system = Some(Family::Debian);
            panel.editor = Some(editor);
            panel.commit()
        };
        assert!(commit(" ", "ls", None).unwrap_err().contains("Name"));
        assert!(commit("x", "  \n", None).unwrap_err().contains("Befehl"));
        assert!(commit("Logs", "ls", None).unwrap_err().contains("Logs"));
        // Valid: renaming itself is fine, but the unreadable file refuses the save.
        assert!(commit("Logs", "ls", Some(0)).unwrap_err().contains("snippets.toml"));
        assert_eq!(panel.snippets[0].command, "journalctl -f", "nothing changed");
        let snippet = SnippetEditor { system: Some(Family::Arch), ..SnippetEditor::new(None, &panel.snippets[0]) };
        assert_eq!(snippet.to_snippet().unwrap().system.as_deref(), Some("arch"));

        // Where: local only, a host, everywhere -- and "after login" can't be local.
        let base = SnippetEditor::new(None, &panel.snippets[0]);
        let local = SnippetEditor { place: Place::Local, ..SnippetEditor::new(None, &panel.snippets[0]) }.to_snippet().unwrap();
        assert!(local.local && local.host.is_none());
        assert!(Place::of(&local) == Place::Local);
        let web1 = SnippetEditor { place: Place::Host("web1".into()), ..SnippetEditor::new(None, &panel.snippets[0]) }.to_snippet().unwrap();
        assert!(!web1.local && web1.host.as_deref() == Some("web1"));
        assert!(base.to_snippet().is_ok_and(|snippet| !snippet.local && snippet.host.is_none()));
        let login = SnippetEditor { place: Place::Local, autorun: Some(Autorun::Login), ..SnippetEditor::new(None, &panel.snippets[0]) };
        assert!(login.to_snippet().unwrap_err().contains("Nur lokal"));
    }

    /// A click on the warning's first button ("run it"), where the second
    /// pass lays it out again in the same place.
    fn click_input() -> egui::RawInput {
        let pos = egui::pos2(20.0, 80.0);
        egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            ..egui::RawInput::default()
        }
    }
}
