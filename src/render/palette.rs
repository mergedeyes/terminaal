//! Default ANSI color palette.
//!
//! `alacritty_terminal`'s own `Term::colors` table (`term::color::Colors`)
//! starts out completely empty -- every one of its 269 slots is `None` --
//! and only gets filled in when the running program issues an OSC 4/10/11
//! (etc.) color-change escape sequence. Everything else has to come from a
//! palette we own and fall back to. This module is that palette: the
//! classic 16 ANSI colors, the 6x6x6 color cube, the 24-step grayscale
//! ramp, and the special foreground/background/cursor/dim slots -- indexed
//! exactly the way `term::color::Colors` expects, so resolving a cell's
//! color is just "ask the terminal's overrides first, then fall back to
//! this table".

use alacritty_terminal::term::color::Colors as TermColors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// Total number of color slots, mirrors `alacritty_terminal::term::color::COUNT`.
pub const COUNT: usize = 269;

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
pub struct Palette([Rgb; COUNT]);

impl Palette {
    pub fn get(&self, index: usize) -> Rgb {
        self.0[index]
    }

    pub fn named(&self, color: NamedColor) -> Rgb {
        self.get(color as usize)
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

impl Default for Palette {
    fn default() -> Self {
        let mut colors = [Rgb { r: 0, g: 0, b: 0 }; COUNT];

        // Classic 16 ANSI colors (0..16). Values are the common
        // "xterm.js default theme" set -- familiar, readable, easy to
        // re-theme later.
        const NORMAL: [(u8, u8, u8); 8] = [
            (0, 0, 0),
            (205, 49, 49),
            (13, 188, 121),
            (229, 229, 16),
            (36, 114, 200),
            (188, 63, 188),
            (17, 168, 205),
            (229, 229, 229),
        ];
        const BRIGHT: [(u8, u8, u8); 8] = [
            (102, 102, 102),
            (241, 76, 76),
            (35, 209, 139),
            (245, 245, 67),
            (59, 142, 234),
            (214, 112, 214),
            (41, 184, 219),
            (229, 229, 229),
        ];
        for (i, &(r, g, b)) in NORMAL.iter().enumerate() {
            colors[i] = Rgb { r, g, b };
        }
        for (i, &(r, g, b)) in BRIGHT.iter().enumerate() {
            colors[8 + i] = Rgb { r, g, b };
        }

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

        colors[NamedColor::Foreground as usize] = Rgb { r: 229, g: 229, b: 229 };
        colors[NamedColor::Background as usize] = Rgb { r: 0, g: 0, b: 0 };
        colors[NamedColor::Cursor as usize] = Rgb { r: 229, g: 229, b: 229 };
        colors[NamedColor::BrightForeground as usize] = Rgb { r: 255, g: 255, b: 255 };
        colors[NamedColor::DimForeground as usize] = Rgb { r: 150, g: 150, b: 150 };

        let dim = |c: Rgb| Rgb {
            r: (c.r as f32 * 0.66) as u8,
            g: (c.g as f32 * 0.66) as u8,
            b: (c.b as f32 * 0.66) as u8,
        };
        colors[NamedColor::DimBlack as usize] = dim(colors[NamedColor::Black as usize]);
        colors[NamedColor::DimRed as usize] = dim(colors[NamedColor::Red as usize]);
        colors[NamedColor::DimGreen as usize] = dim(colors[NamedColor::Green as usize]);
        colors[NamedColor::DimYellow as usize] = dim(colors[NamedColor::Yellow as usize]);
        colors[NamedColor::DimBlue as usize] = dim(colors[NamedColor::Blue as usize]);
        colors[NamedColor::DimMagenta as usize] = dim(colors[NamedColor::Magenta as usize]);
        colors[NamedColor::DimCyan as usize] = dim(colors[NamedColor::Cyan as usize]);
        colors[NamedColor::DimWhite as usize] = dim(colors[NamedColor::White as usize]);

        Self(colors)
    }
}
