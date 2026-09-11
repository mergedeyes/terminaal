//! Startup splash: `assets/terminaal_taa.gif` played once over the whole
//! window while the first tab's shell starts underneath, then faded out.
//!
//! The GIF's frames are decoded and scaled to display size on a worker
//! thread ([`decode`]); `app.rs` hands the result to [`Splash::play`].
//! [`Splash::show`] says when the picture changes next and `app.rs`
//! schedules exactly that redraw: one per GIF frame plus the fade, nothing
//! once it's over. Not via egui's `request_repaint_after` -- that subtracts
//! the predicted frame time, which after an idle stretch is the whole idle
//! time, so it asked for bursts of immediate repaints instead. Any key or
//! click ends the splash early -- see `app.rs`.

use std::io::Cursor;
use std::time::{Duration, Instant};

use egui::epaint::RectShape;
use egui::{Color32, ColorImage, CornerRadius, Id, LayerId, Order, Rect, TextureHandle, TextureOptions, pos2, vec2};
use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;

use super::theme;

/// `assets/terminaal_taa.gif` scaled to 640 px (`MAX_SIDE` at scale 2):
/// `magick terminaal_taa.gif -coalesce -resize 640x640 -layers Optimize
/// terminaal_splash_640.gif`. image's GIF frame iterator is generic over
/// the reader, so its compositing loop compiles unoptimized in dev builds
/// -- the 1200 px original took ~0.7 s there just to decode.
const GIF: &[u8] = include_bytes!("../../assets/terminaal_splash_640.gif");

/// Largest the animation is shown, in logical pixels; smaller windows get
/// half their shorter side.
pub const MAX_SIDE: f32 = 320.0;
/// How long the whole overlay takes to fade out after the last GIF frame.
const FADE: Duration = Duration::from_millis(350);
/// Redraw interval while fading.
const FADE_STEP: Duration = Duration::from_millis(16);

/// Decoded frames, scaled to display size, with their delays.
pub struct SplashFrames(Vec<(ColorImage, Duration)>);

// Not derived: it would dump megabytes of pixels.
impl std::fmt::Debug for SplashFrames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SplashFrames({} frames)", self.0.len())
    }
}

/// Decode the GIF, scaled so its longer side is `side` pixels. image's
/// decoder composites each frame onto the full canvas (offsets, disposal).
pub fn decode(side: u32) -> Result<SplashFrames, String> {
    let decoder = GifDecoder::new(Cursor::new(GIF)).map_err(|e| e.to_string())?;
    let frames = decoder
        .into_frames()
        .map(|frame| {
            let frame = frame.map_err(|e| e.to_string())?;
            let (numer, denom) = frame.delay().numer_denom_ms();
            let ms = numer / denom.max(1);
            // Like browsers: 0-10 ms means "unspecified".
            let delay = Duration::from_millis(if ms <= 10 { 100 } else { ms.into() });
            let buffer = frame.into_buffer();
            let scale = side as f32 / buffer.width().max(buffer.height()) as f32;
            let width = ((buffer.width() as f32 * scale).round() as u32).max(1);
            let height = ((buffer.height() as f32 * scale).round() as u32).max(1);
            // Through `DynamicImage` rather than the generic
            // `imageops::resize`: generics are compiled in the calling
            // crate, i.e. unoptimized in dev builds (~30x slower), while
            // this non-generic method is compiled inside `image`.
            let scaled = image::DynamicImage::ImageRgba8(buffer)
                .resize_exact(width, height, FilterType::Triangle)
                .into_rgba8();
            let image = ColorImage::from_rgba_unmultiplied([width as usize, height as usize], scaled.as_raw());
            Ok((image, delay))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if frames.is_empty() {
        return Err("GIF has no frames".into());
    }
    Ok(SplashFrames(frames))
}

pub enum Splash {
    /// Frames are still being decoded; nothing is shown yet.
    Loading { since: Instant },
    Playing { textures: Vec<TextureHandle>, delays: Vec<Duration>, start: Instant },
}

impl Splash {
    pub fn play(ctx: &egui::Context, frames: SplashFrames) -> Self {
        let (textures, delays) = frames
            .0
            .into_iter()
            .enumerate()
            .map(|(i, (image, delay))| (ctx.load_texture(format!("splash-{i}"), image, TextureOptions::LINEAR), delay))
            .unzip();
        Splash::Playing { textures, delays, start: Instant::now() }
    }

    pub fn is_playing(&self) -> bool {
        matches!(self, Splash::Playing { .. })
    }

    /// Paint the current frame over everything. Returns how soon the
    /// picture changes next -- `None` while loading, and once it's over.
    pub fn show(&self, ctx: &egui::Context) -> Option<Duration> {
        let Splash::Playing { textures, delays, start } = self else { return None };
        let (idx, alpha, next) = timeline(delays, start.elapsed())?;

        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("splash")));
        painter.rect_filled(screen, 0.0, theme::BG.gamma_multiply(alpha));

        let texture = &textures[idx];
        let [w, h] = texture.size().map(|v| v as f32);
        let side = (screen.width().min(screen.height()) * 0.5).min(MAX_SIDE);
        let scale = side / w.max(h);
        let rect = Rect::from_center_size(screen.center(), vec2(w * scale, h * scale));
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let radius = CornerRadius::same((side * 0.08) as u8);
        painter.add(RectShape::filled(rect, radius, Color32::WHITE.gamma_multiply(alpha)).with_texture(texture.id(), uv));

        Some(next)
    }
}

/// Which frame shows `elapsed` into the animation, the overlay's opacity
/// and how long until that changes; `None` once the fade-out is over.
fn timeline(delays: &[Duration], elapsed: Duration) -> Option<(usize, f32, Duration)> {
    let mut end = Duration::ZERO;
    for (i, delay) in delays.iter().enumerate() {
        end += *delay;
        if elapsed < end {
            return Some((i, 1.0, end - elapsed));
        }
    }
    let last = delays.len().checked_sub(1)?;
    let fading = elapsed - end;
    (fading < FADE).then(|| (last, 1.0 - fading.as_secs_f32() / FADE.as_secs_f32(), FADE_STEP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_decodes_scaled() {
        let SplashFrames(frames) = decode(64).unwrap();
        assert!(!frames.is_empty());
        assert!(frames.iter().all(|(image, _)| image.size[0].max(image.size[1]) == 64));
        assert!(frames.iter().all(|(_, delay)| !delay.is_zero()));
    }

    #[test]
    fn timeline_steps_through_frames_then_fades() {
        let ms = Duration::from_millis;
        let delays = [ms(100), ms(200)];
        assert_eq!(timeline(&delays, ms(0)), Some((0, 1.0, ms(100))));
        assert_eq!(timeline(&delays, ms(150)), Some((1, 1.0, ms(150))));
        let (idx, alpha, next) = timeline(&delays, ms(300) + FADE / 2).unwrap();
        assert_eq!((idx, next), (1, FADE_STEP));
        assert!((alpha - 0.5).abs() < 1e-3);
        assert_eq!(timeline(&delays, ms(300) + FADE), None);
        assert_eq!(timeline(&[], ms(0)), None);
    }
}
