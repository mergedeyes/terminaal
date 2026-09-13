//! The built-in commands, under the aliases in the sidebar's shell
//! section: one button per chore (`crate::commands`), tailored to the
//! system the active tab is on.
//!
//! A click hands the line back as [`SidebarAction::RunCommand`]; whether
//! it is run or only typed into the prompt is `commands_run`. Before the
//! first one is run, the warning below says so -- once per installation
//! (`commands_warned`).

use egui::{CornerRadius, Frame, Margin, RichText, Ui};

use crate::commands::{self, Command, Family, Group, Target};
use crate::config::{Config, Setting};
use crate::i18n::t;
use crate::ui::sidebar::SidebarAction;
use crate::ui::theme;
use crate::ui::widgets::{section_title, weak};

#[derive(Default)]
pub struct CommandsPanel {
    /// The command waiting for the warning to be acknowledged.
    pending: Option<Command>,
}

impl CommandsPanel {
    /// `target` is `None` while no terminal tab is showing (the settings
    /// tab is): there'd be no shell to send anything to.
    pub fn show(&mut self, ui: &mut Ui, config: &Config, target: Option<&Target>, actions: &mut Vec<SidebarAction>) {
        section_title(ui, &t!("cmd-title"));
        let Some(target) = target else {
            ui.label(weak(t!("cmd-no-tab")));
            return;
        };

        let Some(system) = target.system else {
            // An SSH session that hasn't got to the probe yet.
            ui.label(weak(t!("cmd-system-probing")));
            return;
        };
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
                        self.activate(command, config, actions);
                    }
                }
            });
            ui.add_space(4.0);
        }

        if self.pending.is_some() {
            self.warning(ui, actions);
        }
    }

    /// A button was clicked: off it goes, unless the warning still has to
    /// be acknowledged. That warning is about running things unasked, so
    /// it only applies while that's what a click does.
    fn activate(&mut self, command: &Command, config: &Config, actions: &mut Vec<SidebarAction>) {
        if config.commands_run && !config.commands_warned {
            self.pending = Some(command.clone());
        } else {
            actions.push(SidebarAction::RunCommand(command.line.clone()));
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
                if let Some(command) = &self.pending {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&command.line).monospace().size(12.0).color(theme::colors().text_weak));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    confirmed = ui.button(t!("cmd-warn-run")).clicked();
                    cancelled = ui.button(t!("common-cancel")).clicked();
                });
            },
        );
        if confirmed && let Some(command) = self.pending.take() {
            actions.push(SidebarAction::ChangeSetting { setting: Setting::CommandsWarned(true), save: true });
            actions.push(SidebarAction::RunCommand(command.line));
        }
        if cancelled {
            self.pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::System;

    fn target(family: Family) -> Target {
        Target { system: Some(System { family, root: false }), host: None, configured: false }
    }

    /// Lays the section out in headless egui passes -- no tab, a system
    /// we know, one we don't, and the warning -- to catch panics the type
    /// checker can't.
    #[test]
    fn renders_every_state_headless() {
        let ctx = egui::Context::default();
        let config = Config::default();
        let mut panel = CommandsPanel::default();
        for target in [None, Some(target(Family::Arch)), Some(target(Family::Unknown)), Some(Target::default())] {
            let mut actions = Vec::new();
            ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui, &config, target.as_ref(), &mut actions))
                .drop_without_applying_deltas();
            assert!(actions.is_empty(), "nothing was clicked");
        }
        panel.pending = Some(commands::catalog(System { family: Family::Debian, root: false }, false).remove(0));
        let mut actions = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui, &config, Some(&target(Family::Debian)), &mut actions))
            .drop_without_applying_deltas();
        assert!(panel.pending.is_some(), "the warning is still waiting");
    }

    /// The first click asks before anything runs; once acknowledged --
    /// and whenever commands are only typed out -- it goes straight
    /// through.
    #[test]
    fn the_warning_comes_before_the_first_run() {
        let command = commands::catalog(System { family: Family::Arch, root: false }, false).remove(0);
        let mut panel = CommandsPanel::default();
        let mut config = Config::default();
        assert!(config.commands_run && !config.commands_warned, "as it is out of the box");

        let mut actions = Vec::new();
        panel.activate(&command, &config, &mut actions);
        assert!(actions.is_empty(), "nothing runs before the warning is acknowledged");
        assert_eq!(panel.pending.as_ref(), Some(&command));

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
        panel.activate(&command, &config, &mut actions);
        assert!(matches!(actions.as_slice(), [SidebarAction::RunCommand(_)]));

        // Typing it out is harmless, so it never asks.
        config.commands_warned = false;
        config.commands_run = false;
        let mut actions = Vec::new();
        panel.activate(&command, &config, &mut actions);
        assert!(matches!(actions.as_slice(), [SidebarAction::RunCommand(_)]));
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
