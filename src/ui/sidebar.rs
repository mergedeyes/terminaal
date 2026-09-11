//! The sidebar left of the console. Its header switches between sections:
//! shell management (here), SSH (`ui::ssh_panel`), keys
//! (`ui::keys_panel`) and settings (`ui::settings_panel`).
//!
//! Shells: pick among the installed shells (open a tab with one, make one
//! the default for new tabs) and edit each shell's aliases and functions
//! (`shells::managed`).
//!
//! Plain egui immediate-mode UI. Anything that needs the rest of the app
//! -- spawning a tab, writing the config -- is handed back as a
//! [`SidebarAction`] instead of being done here.

use egui::{
    Align, Align2, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense, Stroke, TextEdit,
    TextStyle, Ui, pos2, vec2,
};

use crate::config::{Config, Setting};
use crate::i18n::{Language, t};
use crate::shells::managed::{self, Entry, EntryKind};
use crate::shells::{self, InstalledShell, ShellKind};
use crate::ssh::SshTarget;
use crate::ui::keys_panel::KeysPanel;
use crate::ui::settings_panel::SettingsPanel;
use crate::ui::ssh_panel::{SshData, SshPanel};
use crate::ui::theme;
use crate::ui::widgets::{Status, list_row, section_title, tilde, weak};

pub enum SidebarAction {
    /// Open a new tab running this shell.
    OpenTab(InstalledShell),
    /// Make this shell the default for new tabs (persisted in the config).
    SetDefaultShell(InstalledShell),
    /// Open a new tab connected to this host over SSH. Boxed: a target
    /// with all its settings dwarfs the other variants.
    Connect(Box<SshTarget>),
    /// Switch the UI language -- `None`: follow the locale -- and persist
    /// that in the config.
    SetLanguage(Option<Language>),
    /// Apply a config option; with `save`, also persist it. Sliders send
    /// this on every step while dragged, and with `save` once let go.
    ChangeSetting { setting: Setting, save: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Section {
    Shells,
    Ssh,
    Keys,
    Settings,
}

pub struct Sidebar {
    section: Section,
    /// Hosts and keys, shared by the SSH and key sections.
    data: SshData,
    ssh: SshPanel,
    keys: KeysPanel,
    settings: SettingsPanel,

    shells: Vec<InstalledShell>,
    selected: usize,
    /// The selected shell's managed entries as last loaded/saved, or why
    /// they couldn't be read.
    entries: Result<Vec<Entry>, String>,
    /// Which list (aliases or functions) is showing.
    list: EntryKind,
    editor: Option<Editor>,
    /// Entry whose delete button was clicked once; a second click confirms.
    confirm_delete: Option<(EntryKind, String)>,
    status: Option<Status>,
}

struct Editor {
    kind: EntryKind,
    /// Name of the entry being edited; `None` while adding a new one.
    original: Option<String>,
    name: String,
    value: String,
    error: Option<String>,
    /// Put the cursor into the name field on the first frame.
    focus_pending: bool,
}

impl Editor {
    fn new(kind: EntryKind) -> Self {
        Self { kind, original: None, name: String::new(), value: String::new(), error: None, focus_pending: true }
    }

    fn edit(entry: &Entry) -> Self {
        Self {
            kind: entry.kind,
            original: Some(entry.name.clone()),
            name: entry.name.clone(),
            value: entry.value.clone(),
            error: None,
            focus_pending: true,
        }
    }
}

impl Sidebar {
    pub fn new(default: &InstalledShell) -> Self {
        let mut shells = shells::detect();
        // A configured or `$SHELL` shell that isn't in /etc/shells should
        // still show up, or there'd be no way to see which one is default.
        if !shells.iter().any(|s| s.is(default)) {
            shells.insert(0, default.clone());
        }
        let selected = shells.iter().position(|s| s.is(default)).unwrap_or(0);
        let data = SshData::load();
        let mut sidebar = Self {
            section: Section::Shells,
            ssh: SshPanel::new(&data),
            keys: KeysPanel::new(),
            settings: SettingsPanel::default(),
            data,
            shells,
            selected,
            entries: Ok(Vec::new()),
            list: EntryKind::Alias,
            editor: None,
            confirm_delete: None,
            status: None,
        };
        sidebar.load_entries();
        sidebar
    }

    /// Show the outcome of something `app.rs` did for a [`SidebarAction`]
    /// in the section it came from (the one showing).
    pub fn report(&mut self, result: Result<String, String>) {
        match self.section {
            Section::Shells => self.status = Some(Status::from_result(result)),
            Section::Ssh => self.ssh.report(result),
            Section::Keys => self.keys.report(result),
            Section::Settings => self.settings.report(result),
        }
    }

    /// Lay the sidebar out as a left panel of the configured width inside
    /// `ui` (egui's root UI, covering the whole window). `header_height`
    /// is in points; the header lines up with the tab bar next to it.
    /// `window_size` is the window's size in logical pixels.
    /// Returns where the panel's right edge ended up (in points) -- the
    /// console starts there -- plus any actions for the app.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        default: &InstalledShell,
        config: &Config,
        window_size: [f64; 2],
        header_height: f32,
    ) -> (f32, Vec<SidebarAction>) {
        let mut actions = Vec::new();
        let panel = egui::Panel::left("sidebar")
            .exact_size(config.sidebar_width)
            .resizable(false)
            .frame(Frame::new().fill(theme::BG))
            .show(ui, |ui| {
                header(ui, &mut self.section, header_height);
                ScrollArea::vertical().id_salt(self.section).auto_shrink([false; 2]).show(ui, |ui| {
                    Frame::new().inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| match self.section {
                        Section::Shells => self.shells_section(ui, default, &mut actions),
                        Section::Ssh => self.ssh.show(ui, &mut self.data, &mut actions),
                        Section::Keys => self.keys.show(ui, &mut self.data),
                        Section::Settings => {
                            self.settings.show(ui, config, &self.shells, default, window_size, &mut actions)
                        }
                    });
                });
            });
        (panel.response.rect.right(), actions)
    }

    fn shells_section(&mut self, ui: &mut Ui, default: &InstalledShell, actions: &mut Vec<SidebarAction>) {
        self.shell_list(ui, default, actions);
        ui.add_space(18.0);
        self.managed_section(ui);
        if let Some(status) = &self.status {
            ui.add_space(8.0);
            status.show(ui);
        }
    }

    fn selected_shell(&self) -> &InstalledShell {
        &self.shells[self.selected]
    }

    fn load_entries(&mut self) {
        let kind = self.selected_shell().kind;
        self.entries = managed::load(kind).map_err(|err| {
            let path = managed::path_for(kind).map(|p| tilde(&p)).unwrap_or_default();
            t!("common-file-unreadable", path = path, err = err.to_string())
        });
        self.editor = None;
        self.confirm_delete = None;
    }

    fn save(&mut self, entries: Vec<Entry>) -> Result<(), String> {
        managed::save(self.selected_shell().kind, &entries)
            .map_err(|err| t!("shells-save-failed", err = err.to_string()))?;
        self.entries = Ok(entries);
        Ok(())
    }

    fn shell_list(&mut self, ui: &mut Ui, default: &InstalledShell, actions: &mut Vec<SidebarAction>) {
        section_title(ui, &t!("shells-installed"));
        let (badge, hint) = (t!("common-default"), t!("shells-double-click"));
        let mut clicked = None;
        for (i, shell) in self.shells.iter().enumerate() {
            let row = list_row(
                ui,
                &shell.name,
                &shell.path.display().to_string(),
                i == self.selected,
                shell.is(default).then_some(badge.as_str()),
                &hint,
            );
            if row.double_clicked() {
                actions.push(SidebarAction::OpenTab(shell.clone()));
            }
            if row.clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked
            && i != self.selected
        {
            self.selected = i;
            self.status = None;
            self.load_entries();
        }

        let shell = self.selected_shell().clone();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let new_tab = ui.button(t!("shells-new-tab")).on_hover_text(t!("shells-new-tab-hint", shell = &shell.name));
            if new_tab.clicked() {
                actions.push(SidebarAction::OpenTab(shell.clone()));
            }
            let is_default = shell.is(default);
            let button = ui
                .add_enabled(!is_default, egui::Button::new(t!("shells-make-default")))
                .on_hover_text(t!("shells-make-default-hint"));
            if button.clicked() {
                actions.push(SidebarAction::SetDefaultShell(shell.clone()));
            }
        });
    }

    fn managed_section(&mut self, ui: &mut Ui) {
        let shell = self.selected_shell().clone();
        section_title(ui, &t!("shells-managed-title", shell = &shell.name));
        let Some(path) = managed::path_for(shell.kind) else {
            ui.label(weak(t!("shells-managed-unsupported", shell = &shell.name)));
            return;
        };
        ui.label(RichText::new(tilde(&path)).monospace().size(11.0).color(theme::TEXT_WEAK))
            .on_hover_text(t!("shells-managed-file-hint"));
        ui.add_space(6.0);

        let entries = match &self.entries {
            Ok(entries) => entries.clone(),
            Err(err) => {
                ui.label(RichText::new(err).color(theme::ERROR));
                if ui.button(t!("common-reload")).clicked() {
                    self.load_entries();
                }
                return;
            }
        };

        let count = |kind| entries.iter().filter(|e| e.kind == kind).count();
        let before = self.list;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.list, EntryKind::Alias, t!("shells-aliases", count = count(EntryKind::Alias)));
            ui.selectable_value(
                &mut self.list,
                EntryKind::Function,
                t!("shells-functions", count = count(EntryKind::Function)),
            );
        });
        if self.list != before {
            self.editor = None;
            self.confirm_delete = None;
        }
        ui.add_space(4.0);

        let mut edit = None;
        let mut ask_delete = None;
        let mut delete = None;
        let mut cancel_delete = false;
        let shown: Vec<&Entry> = entries.iter().filter(|e| e.kind == self.list).collect();
        if shown.is_empty() && self.editor.is_none() {
            ui.label(weak(match self.list {
                EntryKind::Alias => t!("shells-no-aliases"),
                EntryKind::Function => t!("shells-no-functions"),
            }));
        }
        for entry in shown {
            let key = (entry.kind, entry.name.clone());
            let confirming = self.confirm_delete.as_ref() == Some(&key);
            Frame::new().fill(theme::ROW_BG).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(8, 6)).show(
                ui,
                |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&entry.name).monospace().strong().color(theme::TEXT));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if confirming {
                                if ui.small_button(t!("common-no")).clicked() {
                                    cancel_delete = true;
                                }
                                if ui.small_button(RichText::new(t!("common-delete")).color(theme::ERROR)).clicked() {
                                    delete = Some(key.clone());
                                }
                            } else {
                                if ui.small_button("🗑").on_hover_text(t!("common-delete")).clicked() {
                                    ask_delete = Some(key.clone());
                                }
                                if ui.small_button("✏").on_hover_text(t!("common-edit")).clicked() {
                                    edit = Some(entry.clone());
                                }
                            }
                        });
                    });
                    let mut lines = entry.value.lines();
                    let mut preview = lines.next().unwrap_or_default().to_string();
                    if lines.next().is_some() {
                        preview.push_str("  …");
                    }
                    ui.add(egui::Label::new(RichText::new(preview).monospace().size(12.0).color(theme::TEXT_WEAK)).truncate());
                },
            );
            ui.add_space(2.0);
        }

        if let Some(entry) = edit {
            self.editor = Some(Editor::edit(&entry));
            self.confirm_delete = None;
            self.status = None;
        }
        if let Some(key) = ask_delete {
            self.confirm_delete = Some(key);
        }
        if cancel_delete {
            self.confirm_delete = None;
        }
        if let Some((kind, name)) = delete {
            let mut remaining = entries.clone();
            remaining.retain(|e| !(e.kind == kind && e.name == name));
            let result = self.save(remaining).map(|()| t!("common-deleted", name = &name));
            self.status = Some(Status::from_result(result));
            self.confirm_delete = None;
        }

        ui.add_space(4.0);
        if self.editor.is_some() {
            self.editor_form(ui, &shell);
        } else {
            let label = match self.list {
                EntryKind::Alias => t!("shells-add-alias"),
                EntryKind::Function => t!("shells-add-function"),
            };
            if ui.button(label).clicked() {
                self.editor = Some(Editor::new(self.list));
                self.confirm_delete = None;
                self.status = None;
            }
        }
    }

    fn editor_form(&mut self, ui: &mut Ui, shell: &InstalledShell) {
        let Some(editor) = self.editor.as_mut() else { return };
        let mut save = false;
        let mut cancel = false;
        Frame::group(ui.style()).fill(theme::ROW_BG).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = match (editor.kind, editor.original.is_some()) {
                (EntryKind::Alias, false) => t!("shells-new-alias"),
                (EntryKind::Alias, true) => t!("shells-edit-alias"),
                (EntryKind::Function, false) => t!("shells-new-function"),
                (EntryKind::Function, true) => t!("shells-edit-function"),
            };
            ui.label(RichText::new(title).strong());
            ui.add_space(2.0);

            ui.label(weak("Name"));
            let name = ui.add(
                TextEdit::singleline(&mut editor.name)
                    .font(TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .hint_text(if editor.kind == EntryKind::Alias { "ll" } else { "mkcd" }),
            );
            if editor.focus_pending {
                name.request_focus();
                editor.focus_pending = false;
            }

            match editor.kind {
                EntryKind::Alias => {
                    ui.label(weak(t!("shells-command")));
                    let command = ui.add(
                        TextEdit::singleline(&mut editor.value)
                            .font(TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .hint_text("ls -la"),
                    );
                    if command.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        save = true;
                    }
                }
                EntryKind::Function => {
                    ui.label(weak(t!("shells-body")));
                    let (hint, args) = match shell.kind {
                        ShellKind::Fish => ("mkdir -p $argv[1]; and cd $argv[1]", t!("shells-args-fish")),
                        _ => ("mkdir -p \"$1\" && cd \"$1\"", t!("shells-args-posix")),
                    };
                    ui.add(
                        TextEdit::multiline(&mut editor.value)
                            .code_editor()
                            .desired_rows(6)
                            .desired_width(f32::INFINITY)
                            .hint_text(hint),
                    );
                    ui.label(weak(args).size(11.0));
                }
            }

            if let Some(err) = &editor.error {
                ui.label(RichText::new(err).color(theme::ERROR));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(t!("common-save")).clicked() {
                    save = true;
                }
                if ui.button(t!("common-cancel")).clicked() {
                    cancel = true;
                }
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

    /// Validate the editor's contents and write them to the managed file.
    fn commit(&mut self) -> Result<String, String> {
        let Some(editor) = &self.editor else { return Err(t!("shells-no-entry-open")) };
        let kind = editor.kind;
        let original = editor.original.clone();
        let name = editor.name.trim().to_string();
        let value = match kind {
            EntryKind::Alias => editor.value.trim().to_string(),
            EntryKind::Function => editor.value.trim_end().to_string(),
        };

        managed::validate_name(&name)?;
        if value.trim().is_empty() {
            return Err(match kind {
                EntryKind::Alias => t!("shells-command-missing"),
                EntryKind::Function => t!("shells-body-missing"),
            });
        }

        let mut entries = self.entries.clone()?;
        let is_original = |e: &Entry| e.kind == kind && original.as_deref() == Some(e.name.as_str());
        // One namespace for both: a same-named alias would shadow the
        // function (bash/zsh) or replace it (fish, where aliases are
        // functions).
        if let Some(clash) = entries.iter().find(|e| e.name == name && !is_original(e)) {
            return Err(t!("shells-name-taken", name = &name, kind = clash.kind.label()));
        }
        let entry = Entry { kind, name: name.clone(), value };
        match entries.iter().position(is_original) {
            Some(i) => entries[i] = entry,
            None => entries.push(entry),
        }
        self.save(entries)?;
        Ok(t!("shells-saved", name = &name, shell = &self.selected_shell().name))
    }
}

/// Top strip with one tab per section -- as high as the tab bar next to
/// it, with the same bottom border -- the active one underlined in the
/// accent color. Settings is a narrow gear at the end.
fn header(ui: &mut Ui, section: &mut Section, height: f32) {
    const TAB_WIDTH: f32 = 72.0;
    const ICON_TAB_WIDTH: f32 = 40.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    let keys = t!("sidebar-keys");
    let tabs = [
        (Section::Shells, "Shells", TAB_WIDTH),
        (Section::Ssh, "SSH", TAB_WIDTH),
        (Section::Keys, keys.as_str(), TAB_WIDTH),
        (Section::Settings, "⚙", ICON_TAB_WIDTH),
    ];
    let mut left = rect.left() + 4.0;
    for (i, (tab, label, width)) in tabs.into_iter().enumerate() {
        let tab_rect = Rect::from_min_size(pos2(left, rect.top()), vec2(width, height));
        left += width;
        let mut response = ui
            .interact(tab_rect, ui.id().with(("sidebar-section", i)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if tab == Section::Settings {
            response = response.on_hover_text(t!("sidebar-settings"));
        }
        if response.clicked() {
            *section = tab;
        }
        let active = *section == tab;
        let painter = ui.painter();
        if response.hovered() && !active {
            painter.rect_filled(tab_rect.shrink2(vec2(2.0, 6.0)), CornerRadius::same(4), theme::HOVER_BG);
        }
        let color = if active { theme::TEXT } else { theme::TEXT_WEAK };
        painter.text(tab_rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), color);
        if active {
            let underline = Rect::from_min_size(pos2(tab_rect.left() + 8.0, rect.bottom() - 2.0), vec2(width - 16.0, 2.0));
            painter.rect_filled(underline, CornerRadius::same(0), theme::ACCENT);
        }
    }
}
