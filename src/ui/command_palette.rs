//! The command palette (Ctrl+Shift+P): one search field over everything
//! that can be picked -- open tabs, hosts with each of their logins,
//! snippets for the active terminal, shortcut actions, themes and shells.
//!
//! `app.rs` builds the [`Entry`] list when it opens the palette and keeps
//! the keyboard itself, like for the scrollback search: typed text goes to
//! [`CommandPalette::push_str`], arrows to [`CommandPalette::move_selection`],
//! Enter to [`CommandPalette::selected`]. egui only draws it, and reports a
//! click on an entry. Matching is fuzzy ([`fuzzy`]): the query's characters
//! in order, rewarded where they start words or follow each other.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Align2, Area, Frame, Id, Layout, Order, Rect, RichText, ScrollArea, Sense, TextStyle, UiKind, vec2};

use crate::i18n::t;
use crate::shortcuts::Action;
use crate::ui::theme;

/// Widest the palette gets, in points.
const WIDTH: f32 = 560.0;
/// Height of the list before it scrolls, in points.
const LIST_HEIGHT: f32 = 380.0;
/// Rows PageUp/PageDown move.
const PAGE: isize = 8;

/// What picking an entry does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Action(Action),
    /// Switch to the tab at this index.
    Tab(usize),
    /// Connect to a host with login `login` (index into `Host::all_logins`);
    /// `saved`: in hosts.toml, else from `~/.ssh/config`.
    Connect { saved: bool, index: usize, login: usize },
    /// Run this snippet's command in the active terminal.
    Snippet(String),
    Theme(String),
    /// A new tab with the installed shell at this index.
    Shell(usize),
}

/// One pickable line.
#[derive(Clone, Debug)]
pub struct Entry {
    /// What kind of thing it is, shown in front: "Tab", "Host", an action's
    /// group, …
    pub kind: String,
    /// Matched against the query and highlighted.
    pub title: String,
    /// Shown weaker on the right (a shortcut, `user@host`, a command);
    /// matched too, without highlighting.
    pub detail: String,
    pub item: Item,
}

/// An entry that matches the query.
struct Hit {
    entry: usize,
    score: i32,
    /// Character indices of `title` that matched.
    positions: Vec<usize>,
}

pub struct CommandPalette {
    query: String,
    entries: Vec<Entry>,
    hits: Vec<Hit>,
    /// Index into `hits`.
    selected: usize,
    /// The selection moved by keyboard: scroll it into view next pass.
    reveal: bool,
    /// Where the palette ended up in the last pass, in points.
    rect: Option<Rect>,
}

impl CommandPalette {
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut palette = Self { query: String::new(), entries, hits: Vec::new(), selected: 0, reveal: true, rect: None };
        palette.refilter();
        palette
    }

    #[cfg(test)]
    fn query(&self) -> &str {
        &self.query
    }

    /// Type into the query. Line breaks (a pasted line) end it.
    pub fn push_str(&mut self, text: &str) {
        let text = text.lines().next().unwrap_or_default();
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if !text.is_empty() {
            self.query.push_str(&text);
            self.refilter();
        }
    }

    pub fn pop(&mut self) {
        if self.query.pop().is_some() {
            self.refilter();
        }
    }

    /// Move the selection by `rows`, wrapping around at the ends for
    /// single steps and stopping there for pages.
    pub fn move_selection(&mut self, rows: isize) {
        let len = self.hits.len() as isize;
        if len == 0 {
            return;
        }
        let next = self.selected as isize + rows;
        self.selected = if rows.abs() == 1 { next.rem_euclid(len) } else { next.clamp(0, len - 1) } as usize;
        self.reveal = true;
    }

    pub fn page(&mut self, down: bool) {
        self.move_selection(if down { PAGE } else { -PAGE });
    }

    /// The entry Enter would pick.
    pub fn selected(&self) -> Option<&Item> {
        self.hits.get(self.selected).map(|hit| &self.entries[hit.entry].item)
    }

    /// `pos` (points) lies on the palette as last shown.
    pub fn contains(&self, pos: egui::Pos2) -> bool {
        self.rect.is_some_and(|rect| rect.contains(pos))
    }

    /// Match the entries against the query again: best first, ties in the
    /// order they were given. Without a query, all of them as given.
    fn refilter(&mut self) {
        let words: Vec<&str> = self.query.split_whitespace().collect();
        self.hits = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(entry, candidate)| {
                let mut score = 0;
                let mut positions = Vec::new();
                for word in &words {
                    match fuzzy(word, &candidate.title) {
                        Some((word_score, mut matched)) => {
                            score += word_score;
                            positions.append(&mut matched);
                        }
                        // The detail counts, but less: "root" finds a
                        // host's login, below a host named root.
                        None => score += fuzzy(word, &candidate.detail).or_else(|| fuzzy(word, &candidate.kind))?.0 / 2,
                    }
                }
                positions.sort_unstable();
                positions.dedup();
                Some(Hit { entry, score, positions })
            })
            .collect();
        // Stable: equal scores keep the given order.
        self.hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        self.selected = 0;
        self.reveal = true;
    }

    /// Show the palette at the top of `area` (points: right of the sidebar,
    /// below the tab bar). Returns the entry clicked in this pass.
    pub fn show(&mut self, ctx: &egui::Context, area: Rect) -> Option<Item> {
        let colors = theme::colors();
        let width = WIDTH.min(area.width() - 32.0).max(200.0);
        let mut clicked = None;
        let response = Area::new(Id::new("command_palette"))
            .kind(UiKind::Popup)
            .order(Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(area.center().x, area.top() + 12.0))
            .constrain(true)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    // The query, as a field that isn't one: the keyboard
                    // stays with `app.rs`.
                    Frame::new().fill(colors.input).corner_radius(4).inner_margin(8).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let line = if self.query.is_empty() {
                            RichText::new(t!("palette-placeholder")).color(colors.text_weak)
                        } else {
                            RichText::new(format!("{}▏", self.query)).color(colors.text)
                        };
                        ui.label(line.text_style(TextStyle::Body));
                    });
                    ui.add_space(4.0);
                    if self.hits.is_empty() {
                        ui.label(RichText::new(t!("palette-nothing-found")).color(colors.text_weak));
                    }
                    ScrollArea::vertical().max_height(LIST_HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
                        for (i, hit) in self.hits.iter().enumerate() {
                            let entry = &self.entries[hit.entry];
                            let row = entry_row(ui, entry, &hit.positions, i == self.selected);
                            if i == self.selected && self.reveal {
                                row.scroll_to_me(None);
                            }
                            if row.clicked() {
                                clicked = Some(entry.item.clone());
                            }
                        }
                    });
                    ui.add_space(2.0);
                    ui.label(RichText::new(t!("palette-hint")).size(11.0).color(colors.text_weak));
                });
            })
            .response;
        self.reveal = false;
        self.rect = Some(response.rect);
        clicked
    }
}

/// One entry: kind, title with its matched characters in the accent color,
/// detail on the right.
fn entry_row(ui: &mut egui::Ui, entry: &Entry, positions: &[usize], selected: bool) -> egui::Response {
    let colors = theme::colors();
    let font = TextStyle::Body.resolve(ui.style());
    let small = TextStyle::Small.resolve(ui.style());
    let height = ui.spacing().interact_size.y + 4.0;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if selected || response.hovered() {
        let fill = if selected { colors.selected } else { colors.hover };
        ui.painter().rect_filled(rect, 4.0, fill);
    }
    let inner = rect.shrink2(vec2(8.0, 0.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.add_sized(
            vec2(92.0, height),
            egui::Label::new(RichText::new(&entry.kind).font(small.clone()).color(colors.text_weak)).truncate(),
        );
        let mut job = LayoutJob::default();
        for (i, c) in entry.title.chars().enumerate() {
            let color = if positions.contains(&i) { colors.accent } else { colors.text };
            job.append(c.encode_utf8(&mut [0; 4]), 0.0, TextFormat::simple(font.clone(), color));
        }
        ui.add(egui::Label::new(job).truncate().selectable(false));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let detail = entry.detail.lines().next().unwrap_or_default();
            ui.add(
                egui::Label::new(RichText::new(detail).font(small).color(colors.text_weak)).truncate().selectable(false),
            );
        });
    });
    response
}

/// How well `query` matches `text`, case-insensitively, with the character
/// indices of `text` that matched; `None` unless all of `query`'s characters
/// appear in `text` in order. Each character is taken where it starts a
/// word if it can, else at its first place; the query as one piece of the
/// text (at a word start if possible) is tried too and scores extra.
pub fn fuzzy(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let word_start =
        |i: usize| i == 0 || !chars[i - 1].is_alphanumeric() || (chars[i].is_uppercase() && chars[i - 1].is_lowercase());

    let mut positions = Vec::with_capacity(query.len());
    let mut from = 0;
    // The rest of the query still fits into the text after `i`.
    let fits = |i: usize, rest: &[char]| {
        let mut chars = lower[i + 1..].iter();
        rest.iter().all(|q| chars.any(|c| c == q))
    };
    for (n, &q) in query.iter().enumerate() {
        let rest = &query[n + 1..];
        let found = |i: &usize| lower[*i] == q;
        // Right after the last match beats a word start further on.
        let next = if positions.last().is_some_and(|&last: &usize| last + 1 == from) && lower.get(from) == Some(&q) {
            Some(from)
        } else {
            (from..lower.len())
                .filter(found)
                .find(|&i| word_start(i) && fits(i, rest))
                .or_else(|| (from..lower.len()).find(found))
        };
        positions.push(next?);
        from = positions[positions.len() - 1] + 1;
    }
    let mut best = (score(&positions, &word_start), positions);

    if query.len() > 1 && query.len() <= lower.len() {
        let runs: Vec<usize> = (0..=lower.len() - query.len()).filter(|&i| lower[i..i + query.len()] == query[..]).collect();
        if let Some(start) = runs.iter().copied().find(|&i| word_start(i)).or_else(|| runs.first().copied()) {
            let run: Vec<usize> = (start..start + query.len()).collect();
            let run_score = score(&run, &word_start) + 10;
            if run_score > best.0 {
                best = (run_score, run);
            }
        }
    }
    // Shorter texts first among equal matches.
    best.0 -= (chars.len() as i32 / 16).min(4);
    Some(best)
}

/// Points for matched `positions`: each character, more at word starts and
/// right after the one before, less after a gap.
fn score(positions: &[usize], word_start: &impl Fn(usize) -> bool) -> i32 {
    let mut score = 0;
    let mut last: Option<usize> = None;
    for &i in positions {
        score += 1;
        if word_start(i) {
            score += 8;
        }
        score += match last {
            Some(last) if last + 1 == i => 6,
            Some(last) => -((i - last - 1) as i32).min(5),
            None => -(i as i32).min(5),
        };
        last = Some(i);
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, title: &str, detail: &str, item: Item) -> Entry {
        Entry { kind: kind.into(), title: title.into(), detail: detail.into(), item }
    }

    fn titles(palette: &CommandPalette) -> Vec<&str> {
        palette.hits.iter().map(|hit| palette.entries[hit.entry].title.as_str()).collect()
    }

    #[test]
    fn fuzzy_matches_in_order_and_prefers_word_starts() {
        assert_eq!(fuzzy("spr", "Split right").unwrap().1, [0, 1, 6]);
        assert!(fuzzy("rs", "Split right").is_none(), "out of order");
        assert_eq!(fuzzy("NT", "new tab").unwrap().1, [0, 4]);
        // Word starts and runs beat scattered characters.
        let (tab, _) = fuzzy("tab", "New tab").unwrap();
        let (scattered, _) = fuzzy("tab", "Toggle a broadcast").unwrap();
        assert!(tab > scattered);
        assert_eq!(fuzzy("f", "SftpFiles").unwrap().1, [4], "camel case starts words");
        assert_eq!(fuzzy("ff", "SftpFiles").unwrap().1, [1, 4], "a word start only where the rest still fits");
        assert!(fuzzy("ü", "Grün").is_some());
        assert_eq!(fuzzy("", "anything").unwrap().1, Vec::<usize>::new());
    }

    #[test]
    fn filters_sorts_and_moves() {
        let mut palette = CommandPalette::new(vec![
            entry("Aktion", "Rechts teilen", "Strg+Umschalt+D", Item::Action(Action::SplitRight)),
            entry("Host", "prod", "root@10.0.0.1", Item::Connect { saved: true, index: 0, login: 0 }),
            entry("Host", "root-box", "me@box", Item::Connect { saved: true, index: 1, login: 0 }),
            entry("Theme", "Nord", "", Item::Theme("Nord".into())),
        ]);
        assert_eq!(titles(&palette).len(), 4, "everything without a query");
        assert_eq!(palette.selected(), Some(&Item::Action(Action::SplitRight)));

        palette.push_str("root");
        assert_eq!(titles(&palette), ["root-box", "prod"], "the title beats the detail");
        palette.move_selection(1);
        assert_eq!(palette.selected(), Some(&Item::Connect { saved: true, index: 0, login: 0 }));
        palette.move_selection(1);
        assert_eq!(palette.selected(), Some(&Item::Connect { saved: true, index: 1, login: 0 }), "wraps around");
        palette.page(true);
        assert_eq!(palette.selected(), Some(&Item::Connect { saved: true, index: 0, login: 0 }), "pages stop at the end");

        palette.pop();
        palette.pop();
        palette.pop();
        palette.pop();
        palette.push_str("host no\nrest");
        assert_eq!(palette.query(), "host no");
        assert!(titles(&palette).is_empty(), "every word has to match");
        palette.pop();
        palette.pop();
        palette.pop();
        assert_eq!(titles(&palette), ["prod", "root-box"], "the kind counts too");
        palette.push_str("zzz");
        assert_eq!(palette.selected(), None);
        palette.move_selection(1);
    }

    #[test]
    fn renders_and_reports_a_click() {
        let ctx = egui::Context::default();
        let mut palette = CommandPalette::new(vec![
            entry("Tab", "fish", "Tab 1", Item::Tab(0)),
            entry("Befehl", "Logs", "journalctl -f\nmore", Item::Snippet("journalctl -f".into())),
        ]);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0));
        let pass = |palette: &mut CommandPalette, events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let mut clicked = None;
            ctx.run_ui(input, |ui| clicked = palette.show(ui.ctx(), screen).or(clicked.clone())).drop_without_applying_deltas();
            clicked
        };
        for _ in 0..3 {
            assert_eq!(pass(&mut palette, Vec::new()), None);
        }
        let rect = palette.rect.expect("shown");
        assert!(rect.width() <= WIDTH + 40.0 && rect.center().x.round() == 400.0);
        // The second row: below the query and the first row.
        let point = egui::pos2(rect.center().x, rect.top() + 84.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let picked = [vec![egui::Event::PointerMoved(point)], vec![button(true)], vec![button(false)]]
            .into_iter()
            .fold(None, |picked, events| pass(&mut palette, events).or(picked));
        assert!(matches!(picked, Some(Item::Tab(0) | Item::Snippet(_))), "a click picks a row");
    }
}

