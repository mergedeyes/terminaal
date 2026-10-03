//! The files tab of an SSH connection (`sftp`): this machine's folders on
//! the left, the server's on the right, synced folders, transfers and files
//! being edited locally below. Both lists pick several entries
//! (Ctrl+click, Shift+click) for uploading, downloading and deleting.
//!
//! The panel only lays out what the session's [`State`] says and turns
//! clicks into [`FilesAction`]s; `app.rs` passes them to the session, or --
//! for sudo -- into the terminal the connection belongs to. The local side
//! is read here, when a folder is opened or reloaded (and after a download
//! finished), never per frame.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{Align, Align2, CornerRadius, FontId, Frame, Key, Layout, Margin, RichText, ScrollArea, Sense, TextEdit, Ui, vec2};

use crate::i18n::t;
use crate::sftp::edit::{EditAction, EditState, EditStatus, SudoFor};
use crate::sftp::session::{Command, Connection, Direction, State, TransferState, TransferStatus, join, parent};
use crate::sftp::sync::{SyncState, SyncStatus};
use crate::ui::theme;
use crate::ui::widgets::{Status, section_title, tilde, weak};

const ROW_HEIGHT: f32 = 22.0;
const GAP: f32 = 12.0;
/// A synced pair's notes shown at most; the rest are counted.
const SYNC_NOTES: usize = 6;

pub enum FilesAction {
    Remote(Command),
    /// For the host's background sync session (live pairs).
    Background(Command),
    /// Run this sudo command in the connection's terminal, then tell the
    /// session it's running.
    RunSudo { edit: u64, command: String },
    /// Open the SFTP connection anew.
    Reconnect,
}

/// What the tab shows, lent by `app.rs` for one pass.
pub struct FilesView<'a> {
    pub state: &'a State,
    /// `user@host`.
    pub label: &'a str,
    /// The terminal of the connection is still open (sudo runs there).
    pub terminal: bool,
    /// The configured editor, `None`: the desktop's default.
    pub editor: Option<&'a str>,
    /// The host's background sync session, if it has live pairs.
    pub background: Option<&'a State>,
}

/// The entries picked in one list: a click picks one, Ctrl+click adds or
/// removes one, Shift+click picks the range from the last click.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Picked {
    /// In the order they were picked.
    names: Vec<String>,
    /// Where a Shift+click range starts.
    anchor: Option<String>,
}

impl Picked {
    /// `listed`: the list's names in their order.
    fn click(&mut self, name: &str, listed: &[&str], ctrl: bool, shift: bool) {
        let at = |name: &str| listed.iter().position(|listed| *listed == name);
        if shift && let Some((from, to)) = self.anchor.as_deref().and_then(at).zip(at(name)) {
            let range = &listed[from.min(to)..=from.max(to)];
            if !ctrl {
                self.names.clear();
            }
            for name in range {
                if !self.contains(name) {
                    self.names.push(name.to_string());
                }
            }
            return;
        }
        if ctrl {
            match self.names.iter().position(|picked| picked == name) {
                Some(i) => drop(self.names.remove(i)),
                None => self.names.push(name.to_string()),
            }
        } else {
            self.names = vec![name.to_string()];
        }
        self.anchor = Some(name.to_string());
    }

    fn contains(&self, name: &str) -> bool {
        self.names.iter().any(|picked| picked == name)
    }

    fn clear(&mut self) {
        self.names.clear();
        self.anchor = None;
    }

    fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The one entry picked, if it's exactly one.
    fn only(&self) -> Option<&str> {
        match self.names.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    /// Forget what isn't listed any more.
    fn keep_listed(&mut self, listed: &[&str]) {
        self.names.retain(|name| listed.contains(&name.as_str()));
        if self.anchor.as_deref().is_some_and(|anchor| !listed.contains(&anchor)) {
            self.anchor = None;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalEntry {
    name: String,
    dir: bool,
    size: u64,
    modified: Option<SystemTime>,
}

pub struct FilesPanel {
    local_dir: PathBuf,
    local_entries: Result<Vec<LocalEntry>, String>,
    local_selected: Picked,
    remote_selected: Picked,
    /// The server folder the remote selection is in.
    remote_dir: String,
    /// Path fields while they're being typed into.
    local_path: Option<String>,
    remote_path: Option<String>,
    /// Renaming this remote entry: its name and the new one so far.
    rename: Option<(String, String)>,
    new_folder: Option<String>,
    /// Asked once about these, the next click deletes them.
    confirm_delete: Option<Vec<String>>,
    /// The same for local entries, which go to the trash.
    confirm_trash: Option<Vec<String>>,
    /// The outcome of the last local action.
    local_report: Option<Result<String, String>>,
    confirm_close: Option<u64>,
    /// Finished downloads and sync runs last frame: more means the local
    /// side changed.
    downloads_done: usize,
    syncs_done: u64,
}

impl FilesPanel {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let mut panel = Self {
            local_dir: home.clone(),
            local_entries: Ok(Vec::new()),
            local_selected: Picked::default(),
            remote_selected: Picked::default(),
            remote_dir: String::new(),
            local_path: None,
            remote_path: None,
            rename: None,
            new_folder: None,
            confirm_delete: None,
            confirm_close: None,
            confirm_trash: None,
            local_report: None,
            downloads_done: 0,
            syncs_done: 0,
        };
        panel.open_local(home);
        panel
    }

    fn open_local(&mut self, dir: PathBuf) {
        match read_local(&dir) {
            Ok(entries) => {
                if dir != self.local_dir {
                    self.local_selected.clear();
                    self.confirm_trash = None;
                }
                let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
                self.local_selected.keep_listed(&names);
                self.local_entries = Ok(entries);
                self.local_dir = dir;
            }
            Err(err) => self.local_entries = Err(t!("files-local-failed", path = tilde(&dir), err = err.to_string())),
        }
    }

    /// Any edit waiting for the user: marked in the tab's title.
    pub fn needs_attention(state: &State) -> bool {
        state.edits.iter().any(|edit| {
            matches!(edit.state, EditState::Conflict | EditState::Denied(_) | EditState::SudoReady(_) | EditState::Failed(_))
        })
    }

    /// The whole tab, filling `ui`'s max rect.
    pub fn show_tab(&mut self, ui: &mut Ui, view: &FilesView, actions: &mut Vec<FilesAction>) {
        ui.painter().rect_filled(ui.max_rect(), CornerRadius::ZERO, theme::colors().panel);
        let done = view.state.transfers.iter().filter(|t| t.direction == Direction::Download && t.is_over()).count();
        let syncs_done = view.state.syncs_finished + view.background.map_or(0, |state| state.syncs_finished);
        if done != self.downloads_done || syncs_done != self.syncs_done {
            self.downloads_done = done;
            self.syncs_done = syncs_done;
            self.open_local(self.local_dir.clone());
        }
        Frame::new().inner_margin(Margin::symmetric(16, 10)).show(ui, |ui| {
            self.header(ui, view, actions);
            if !view.state.attached {
                return;
            }
            ui.add_space(6.0);
            // Room below for transfers and edits only when there are any.
            let syncs = !view.state.syncs.is_empty() || view.background.is_some_and(|state| !state.syncs.is_empty());
            let below = syncs || !view.state.transfers.is_empty() || !view.state.edits.is_empty();
            let share = if below { 0.55 } else { 1.0 };
            // Path, buttons and a prompt line above each list.
            let lists_height = ((ui.available_height() - 110.0) * share).max(120.0);
            ui.columns(2, |columns| {
                columns[0].set_min_height(lists_height);
                columns[1].set_min_height(lists_height);
                self.local_side(&mut columns[0], view, lists_height, actions);
                self.remote_side(&mut columns[1], view, lists_height, actions);
            });
            ui.add_space(GAP);
            ScrollArea::vertical().id_salt("files-below").auto_shrink([false, true]).show(ui, |ui| {
                self.syncs(ui, view, actions);
                self.transfers(ui, view, actions);
                self.edits(ui, view, actions);
            });
        });
    }

    fn header(&mut self, ui: &mut Ui, view: &FilesView, actions: &mut Vec<FilesAction>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("files-title", host = view.label)).size(16.0).strong());
            match &view.state.connection {
                Connection::Waiting if !view.terminal => {
                    ui.label(RichText::new(t!("files-terminal-closed")).color(theme::colors().error));
                }
                Connection::Waiting if view.state.attached => {
                    ui.spinner();
                    ui.label(RichText::new(t!("files-waiting-reconnect")).color(theme::colors().error));
                }
                Connection::Waiting => {
                    ui.spinner();
                    ui.label(weak(t!("files-waiting-login")));
                }
                Connection::Ready => {}
                Connection::Failed(err) => {
                    ui.label(RichText::new(err).color(theme::colors().error));
                    if ui.button(t!("files-reconnect")).clicked() {
                        actions.push(FilesAction::Reconnect);
                    }
                }
            }
        });
        if let Some(report) = &view.state.report {
            Status::from_result(report.clone()).show(ui);
        }
    }

    fn local_side(&mut self, ui: &mut Ui, view: &FilesView, height: f32, actions: &mut Vec<FilesAction>) {
        section_title(ui, &t!("files-local"));
        let shown = tilde(&self.local_dir);
        if let Some(dir) = path_bar(ui, "files-local-path", &mut self.local_path, &shown) {
            let dir = expand(&dir);
            self.open_local(dir);
        }
        ui.horizontal(|ui| {
            if ui.button("⬆").on_hover_text(t!("files-up")).clicked()
                && let Some(up) = self.local_dir.parent()
            {
                self.open_local(up.to_path_buf());
            }
            if ui.button("⟳").on_hover_text(t!("files-reload")).clicked() {
                self.open_local(self.local_dir.clone());
            }
            let ready = view.state.connection == Connection::Ready;
            let upload = ui.add_enabled(ready && !self.local_selected.is_empty(), egui::Button::new(t!("files-upload")));
            if upload.clicked() {
                for name in &self.local_selected.names {
                    let local = self.local_dir.join(name);
                    actions.push(FilesAction::Remote(Command::Upload { local, remote_dir: view.state.dir.clone() }));
                }
            }
            let picked = !self.local_selected.is_empty();
            let asked = picked && self.confirm_trash.as_ref() == Some(&self.local_selected.names);
            let label = if asked { t!("files-delete-confirm") } else { t!("files-delete") };
            if ui.add_enabled(picked, egui::Button::new(label)).on_hover_text(t!("files-trash-hint")).clicked() {
                if asked {
                    let paths: Vec<PathBuf> = self.local_selected.names.iter().map(|name| self.local_dir.join(name)).collect();
                    self.local_report = Some(trash(&paths));
                    self.confirm_trash = None;
                    self.local_selected.clear();
                    self.open_local(self.local_dir.clone());
                } else {
                    self.confirm_trash = Some(self.local_selected.names.clone());
                }
            }
            picked_count(ui, &self.local_selected);
        });
        if let Some(report) = &self.local_report {
            Status::from_result(report.clone()).show(ui);
        }
        let entries = match &self.local_entries {
            Ok(entries) => entries.clone(),
            Err(err) => {
                ui.label(RichText::new(err).color(theme::colors().error));
                Vec::new()
            }
        };
        let mut open = None;
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        list_frame(ui, height, "files-local-list", |ui| {
            for entry in &entries {
                let selected = self.local_selected.contains(&entry.name);
                let row = entry_row(ui, &entry.name, entry.dir, entry.size, entry.modified.and_then(unix_secs), selected);
                if row.clicked() {
                    let modifiers = ui.input(|i| i.modifiers);
                    self.local_selected.click(&entry.name, &names, modifiers.command, modifiers.shift);
                    if self.confirm_trash.as_ref() != Some(&self.local_selected.names) {
                        self.confirm_trash = None;
                    }
                }
                if row.double_clicked() && entry.dir {
                    open = Some(self.local_dir.join(&entry.name));
                }
            }
        });
        if let Some(dir) = open {
            self.open_local(dir);
        }
    }

    fn remote_side(&mut self, ui: &mut Ui, view: &FilesView, height: f32, actions: &mut Vec<FilesAction>) {
        let state = view.state;
        let ready = state.connection == Connection::Ready;
        section_title(ui, &t!("files-remote", host = view.label));
        let names: Vec<&str> = state.entries.iter().map(|entry| entry.name.as_str()).collect();
        if self.remote_dir != state.dir {
            self.remote_dir = state.dir.clone();
            self.remote_selected.clear();
            self.confirm_delete = None;
        }
        self.remote_selected.keep_listed(&names);
        if let Some(dir) = path_bar(ui, "files-remote-path", &mut self.remote_path, &state.dir) {
            actions.push(FilesAction::Remote(Command::List(Some(dir))));
        }
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(ready, |ui| {
                if ui.button("⬆").on_hover_text(t!("files-up")).clicked() {
                    actions.push(FilesAction::Remote(Command::List(Some(parent(&state.dir)))));
                }
                if ui.button("~").on_hover_text(t!("files-home")).clicked() {
                    actions.push(FilesAction::Remote(Command::List(Some(state.home.clone()))));
                }
                if ui.button("⟳").on_hover_text(t!("files-reload")).clicked() {
                    actions.push(FilesAction::Remote(Command::List(None)));
                }
                let picked: Vec<_> =
                    state.entries.iter().filter(|entry| self.remote_selected.contains(&entry.name)).collect();
                if ui.add_enabled(!picked.is_empty(), egui::Button::new(t!("files-download"))).clicked() {
                    for entry in &picked {
                        let remote = join(&state.dir, &entry.name);
                        actions.push(FilesAction::Remote(Command::Download { remote, local_dir: self.local_dir.clone() }));
                    }
                }
                let entry = self.remote_selected.only().and_then(|name| state.entries.iter().find(|e| e.name == name));
                let file = entry.filter(|e| !e.attrs.is_dir());
                let edit = ui.add_enabled(file.is_some(), egui::Button::new(t!("files-edit")));
                if edit.on_hover_text(t!("files-edit-hint")).clicked()
                    && let Some(entry) = file
                {
                    actions.push(edit_command(&join(&state.dir, &entry.name), view.editor));
                }
                if ui.add_enabled(entry.is_some(), egui::Button::new(t!("files-rename"))).clicked()
                    && let Some(entry) = entry
                {
                    self.rename = Some((entry.name.clone(), entry.name.clone()));
                }
                if ui.button(t!("files-new-folder")).clicked() {
                    self.new_folder = Some(String::new());
                }
                let asked = !picked.is_empty() && self.confirm_delete.as_ref() == Some(&self.remote_selected.names);
                let label = if asked { t!("files-delete-confirm") } else { t!("files-delete") };
                let delete = ui.add_enabled(!picked.is_empty(), egui::Button::new(label));
                if delete.on_hover_text(t!("files-delete-hint")).clicked() {
                    if asked {
                        let paths = picked.iter().map(|entry| (join(&state.dir, &entry.name), entry.attrs.is_dir())).collect();
                        actions.push(FilesAction::Remote(Command::Remove(paths)));
                        self.confirm_delete = None;
                        self.remote_selected.clear();
                    } else {
                        self.confirm_delete = Some(self.remote_selected.names.clone());
                    }
                }
            });
            picked_count(ui, &self.remote_selected);
        });
        self.name_prompts(ui, state, actions);
        if let Some(err) = &state.listing_error {
            ui.label(RichText::new(err).color(theme::colors().error));
        }
        let mut open = None;
        list_frame(ui, height, "files-remote-list", |ui| {
            for entry in &state.entries {
                let selected = self.remote_selected.contains(&entry.name);
                let dir = entry.attrs.is_dir();
                let row = entry_row(ui, &entry.name, dir, entry.attrs.size.unwrap_or(0), entry.attrs.mtime().map(u64::from), selected);
                if row.clicked() {
                    let modifiers = ui.input(|i| i.modifiers);
                    self.remote_selected.click(&entry.name, &names, modifiers.command, modifiers.shift);
                    if self.confirm_delete.as_ref() != Some(&self.remote_selected.names) {
                        self.confirm_delete = None;
                    }
                }
                if row.double_clicked() {
                    open = Some(entry.clone());
                }
            }
        });
        if let Some(entry) = open.filter(|_| ready) {
            let path = join(&state.dir, &entry.name);
            if entry.attrs.is_dir() {
                actions.push(FilesAction::Remote(Command::List(Some(path))));
            } else {
                // A symlink may lead to a folder: the session looks.
                let editor = view.editor.map(str::to_string);
                actions.push(FilesAction::Remote(Command::Open { path, editor }));
            }
        }
    }

    /// Typing a new name, for a rename or a new folder.
    fn name_prompts(&mut self, ui: &mut Ui, state: &State, actions: &mut Vec<FilesAction>) {
        if let Some((from, to)) = &mut self.rename {
            let mut done = None;
            ui.horizontal(|ui| {
                ui.label(t!("files-rename-to", name = from.as_str()));
                let field = ui.add(TextEdit::singleline(to).desired_width(180.0));
                let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (ui.button(t!("files-ok")).clicked() || submitted) && !to.trim().is_empty() {
                    done = Some(true);
                }
                if ui.button(t!("files-cancel")).clicked() {
                    done = Some(false);
                }
            });
            match done {
                Some(true) => {
                    let (from, to) = self.rename.take().expect("renaming");
                    if from != to.trim() && !to.contains('/') {
                        actions.push(FilesAction::Remote(Command::Rename {
                            from: join(&state.dir, &from),
                            to: join(&state.dir, to.trim()),
                        }));
                        self.remote_selected.names = vec![to.trim().to_string()];
                    }
                }
                Some(false) => self.rename = None,
                None => {}
            }
        }
        if let Some(name) = &mut self.new_folder {
            let mut done = None;
            ui.horizontal(|ui| {
                ui.label(t!("files-new-folder-name"));
                let field = ui.add(TextEdit::singleline(name).desired_width(180.0));
                let submitted = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (ui.button(t!("files-ok")).clicked() || submitted) && !name.trim().is_empty() && !name.contains('/') {
                    done = Some(true);
                }
                if ui.button(t!("files-cancel")).clicked() {
                    done = Some(false);
                }
            });
            match done {
                Some(true) => {
                    let name = self.new_folder.take().expect("naming");
                    actions.push(FilesAction::Remote(Command::Mkdir(name.trim().to_string())));
                }
                Some(false) => self.new_folder = None,
                None => {}
            }
        }
    }

    /// The pairs synced on opening (this session's) and in the background.
    fn syncs(&mut self, ui: &mut Ui, view: &FilesView, actions: &mut Vec<FilesAction>) {
        let own = view.state.syncs.iter().map(|status| (status, false));
        let background = view.background.into_iter().flat_map(|state| state.syncs.iter().map(|status| (status, true)));
        let pairs: Vec<(&SyncStatus, bool)> = own.chain(background).collect();
        if pairs.is_empty() {
            return;
        }
        section_title(ui, &t!("files-syncs"));
        for (status, background) in pairs {
            let send = |command| if background { FilesAction::Background(command) } else { FilesAction::Remote(command) };
            Frame::new().inner_margin(Margin::symmetric(0, 3)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let text = format!("{} {} {}", status.local, status.direction.arrow(), status.remote);
                    ui.label(RichText::new(text).monospace()).on_hover_text(status.direction.label());
                    if status.live {
                        ui.label(RichText::new(t!("files-sync-live")).color(theme::colors().accent).size(11.0))
                            .on_hover_text(t!("files-sync-live-hint"));
                    }
                    let (text, color) = sync_state_text(&status.state);
                    if status.running() {
                        ui.spinner();
                    }
                    ui.label(RichText::new(text).color(color));
                    if status.running() {
                        if ui.small_button(t!("files-sync-stop")).clicked() {
                            actions.push(send(Command::StopSync(status.index)));
                        }
                    } else if ui.small_button(t!("files-sync-now")).clicked() {
                        actions.push(send(Command::SyncNow(Some(status.index))));
                    }
                });
                for note in status.notes.iter().take(SYNC_NOTES) {
                    ui.label(RichText::new(note).color(theme::colors().error).size(12.0));
                }
                if status.notes.len() > SYNC_NOTES {
                    ui.label(weak(t!("files-sync-more", count = status.notes.len() - SYNC_NOTES)).size(12.0));
                }
            });
        }
        ui.add_space(GAP);
    }

    fn transfers(&mut self, ui: &mut Ui, view: &FilesView, actions: &mut Vec<FilesAction>) {
        let transfers = &view.state.transfers;
        if transfers.is_empty() {
            return;
        }
        ui.horizontal(|ui| {
            section_title(ui, &t!("files-transfers"));
            if transfers.iter().any(TransferStatus::is_over) && ui.small_button(t!("files-clear-transfers")).clicked() {
                actions.push(FilesAction::Remote(Command::ClearTransfers));
            }
        });
        for transfer in transfers {
            ui.horizontal(|ui| {
                let arrow = match transfer.direction {
                    Direction::Download => "⬇",
                    Direction::Upload => "⬆",
                };
                ui.label(arrow);
                ui.add(egui::Label::new(RichText::new(&transfer.name).monospace()).truncate()).on_hover_text(&transfer.name);
                match &transfer.state {
                    TransferState::Queued | TransferState::Running => {
                        let fraction = if transfer.total == 0 { 0.0 } else { transfer.done as f32 / transfer.total as f32 };
                        let text = format!("{} / {}", size_text(transfer.done), size_text(transfer.total));
                        ui.add(egui::ProgressBar::new(fraction.min(1.0)).desired_width(160.0).text(text));
                        if view.state.connection != Connection::Ready {
                            ui.label(weak(t!("files-transfer-waiting")));
                        }
                        if ui.small_button("×").on_hover_text(t!("files-cancel-transfer")).clicked() {
                            actions.push(FilesAction::Remote(Command::Cancel(transfer.id)));
                        }
                    }
                    TransferState::Done => drop(ui.label(RichText::new(t!("files-transfer-done")).color(theme::colors().success))),
                    TransferState::Cancelled => drop(ui.label(weak(t!("files-transfer-cancelled")))),
                    TransferState::Failed(err) => drop(ui.label(RichText::new(err).color(theme::colors().error))),
                }
            });
        }
        ui.add_space(GAP);
    }

    fn edits(&mut self, ui: &mut Ui, view: &FilesView, actions: &mut Vec<FilesAction>) {
        if view.state.edits.is_empty() {
            return;
        }
        section_title(ui, &t!("files-edits"));
        for edit in &view.state.edits {
            Frame::new().inner_margin(Margin::symmetric(0, 4)).show(ui, |ui| self.edit_row(ui, view, edit, actions));
        }
        ui.label(weak(t!("files-edits-note")).size(11.0));
    }

    fn edit_row(&mut self, ui: &mut Ui, view: &FilesView, edit: &EditStatus, actions: &mut Vec<FilesAction>) {
        let colors = theme::colors();
        let act = |action| FilesAction::Remote(Command::EditAction { id: edit.id, action });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&edit.remote).monospace()).on_hover_text(t!("files-edit-local", path = tilde(&edit.local)));
            let (text, color) = match &edit.state {
                EditState::Synced => (t!("files-edit-synced"), colors.success),
                EditState::Uploading => (t!("files-edit-uploading"), colors.text_weak),
                EditState::Conflict => (t!("files-edit-conflict-state"), colors.error),
                EditState::Denied(SudoFor::Open) => (t!("files-edit-denied-read"), colors.error),
                EditState::Denied(SudoFor::Save) => (t!("files-edit-denied-write"), colors.error),
                EditState::SudoReady(_) => (t!("files-sudo-ready"), colors.accent),
                EditState::SudoRunning(_) => (t!("files-sudo-running"), colors.text_weak),
                EditState::Failed(err) => (err.clone(), colors.error),
            };
            ui.label(RichText::new(text).color(color));
        });
        ui.horizontal_wrapped(|ui| {
            match &edit.state {
                EditState::Conflict => {
                    if ui.button(t!("files-edit-overwrite")).on_hover_text(t!("files-edit-overwrite-hint")).clicked() {
                        actions.push(act(EditAction::Overwrite));
                    }
                    if ui.button(t!("files-edit-take-theirs")).on_hover_text(t!("files-edit-take-theirs-hint")).clicked() {
                        actions.push(act(EditAction::TakeTheirs));
                    }
                }
                EditState::Denied(_) => {
                    if ui.button(t!("files-sudo-use")).on_hover_text(t!("files-sudo-use-hint")).clicked() {
                        actions.push(act(EditAction::UseSudo));
                    }
                }
                EditState::SudoReady(_) => {
                    if let Some(command) = &edit.command {
                        ui.label(RichText::new(command.trim()).monospace());
                        let run = ui.add_enabled(view.terminal, egui::Button::new(t!("files-sudo-run")));
                        let run = if view.terminal { run.on_hover_text(t!("files-sudo-run-hint")) } else { run.on_disabled_hover_text(t!("files-sudo-no-terminal")) };
                        if run.clicked() {
                            actions.push(FilesAction::RunSudo { edit: edit.id, command: command.clone() });
                        }
                    }
                    if ui.button(t!("files-cancel")).clicked() {
                        actions.push(act(EditAction::CancelSudo));
                    }
                }
                EditState::SudoRunning(_) => {
                    ui.spinner();
                    if ui.button(t!("files-cancel")).clicked() {
                        actions.push(act(EditAction::CancelSudo));
                    }
                }
                EditState::Synced | EditState::Uploading | EditState::Failed(_) => {}
            }
            if !matches!(edit.state, EditState::Denied(SudoFor::Open) | EditState::SudoReady(SudoFor::Open) | EditState::SudoRunning(SudoFor::Open))
                && ui.button(t!("files-edit-reopen")).clicked()
            {
                actions.push(act(EditAction::Reopen));
            }
            let asked = self.confirm_close == Some(edit.id);
            let label = if asked && edit.unsaved { t!("files-edit-close-unsaved") } else { t!("files-edit-close") };
            if ui.button(label).on_hover_text(t!("files-edit-close-hint")).clicked() {
                if edit.unsaved && !asked {
                    self.confirm_close = Some(edit.id);
                } else {
                    self.confirm_close = None;
                    actions.push(act(EditAction::Close));
                }
            }
        });
    }
}

impl Default for FilesPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Into the desktop's trash, with `gio trash` (GLib's, on nearly every
/// desktop) -- it knows the trash of other disks too.
fn trash(paths: &[PathBuf]) -> Result<String, String> {
    let output = std::process::Command::new("gio").arg("trash").arg("--").args(paths).output();
    match output {
        Ok(output) if output.status.success() => Ok(match paths {
            [one] => t!("files-trashed", name = one.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()),
            many => t!("files-trashed-many", count = many.len()),
        }),
        Ok(output) => Err(t!("files-trash-failed", err = String::from_utf8_lossy(&output.stderr).trim().to_string())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(t!("files-trash-no-gio")),
        Err(err) => Err(t!("files-trash-failed", err = err.to_string())),
    }
}

/// "3 selected" next to a list's buttons, once there's more than one.
fn picked_count(ui: &mut Ui, picked: &Picked) {
    if picked.names.len() > 1 {
        ui.label(weak(t!("files-selected", count = picked.names.len()))).on_hover_text(t!("files-select-hint"));
    }
}

fn sync_state_text(state: &SyncState) -> (String, egui::Color32) {
    let colors = theme::colors();
    match state {
        SyncState::Waiting => (t!("files-sync-waiting"), colors.text_weak),
        SyncState::Scanning => (t!("files-sync-scanning"), colors.text_weak),
        SyncState::Copying { done, total } => (t!("files-sync-copying", done = *done, total = *total), colors.text_weak),
        SyncState::Done { at, copied: 0, deleted: 0 } => {
            (t!("files-sync-done", time = unix_secs(*at).map(date_text).unwrap_or_default()), colors.success)
        }
        SyncState::Done { at, copied, deleted } => {
            let time = unix_secs(*at).map(date_text).unwrap_or_default();
            (t!("files-sync-done-changes", time = time, copied = *copied, deleted = *deleted), colors.success)
        }
        SyncState::Busy => (t!("files-sync-busy"), colors.text_weak),
        SyncState::Stopped => (t!("files-sync-stopped"), colors.text_weak),
        SyncState::Failed(err) => (err.clone(), colors.error),
    }
}

fn edit_command(path: &str, editor: Option<&str>) -> FilesAction {
    FilesAction::Remote(Command::Edit { path: path.to_string(), editor: editor.map(str::to_string) })
}

/// The path of a side: shown, and typed into on click. Returns a path to
/// open once Enter is pressed.
fn path_bar(ui: &mut Ui, id: &str, editing: &mut Option<String>, shown: &str) -> Option<String> {
    let mut text = editing.clone().unwrap_or_else(|| shown.to_string());
    let field = ui.add(TextEdit::singleline(&mut text).id_salt(id).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
    if field.has_focus() {
        *editing = Some(text.clone());
    }
    if field.lost_focus() {
        *editing = None;
        if ui.input(|i| i.key_pressed(Key::Enter)) && text.trim() != shown {
            return Some(text.trim().to_string());
        }
    }
    None
}

fn list_frame(ui: &mut Ui, height: f32, id: &str, content: impl FnOnce(&mut Ui)) {
    Frame::new().fill(theme::colors().bg).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(4)).show(ui, |ui| {
        ScrollArea::vertical().id_salt(id).max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(
            ui,
            |ui| {
                ui.with_layout(Layout::top_down(Align::Min), content);
            },
        );
    });
}

/// One entry: icon and name on the left, modified date and size on the right.
fn entry_row(ui: &mut Ui, name: &str, dir: bool, size: u64, modified: Option<u64>, selected: bool) -> egui::Response {
    let colors = theme::colors();
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, CornerRadius::same(3), colors.selected);
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(3), colors.hover);
    }
    let y = rect.center().y;
    // Drawn: egui's fonts have no folder or file symbols.
    let icon = egui::Rect::from_center_size(egui::pos2(rect.left() + 13.0, y), vec2(12.0, 10.0));
    if dir {
        let tab = egui::Rect::from_min_size(icon.left_top() - vec2(0.0, 2.0), vec2(5.0, 3.0));
        painter.rect_filled(tab, CornerRadius::same(1), colors.accent);
        painter.rect_filled(icon, CornerRadius::same(2), colors.accent);
    } else {
        let page = egui::Rect::from_center_size(icon.center(), vec2(9.0, 12.0));
        painter.rect_stroke(page, CornerRadius::same(1), egui::Stroke::new(1.0, colors.text_weak), egui::StrokeKind::Inside);
    }
    let right = if dir { String::new() } else { size_text(size) };
    let date = modified.map(date_text).unwrap_or_default();
    painter.text(egui::pos2(rect.right() - 6.0, y), Align2::RIGHT_CENTER, &right, FontId::proportional(12.0), colors.text_weak);
    let date_x = rect.right() - 80.0;
    let narrow = rect.width() < 360.0;
    if !narrow {
        painter.text(egui::pos2(date_x, y), Align2::RIGHT_CENTER, &date, FontId::proportional(12.0), colors.text_weak);
    }
    let name_right = if narrow { rect.right() - 80.0 } else { date_x - 130.0 };
    let name_painter = ui.painter_at(egui::Rect::from_min_max(rect.min, egui::pos2(name_right, rect.max.y)));
    name_painter.text(egui::pos2(rect.left() + 26.0, y), Align2::LEFT_CENTER, name, FontId::proportional(13.0), colors.text);
    response.on_hover_text(name)
}

/// `~/x` and `~` as the home folder.
fn expand(path: &str) -> PathBuf {
    crate::ssh::expand_tilde(path)
}

fn read_local(dir: &Path) -> std::io::Result<Vec<LocalEntry>> {
    let mut entries: Vec<LocalEntry> = std::fs::read_dir(dir)?
        .flatten()
        .map(|entry| {
            let meta = std::fs::metadata(entry.path()).ok();
            LocalEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                dir: meta.as_ref().is_some_and(|meta| meta.is_dir()),
                size: meta.as_ref().map_or(0, |meta| meta.len()),
                modified: meta.and_then(|meta| meta.modified().ok()),
            }
        })
        .collect();
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(entries)
}

fn unix_secs(time: SystemTime) -> Option<u64> {
    time.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// `512 B`, `1.2 KB`, `3.4 MB` -- powers of 1024.
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

/// Local time, `2026-09-14 02:51`.
fn date_text(secs: u64) -> String {
    let time = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the call.
    if unsafe { libc::localtime_r(&time, &mut tm) }.is_null() {
        return String::new();
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sftp::protocol::{Attrs, Entry};
    use crate::sftp::sync::SyncDirection;

    #[test]
    fn picking_with_ctrl_and_shift() {
        let listed = ["a", "b", "c", "d", "e"];
        let mut picked = Picked::default();
        picked.click("b", &listed, false, false);
        assert_eq!(picked.only(), Some("b"));
        picked.click("d", &listed, false, true);
        assert_eq!(picked.names, ["b", "c", "d"], "a range from the last click");
        picked.click("c", &listed, true, false);
        assert_eq!(picked.names, ["b", "d"], "Ctrl+click takes one out");
        picked.click("a", &listed, true, false);
        assert_eq!(picked.names, ["b", "d", "a"]);
        // The range starts at the last plain or Ctrl click: a.
        picked.click("c", &listed, true, true);
        assert_eq!(picked.names, ["b", "d", "a", "c"], "Ctrl+Shift adds a range");
        picked.click("e", &listed, false, true);
        assert_eq!(picked.names, ["a", "b", "c", "d", "e"]);
        picked.keep_listed(&["b", "e"]);
        assert_eq!(picked.names, ["b", "e"]);
        assert_eq!(picked.anchor, None, "the anchor, a, is gone");
        picked.click("e", &listed, false, true);
        assert_eq!(picked.names, ["e"], "without an anchor Shift is a plain click");
        picked.clear();
        assert!(picked.is_empty() && picked.only().is_none());
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(1023), "1023 B");
        assert_eq!(size_text(1536), "1.5 KB");
        assert_eq!(size_text(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn trashing_reports_what_went_wrong() {
        // Only the failure: a real trashing would write to the user's trash.
        if std::process::Command::new("gio").arg("help").output().is_err() {
            return;
        }
        let missing = std::env::temp_dir().join(format!("terminaal-not-there-{}.txt", std::process::id()));
        let err = trash(&[missing]).unwrap_err();
        assert!(err.contains("terminaal-not-there"), "{err}");
    }

    #[test]
    fn local_folders_list_folders_first() {
        let dir = std::env::temp_dir().join(format!("terminaal-files-panel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Zeta")).unwrap();
        std::fs::write(dir.join("alpha.txt"), "12345").unwrap();
        let entries = read_local(&dir).unwrap();
        assert_eq!(entries.iter().map(|e| (e.name.as_str(), e.dir)).collect::<Vec<_>>(), [("Zeta", true), ("alpha.txt", false)]);
        assert_eq!(entries[1].size, 5);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn state() -> State {
        let file = |name: &str, mode| Entry { name: name.into(), attrs: Attrs { size: Some(2048), permissions: Some(mode), atime_mtime: Some((0, 1_700_000_000)), ..Attrs::default() } };
        let edit = |id, state| EditStatus {
            id,
            remote: format!("/etc/file{id}"),
            local: PathBuf::from(format!("/run/user/1000/terminaal/edit/x/file{id}")),
            state,
            command: (id == 4).then(|| " sh '/home/u/.cache/terminaal/sudo/x/run.sh'".to_string()),
            unsaved: id % 2 == 0,
        };
        State {
            connection: Connection::Ready,
            attached: true,
            home: "/home/u".into(),
            dir: "/home/u".into(),
            entries: vec![file("docs", 0o40755), file("notes.txt", 0o100644)],
            listing_error: Some("nope".into()),
            transfers: vec![
                TransferStatus { id: 1, direction: Direction::Download, name: "~/a".into(), done: 10, total: 100, state: TransferState::Running },
                TransferStatus { id: 2, direction: Direction::Upload, name: "/b".into(), done: 1, total: 1, state: TransferState::Failed("x".into()) },
            ],
            edits: vec![
                edit(1, EditState::Synced),
                edit(2, EditState::Conflict),
                edit(3, EditState::Denied(SudoFor::Save)),
                edit(4, EditState::SudoReady(SudoFor::Open)),
                edit(5, EditState::SudoRunning(SudoFor::Save)),
                edit(6, EditState::Failed("gone".into())),
            ],
            syncs: Vec::new(),
            syncs_finished: 0,
            report: Some(Err("failed".into())),
        }
    }

    #[test]
    fn renders_every_state_headless_without_acting() {
        let ctx = egui::Context::default();
        let mut panel = FilesPanel::new();
        let state = state();
        let sync = |index, state, live| SyncStatus {
            index,
            local: "~/site".into(),
            remote: "/srv/www".into(),
            direction: SyncDirection::Both,
            live,
            state,
            notes: (0..8).map(|i| format!("note {i}")).collect(),
        };
        let background =
            State { syncs: vec![sync(0, SyncState::Copying { done: 1, total: 3 }, true), sync(1, SyncState::Busy, true)], ..State::default() };
        let state = State {
            syncs: vec![
                sync(0, SyncState::Waiting, false),
                sync(1, SyncState::Scanning, false),
                sync(2, SyncState::Done { at: SystemTime::now(), copied: 0, deleted: 0 }, false),
                sync(3, SyncState::Done { at: SystemTime::now(), copied: 2, deleted: 1 }, false),
                sync(4, SyncState::Stopped, false),
                sync(5, SyncState::Failed("gone".into()), false),
            ],
            ..state
        };
        panel.remote_selected.names = vec!["docs".into(), "notes.txt".into()];
        for terminal in [true, false] {
            let view = FilesView { state: &state, label: "u@host", terminal, editor: None, background: Some(&background) };
            let mut actions = Vec::new();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 800.0))),
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| panel.show_tab(ui, &view, &mut actions)).drop_without_applying_deltas();
            }
            assert!(actions.is_empty());
        }
        assert!(FilesPanel::needs_attention(&state));
        let calm = State { edits: vec![state.edits[0].clone()], ..state.clone() };
        assert!(!FilesPanel::needs_attention(&calm));

        // Not connected yet: only the header.
        let connecting = State::default();
        let view = FilesView { state: &connecting, label: "u@host", terminal: true, editor: None, background: None };
        let mut actions = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| panel.show_tab(ui, &view, &mut actions)).drop_without_applying_deltas();
        assert!(actions.is_empty());
    }
}
