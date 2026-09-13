//! The console's color table.
//!
//! `alacritty_terminal`'s own `Term::colors` table (`term::color::Colors`)
//! starts out completely empty -- every one of its 269 slots is `None` --
//! and only gets filled in when the running program issues an OSC 4/10/11
//! (etc.) color-change escape sequence. Everything else has to come from a
//! palette we own and fall back to. This module is that palette: the
//! theme's 16 ANSI colors (`crate::theme`), the 6x6x6 color cube, the
//! 24-step grayscale ramp, and the special foreground/background/cursor/dim
//! slots -- indexed exactly the way `term::color::Colors` expects, so
//! resolving a cell's color is just "ask the terminal's overrides first,
//! then fall back to this table".

use alacritty_terminal::term::color::Colors as TermColors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

use crate::theme::TerminalColors;

/// Total number of color slots, mirrors `alacritty_terminal::term::color::COUNT`.
pub const COUNT: usize = 269;

/// Where the theme's dim colors go, black to white.
const DIM: [NamedColor; 8] = [
    NamedColor::DimBlack,
    NamedColor::DimRed,
    NamedColor::DimGreen,
    NamedColor::DimYellow,
    NamedColor::DimBlue,
    NamedColor::DimMagenta,
    NamedColor::DimCyan,
    NamedColor::DimWhite,
];

/// Convert an sRGB color (as stored in the palette, or written in CSS)
/// to the linear `[r, g, b, a]` that quads and the clear color need.
/// The surface is `Bgra8UnormSrgb`, so the GPU treats shader output as
/// linear and sRGB-encodes it on write -- passing sRGB values straight
/// through makes every color come out too light. (Text is unaffected:
/// glyphon does this conversion itself.)
pub fn to_linear(c: Rgb, alpha: f32) -> [f32; 4] {
    let ch = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    [ch(c.r), ch(c.g), ch(c.b), alpha]
}

#[derive(Clone, Copy)]
pub struct Palette {
    colors: [Rgb; COUNT],
    selection: Rgb,
    selection_text: Option<Rgb>,
    search_match: (Rgb, Option<Rgb>),
    search_focus: (Rgb, Option<Rgb>),
}

impl Palette {
    pub fn new(theme: &TerminalColors) -> Self {
        let mut colors = [Rgb::default(); COUNT];
        colors[..8].copy_from_slice(&theme.normal);
        colors[8..16].copy_from_slice(&theme.bright);

        // 6x6x6 color cube (indices 16..232), standard xterm-256 levels.
        const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let mut i = 16;
        for r in LEVELS {
            for g in LEVELS {
                for b in LEVELS {
                    colors[i] = Rgb { r, g, b };
                    i += 1;
                }
            }
        }

        // Grayscale ramp (indices 232..256).
        for step in 0..24u8 {
            let v = 8 + step * 10;
            colors[232 + step as usize] = Rgb { r: v, g: v, b: v };
        }

        colors[NamedColor::Foreground as usize] = theme.foreground;
        colors[NamedColor::Background as usize] = theme.background;
        colors[NamedColor::Cursor as usize] = theme.cursor;
        colors[NamedColor::BrightForeground as usize] = theme.bright_foreground;
        colors[NamedColor::DimForeground as usize] = theme.dim_foreground;
        for (slot, color) in DIM.into_iter().zip(theme.dim) {
            colors[slot as usize] = color;
        }

        Self {
            colors,
            selection: theme.selection,
            selection_text: theme.selection_text,
            search_match: theme.search_match,
            search_focus: theme.search_focus,
        }
    }

    pub fn get(&self, index: usize) -> Rgb {
        self.colors[index]
    }

    pub fn named(&self, color: NamedColor) -> Rgb {
        self.get(color as usize)
    }

    /// Background and text of selected cells; no text color keeps each
    /// cell's own.
    pub fn selection(&self) -> (Rgb, Option<Rgb>) {
        (self.selection, self.selection_text)
    }

    /// Background and text of a search match on screen; `focus` for the
    /// one in focus.
    pub fn search(&self, focus: bool) -> (Rgb, Option<Rgb>) {
        if focus { self.search_focus } else { self.search_match }
    }

    /// Resolve an `ansi::Color` to a concrete RGB value, preferring
    /// whatever the running program has overridden via escape sequences
    /// and falling back to this default palette otherwise.
    pub fn resolve(&self, color: Color, overrides: &TermColors) -> Rgb {
        match color {
            Color::Spec(rgb) => rgb,
            Color::Named(named) => overrides[named].unwrap_or_else(|| self.named(named)),
            Color::Indexed(idx) => overrides[idx as usize].unwrap_or_else(|| self.get(idx as usize)),
        }
    }
}
