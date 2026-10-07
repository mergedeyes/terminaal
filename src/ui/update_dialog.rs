//! The dialog offering a new release (`crate::update`): what's new, then
//! downloading and checking it, then restarting into it. Like the paste
//! warning it sits at the top of the console and egui only draws it; the
//! choices go back to `app.rs`. It doesn't take the keyboard or block
//! anything -- typing on in the terminal is fine.

use std::path::PathBuf;

use egui::{Align2, Area, Button, Frame, Id, Order, Rect, RichText, TextStyle, UiKind};

use crate::i18n::t;
use crate::ui::theme;
use crate::update::{Release, Version};

/// Widest the dialog gets, in points.
const WIDTH: f32 = 480.0;
/// Lines of the release notes shown.
const NOTES_LINES: usize = 8;

/// Where the update stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    /// Offered, nothing done yet.
    Offer,
    /// Downloading and checking.
    Installing,
    /// In place; runs from the next start.
    Installed,
    /// The folder of the binary isn't ours to write: whoever installed it
    /// updates it.
    NotWritable(PathBuf),
    Failed(String),
}

/// What was clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Install,
    /// Close; offered again at the next start.
    Later,
    /// Never offer this version again.
    Skip,
    /// The release's page in the browser.
    OpenPage,
    Restart,
}

pub struct UpdateDialog {
    pub release: Release,
    pub stage: Stage,
    current: Version,
    /// Where the dialog ended up in the last pass, in points.
    rect: Option<Rect>,
}

impl UpdateDialog {
    pub fn new(release: Release, current: Version) -> Self {
        Self { release, stage: Stage::Offer, current, rect: None }
    }

    /// `pos` (points) lies on the dialog as last shown.
    pub fn contains(&self, pos: egui::Pos2) -> bool {
        self.rect.is_some_and(|rect| rect.contains(pos))
    }

    /// Show the dialog at the top of `area` (points). Returns a button
    /// clicked in this pass.
    pub fn show(&mut self, ctx: &egui::Context, area: Rect) -> Option<Choice> {
        let colors = theme::colors();
        let width = WIDTH.min(area.width() - 32.0).max(220.0);
        let version = self.release.version.to_string();
        let mut choice = None;
        let response = Area::new(Id::new("update_dialog"))
            .kind(UiKind::Popup)
            .order(Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(area.center().x, area.top() + 24.0))
            .constrain(true)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(12).show(ui, |ui| {
                    ui.set_width(width);
                    match &self.stage {
                        Stage::Offer => {
                            ui.label(RichText::new(t!("update-title", version = &version)).strong().size(15.0));
                            ui.label(RichText::new(t!("update-current", version = self.current.to_string())).color(colors.text_weak));
                            let notes = notes_preview(&self.release.notes);
                            if !notes.is_empty() {
                                ui.add_space(6.0);
                                Frame::new().fill(colors.input).corner_radius(4).inner_margin(8).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(notes).text_style(TextStyle::Small).color(colors.text));
                                });
                            }
                            if self.release.binary.is_some() && ui.link(t!("update-notes")).clicked() {
                                choice = Some(Choice::OpenPage);
                            }
                            ui.add_space(8.0);
                            ui.horizontal_wrapped(|ui| {
                                if self.release.binary.is_some() {
                                    if button(ui, t!("update-install"), true) {
                                    choice = Some(Choice::Install);
                                }
                                } else {
                                    if button(ui, t!("update-open-page"), true) {
                                    choice = Some(Choice::OpenPage);
                                }
                                }
                                if button(ui, t!("update-later"), false) {
                                    choice = Some(Choice::Later);
                                }
                                if button(ui, t!("update-skip"), false) {
                                    choice = Some(Choice::Skip);
                                }
                            });
                            if self.release.binary.is_none() {
                                ui.add_space(4.0);
                                ui.label(RichText::new(t!("update-no-binary")).size(11.0).color(colors.text_weak));
                            }
                        }
                        Stage::Installing => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(t!("update-installing", version = &version));
                            });
                        }
                        Stage::Installed => {
                            ui.label(RichText::new(t!("update-installed", version = &version)).strong().size(15.0));
                            ui.label(t!("update-installed-hint"));
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if button(ui, t!("update-restart"), true) {
                                    choice = Some(Choice::Restart);
                                }
                                if button(ui, t!("update-later"), false) {
                                    choice = Some(Choice::Later);
                                }
                            });
                            ui.add_space(4.0);
                            ui.label(RichText::new(t!("update-restart-hint")).size(11.0).color(colors.text_weak));
                        }
                        Stage::NotWritable(dir) => {
                            ui.label(RichText::new(t!("update-title", version = &version)).strong().size(15.0));
                            ui.label(t!("update-not-writable", dir = dir.display().to_string()));
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if button(ui, t!("update-open-page"), true) {
                                    choice = Some(Choice::OpenPage);
                                }
                                if button(ui, t!("common-close"), false) {
                                    choice = Some(Choice::Later);
                                }
                            });
                        }
                        Stage::Failed(err) => {
                            ui.label(RichText::new(t!("update-failed")).strong().size(15.0).color(colors.error));
                            ui.label(RichText::new(err).color(colors.text_weak));
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if button(ui, t!("update-retry"), true) {
                                    choice = Some(Choice::Install);
                                }
                                if button(ui, t!("update-open-page"), false) {
                                    choice = Some(Choice::OpenPage);
                                }
                                if button(ui, t!("common-close"), false) {
                                    choice = Some(Choice::Later);
                                }
                            });
                        }
                    }
                });
            })
            .response;
        self.rect = Some(response.rect);
        choice
    }
}

/// A dialog button; `strong` for the one most likely wanted.
fn button(ui: &mut egui::Ui, text: String, strong: bool) -> bool {
    let text = if strong { RichText::new(text).strong() } else { RichText::new(text) };
    ui.add(Button::new(text)).clicked()
}

/// The first lines of the release notes with something in them, as plain
/// text: GitHub's generated notes are Markdown with a link to every pull
/// request.
fn notes_preview(notes: &str) -> String {
    let lines: Vec<String> = notes.lines().map(plain).filter(|line| !line.is_empty()).collect();
    let mut shown: Vec<String> = lines.iter().take(NOTES_LINES).cloned().collect();
    if lines.len() > NOTES_LINES {
        shown.push("…".to_string());
    }
    shown.join("\n")
}

/// One line of Markdown without its marks: headings and bold go, list
/// items get a bullet, GitHub links shrink to `#12` or `v0.1.0...v0.2.0`.
fn plain(line: &str) -> String {
    let line = line.trim().replace("**", "");
    let (bullet, rest) = match line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")) {
        Some(rest) => ("• ", rest),
        None => ("", line.trim_start_matches('#').trim_start()),
    };
    let words: Vec<String> = rest
        .split_whitespace()
        .map(|word| {
            let Some(path) = word.strip_prefix("https://github.com/") else { return word.to_string() };
            let last = path.rsplit('/').next().unwrap_or_default();
            if path.contains("/pull/") {
                format!("#{last}")
            } else if path.contains("/compare/") {
                last.to_string()
            } else {
                word.to_string()
            }
        })
        .collect();
    format!("{bullet}{}", words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::Binary;

    fn release(binary: bool) -> Release {
        Release {
            version: Version(0, 2, 0),
            page: "https://github.com/x/releases/tag/v0.2.0".into(),
            notes: "## What's new\n\n- Scrollbar\n- Updates\n".into(),
            binary: binary.then(|| Binary { url: "u".into(), checksum_url: "c".into(), size: 1 }),
        }
    }

    #[test]
    fn notes_read_as_plain_text() {
        let notes = "## What's Changed\r\n* Scrollbar by @mergedeyes in https://github.com/mergedeyes/terminaal/pull/4\r\n\r\n\
                     **Full Changelog**: https://github.com/mergedeyes/terminaal/compare/v0.1.0...v0.2.0";
        assert_eq!(
            notes_preview(notes),
            "What's Changed\n• Scrollbar by @mergedeyes in #4\nFull Changelog: v0.1.0...v0.2.0"
        );
        assert_eq!(plain("- plain item"), "• plain item");
        assert_eq!(plain("see https://example.com/x"), "see https://example.com/x");
        assert_eq!(plain("fixes 3 bugs"), "fixes 3 bugs", "only links become numbers");
    }

    #[test]
    fn notes_are_cut_short() {
        assert_eq!(notes_preview("## New\n\n- a\r\n- b\n"), "New\n• a\n• b");
        let long: String = (1..=12).map(|n| format!("- {n}\n")).collect();
        let shown = notes_preview(&long);
        assert_eq!(shown.lines().count(), NOTES_LINES + 1);
        assert!(shown.ends_with('…'));
    }

    /// Every stage draws; a click on the first button picks it.
    #[test]
    fn renders_every_stage_and_reports_a_click() {
        let ctx = egui::Context::default();
        let area = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 600.0));
        for binary in [true, false] {
            for stage in [
                Stage::Offer,
                Stage::Installing,
                Stage::Installed,
                Stage::NotWritable("/usr/bin".into()),
                Stage::Failed("checksum".into()),
            ] {
                let mut dialog = UpdateDialog::new(release(binary), Version(0, 1, 0));
                dialog.stage = stage;
                let mut choice = None;
                ctx.run_ui(egui::RawInput::default(), |ui| choice = dialog.show(ui.ctx(), area)).drop_without_applying_deltas();
                assert_eq!(choice, None, "nothing clicked");
                assert!(dialog.contains(egui::pos2(450.0, 40.0)), "at the top in the middle");
                assert!(!dialog.contains(egui::pos2(10.0, 590.0)));
            }
        }

        // Click up the dialog's left edge from the bottom: the first thing
        // hit is the strong button of the offer, which installs.
        let mut dialog = UpdateDialog::new(release(true), Version(0, 1, 0));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = dialog.show(ui.ctx(), area);
        })
        .drop_without_applying_deltas();
        let rect = dialog.rect.unwrap();
        let mut picked = None;
        for y in (rect.top() as i32..rect.bottom() as i32).rev().step_by(4) {
            let pos = egui::pos2(rect.left() + 30.0, y as f32);
            let click = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
            for _ in 0..2 {
                let input = egui::RawInput { events: vec![egui::Event::PointerMoved(pos), click(true), click(false)], ..Default::default() };
                ctx.run_ui(input, |ui| picked = dialog.show(ui.ctx(), area).or(picked)).drop_without_applying_deltas();
            }
            if picked.is_some() {
                break;
            }
        }
        assert_eq!(picked, Some(Choice::Install));
    }
}
