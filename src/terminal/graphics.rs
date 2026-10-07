//! Images in the terminal: the kitty graphics protocol
//! (<https://sw.kovidgoyal.net/kitty/graphics-protocol/>), as `kitten
//! icat`, yazi, chafa or timg speak it.
//!
//! The commands come as APC sequences (`ESC _ G <keys> ; <base64> ESC \`)
//! and `alacritty_terminal` would drop them, so the byte filter in front
//! of its parser (`integration::Filter`) picks them out and hands them to
//! the terminal's [`Graphics`]. Images are stored here, decoded to RGBA.
//!
//! Where an image is shown has to move with the text, like the prompt
//! marks: the filter doesn't know where the cursor is (the parser runs
//! later, on another thread for a local shell), so it writes an *anchor*
//! instead -- one blank cell carrying an OSC 8 hyperlink
//! `terminaal-image:<placement>` at the cursor, then moves the cursor past
//! the image the way kitty does (down by its rows, scrolling if need be,
//! and right by its columns). The anchor scrolls, wraps and gets erased
//! with everything else; the renderer draws each placement whose anchor
//! is on screen (or above it, for an image reaching into view).
//!
//! What's supported: direct transmission (`t=d`), chunked (`m=1`), RGB,
//! RGBA and PNG, zlib compression, image ids and numbers, placements with
//! a source rectangle, size in cells and pixel offset, the cursor policy
//! (`C=1`), queries (`a=q`) and deleting by everything, by id, number or
//! id range. Not: files, temporary files and shared memory (refused, so
//! clients fall back to sending the data), animation, unicode
//! placeholders, relative placements, deleting by position. Images
//! always lie above the cell backgrounds and below the text.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::GeneralPurpose;
use base64::engine::{DecodePaddingMode, GeneralPurposeConfig};

/// URI scheme of the anchor cells.
pub const IMAGE_SCHEME: &str = "terminaal-image:";
/// Largest image accepted, decoded, in bytes.
const MAX_IMAGE_BYTES: usize = 64 << 20;
/// Largest width or height accepted, in pixels.
const MAX_SIDE: u32 = 10_000;
/// Decoded images a terminal keeps at most; past that the oldest go.
const QUOTA_BYTES: usize = 256 << 20;
/// Most rows or columns one placement may cover.
const MAX_CELLS: u32 = 1000;

/// What a terminal shows of an image, shared between the thread filtering
/// its output and the renderer.
pub type SharedGraphics = Arc<Mutex<Graphics>>;

#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Straight (not premultiplied) RGBA, row after row.
    pub rgba: Arc<Vec<u8>>,
    /// Unique per transmission: a new image under an old id is a new
    /// texture.
    pub serial: u64,
    number: Option<u32>,
}

/// One time an image is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub image: u32,
    /// The program's id for it (`p`), 0 if it gave none.
    pub id: u32,
    /// The part of the image shown: x, y, width, height in its pixels.
    pub source: [u32; 4],
    /// Cells it covers, from the anchor cell on.
    pub columns: u32,
    pub rows: u32,
    pub size: Size,
    /// Pixels from the anchor cell's top left corner.
    pub offset: [u32; 2],
}

/// How a placement's image fills its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// At its own size in pixels: no size in cells asked for.
    Native,
    /// Scaled into its cells in proportion: only columns or only rows
    /// asked for (the other follows, rounded up to whole cells).
    Fit,
    /// Stretched over exactly its cells, as kitty does with both asked
    /// for -- programs that tile an image cell by cell (yazi) count on it.
    Fill,
}

/// Where the filter puts a placement: an anchor cell for `key`, and how
/// far the cursor goes past it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub key: u64,
    pub columns: u32,
    pub rows: u32,
    pub move_cursor: bool,
}

impl Anchor {
    /// The bytes that write it at the cursor: a blank cell with the
    /// placement's hyperlink, the cursor back on it, then -- unless the
    /// program said not to -- past the image: down its rows less one
    /// (line feeds, which scroll at the bottom and keep the column) and
    /// right by its columns.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = format!("\x1b]8;;{IMAGE_SCHEME}{}\x1b\\ \x1b]8;;\x1b\\\x08", self.key).into_bytes();
        if self.move_cursor {
            out.extend(std::iter::repeat_n(b'\n', self.rows.saturating_sub(1) as usize));
            out.extend(format!("\x1b[{}C", self.columns).into_bytes());
        }
        out
    }
}

/// What the program sent and the terminal should do about it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub anchor: Option<Anchor>,
    /// A reply to send back to the program.
    pub reply: Option<Vec<u8>>,
}

/// The images and placements of one terminal.
#[derive(Default)]
pub struct Graphics {
    /// Off: commands go past unanswered, as if images weren't supported.
    pub enabled: bool,
    images: HashMap<u32, Image>,
    placements: HashMap<u64, Placement>,
    /// A chunked transmission still coming: its first command and the
    /// base64 so far.
    pending: Option<(Controls, Vec<u8>)>,
    /// Cell size in pixels: images without a size in cells cover as many
    /// cells as their pixels need.
    cell: (f32, f32),
    bytes: usize,
    next_serial: u64,
    next_key: u64,
    /// Ids handed out for images that came with a number only, counting
    /// down from the top so they don't meet the program's own.
    next_free_id: u32,
}

impl Graphics {
    pub fn new(enabled: bool, cell: (f32, f32)) -> SharedGraphics {
        Arc::new(Mutex::new(Self { enabled, cell, next_free_id: u32::MAX, ..Self::default() }))
    }

    pub fn set_cell(&mut self, width: f32, height: f32) {
        self.cell = (width, height);
    }

    pub fn image(&self, id: u32) -> Option<&Image> {
        self.images.get(&id)
    }

    pub fn placement(&self, key: u64) -> Option<&Placement> {
        self.placements.get(&key)
    }

    pub fn has_placements(&self) -> bool {
        !self.placements.is_empty()
    }

    /// The most rows any placement covers: how far above the screen an
    /// anchor may be and still have its image reach into view.
    pub fn tallest(&self) -> u32 {
        self.placements.values().map(|placement| placement.rows).max().unwrap_or(0)
    }

    /// One APC body from the program: `G`, then the keys, `;` and the
    /// payload.
    pub fn command(&mut self, body: &[u8]) -> Outcome {
        let body = body.strip_prefix(b"G").unwrap_or(body);
        let (keys, payload) = match body.iter().position(|&b| b == b';') {
            Some(at) => (&body[..at], &body[at + 1..]),
            None => (body, &[][..]),
        };
        let controls = Controls::parse(keys);
        log::trace!("graphics command {:?} ({} payload bytes)", String::from_utf8_lossy(keys), payload.len());
        // The rest of a chunked transmission: only `m` (and `q`) count.
        if let Some((first, mut data)) = self.pending.take() {
            data.extend_from_slice(payload);
            if controls.more {
                if data.len() > MAX_IMAGE_BYTES * 2 {
                    return self.reply(&first, Err("EFBIG:image too large"));
                }
                self.pending = Some((first, data));
                return Outcome::default();
            }
            return self.run(first, &data);
        }
        if controls.more {
            self.pending = Some((controls, payload.to_vec()));
            return Outcome::default();
        }
        self.run(controls, payload)
    }

    fn run(&mut self, controls: Controls, payload: &[u8]) -> Outcome {
        match controls.action {
            b't' | b'T' | b'q' => {
                let image = match decode(&controls, payload) {
                    Ok(image) => image,
                    Err(err) => return self.reply(&controls, Err(err)),
                };
                if controls.action == b'q' {
                    return self.reply(&controls, Ok(()));
                }
                let id = self.store(&controls, image);
                let controls = Controls { image: id, ..controls };
                if controls.action == b'T' {
                    return self.place(&controls);
                }
                self.reply(&controls, Ok(()))
            }
            b'p' => self.place(&controls),
            b'd' => {
                self.delete(&controls);
                Outcome::default()
            }
            _ => self.reply(&controls, Err("EINVAL:unsupported action")),
        }
    }

    /// Keep `image` under the program's id, the id behind its number, or
    /// a new one; returns the id.
    fn store(&mut self, controls: &Controls, mut image: Image) -> u32 {
        let id = if controls.image != 0 {
            controls.image
        } else {
            self.next_free_id -= 1;
            self.next_free_id
        };
        self.next_serial += 1;
        image.serial = self.next_serial;
        image.number = (controls.number != 0).then_some(controls.number);
        self.bytes += image.rgba.len();
        if let Some(old) = self.images.insert(id, image) {
            self.bytes -= old.rgba.len();
        }
        // Over the quota, the oldest go first -- with their placements.
        while self.bytes > QUOTA_BYTES {
            let Some((&oldest, _)) = self.images.iter().filter(|(other, _)| **other != id).min_by_key(|(_, image)| image.serial) else {
                break;
            };
            self.remove_image(oldest);
        }
        id
    }

    fn remove_image(&mut self, id: u32) {
        if let Some(image) = self.images.remove(&id) {
            self.bytes -= image.rgba.len();
        }
        self.placements.retain(|_, placement| placement.image != id);
    }

    /// Show image `controls.image` (or the newest with `controls.number`)
    /// at the cursor.
    fn place(&mut self, controls: &Controls) -> Outcome {
        let id = if controls.image != 0 {
            controls.image
        } else {
            match self.newest_numbered(controls.number) {
                Some(id) => id,
                None => return self.reply(controls, Err("ENOENT:no such image")),
            }
        };
        let Some(image) = self.images.get(&id) else { return self.reply(controls, Err("ENOENT:no such image")) };
        let x = controls.x.min(image.width.saturating_sub(1));
        let y = controls.y.min(image.height.saturating_sub(1));
        let width = if controls.width == 0 { image.width - x } else { controls.width.min(image.width - x) };
        let height = if controls.height == 0 { image.height - y } else { controls.height.min(image.height - y) };
        let (cell_width, cell_height) = (self.cell.0.max(1.0), self.cell.1.max(1.0));
        let (shown_width, shown_height) = (width as f32 + controls.offset_x as f32, height as f32 + controls.offset_y as f32);
        let cells = |pixels: f32, cell: f32| ((pixels / cell).ceil() as u32).clamp(1, MAX_CELLS);
        // Missing sizes in cells follow from the pixels -- in proportion
        // to the other one when that's given.
        let (columns, rows) = match (controls.columns, controls.rows) {
            (0, 0) => (cells(shown_width, cell_width), cells(shown_height, cell_height)),
            (0, rows) => (cells(rows as f32 * cell_height * width as f32 / height as f32, cell_width), rows),
            (columns, 0) => (columns, cells(columns as f32 * cell_width * height as f32 / width as f32, cell_height)),
            (columns, rows) => (columns, rows),
        };
        let placement = Placement {
            image: id,
            id: controls.placement,
            source: [x, y, width, height],
            columns: columns.min(MAX_CELLS),
            rows: rows.min(MAX_CELLS),
            size: match (controls.columns, controls.rows) {
                (0, 0) => Size::Native,
                (0, _) | (_, 0) => Size::Fit,
                _ => Size::Fill,
            },
            offset: [controls.offset_x, controls.offset_y],
        };
        // A placement id names one placement of the image: shown again,
        // it moves.
        if placement.id != 0 {
            self.placements.retain(|_, other| !(other.image == id && other.id == placement.id));
        }
        self.next_key += 1;
        let key = self.next_key;
        self.placements.insert(key, placement);
        let anchor = Anchor { key, columns: placement.columns, rows: placement.rows, move_cursor: !controls.no_cursor_move };
        Outcome { anchor: Some(anchor), ..self.reply(&Controls { image: id, ..*controls }, Ok(())) }
    }

    fn newest_numbered(&self, number: u32) -> Option<u32> {
        (number != 0).then_some(())?;
        self.images.iter().filter(|(_, image)| image.number == Some(number)).max_by_key(|(_, image)| image.serial).map(|(&id, _)| id)
    }

    /// `d=…`: lowercase removes placements, uppercase their images too.
    fn delete(&mut self, controls: &Controls) {
        let free = controls.delete.is_ascii_uppercase();
        let ids: Vec<u32> = match controls.delete.to_ascii_lowercase() {
            b'a' => {
                self.placements.clear();
                if free {
                    let ids: Vec<u32> = self.images.keys().copied().collect();
                    ids.into_iter().for_each(|id| self.remove_image(id));
                }
                return;
            }
            b'i' => vec![controls.image],
            b'n' => self.newest_numbered(controls.number).into_iter().collect(),
            b'r' => self.images.keys().copied().filter(|id| (controls.x..=controls.y).contains(id)).collect(),
            other => {
                log::debug!("graphics: deleting by {:?} isn't supported", other as char);
                return;
            }
        };
        for id in ids {
            if free && controls.placement == 0 {
                self.remove_image(id);
            } else {
                self.placements.retain(|_, placement| !(placement.image == id && (controls.placement == 0 || placement.id == controls.placement)));
            }
        }
    }

    /// The answer to `controls`, if the program wants one: only when it
    /// named the image (kitty doesn't answer anonymous commands), and
    /// `q=1` keeps OKs, `q=2` errors too, to itself.
    fn reply(&self, controls: &Controls, result: Result<(), &str>) -> Outcome {
        let quiet = match result {
            Ok(()) => controls.quiet >= 1,
            Err(_) => controls.quiet >= 2,
        };
        // An id handed out by us isn't the program's to know.
        let image = if controls.image >= self.next_free_id { 0 } else { controls.image };
        if quiet || (image == 0 && controls.number == 0) {
            return Outcome::default();
        }
        let mut keys = match (image, controls.number) {
            (0, number) => format!("I={number}"),
            (image, 0) => format!("i={image}"),
            (image, number) => format!("i={image},I={number}"),
        };
        if controls.placement != 0 {
            keys.push_str(&format!(",p={}", controls.placement));
        }
        let message = match result {
            Ok(()) => "OK",
            Err(err) => err,
        };
        Outcome { anchor: None, reply: Some(format!("\x1b_G{keys};{message}\x1b\\").into_bytes()) }
    }
}

/// The keys of one command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Controls {
    action: u8,
    format: u32,
    medium: u8,
    compressed: bool,
    pixel_width: u32,
    pixel_height: u32,
    image: u32,
    number: u32,
    placement: u32,
    more: bool,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    offset_x: u32,
    offset_y: u32,
    columns: u32,
    rows: u32,
    no_cursor_move: bool,
    quiet: u32,
    delete: u8,
}

impl Controls {
    fn parse(keys: &[u8]) -> Self {
        let mut controls = Controls {
            action: b'T',
            format: 32,
            medium: b'd',
            compressed: false,
            pixel_width: 0,
            pixel_height: 0,
            image: 0,
            number: 0,
            placement: 0,
            more: false,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            offset_x: 0,
            offset_y: 0,
            columns: 0,
            rows: 0,
            no_cursor_move: false,
            quiet: 0,
            delete: b'a',
        };
        for pair in keys.split(|&b| b == b',') {
            let [key, b'=', value @ ..] = pair else { continue };
            let number = || std::str::from_utf8(value).ok().and_then(|text| text.parse::<u32>().ok()).unwrap_or(0);
            let letter = value.first().copied().unwrap_or(0);
            match key {
                b'a' => controls.action = letter,
                b'f' => controls.format = number(),
                b't' => controls.medium = letter,
                b'o' => controls.compressed = letter == b'z',
                b's' => controls.pixel_width = number(),
                b'v' => controls.pixel_height = number(),
                b'i' => controls.image = number(),
                b'I' => controls.number = number(),
                b'p' => controls.placement = number(),
                b'm' => controls.more = number() == 1,
                b'x' => controls.x = number(),
                b'y' => controls.y = number(),
                b'w' => controls.width = number(),
                b'h' => controls.height = number(),
                b'X' => controls.offset_x = number(),
                b'Y' => controls.offset_y = number(),
                b'c' => controls.columns = number(),
                b'r' => controls.rows = number(),
                b'C' => controls.no_cursor_move = number() == 1,
                b'q' => controls.quiet = number(),
                b'd' => controls.delete = letter,
                _ => {}
            }
        }
        controls
    }
}

/// The image a transmission carries, decoded to RGBA.
fn decode(controls: &Controls, payload: &[u8]) -> Result<Image, &'static str> {
    if controls.medium != b'd' {
        // Files and shared memory aren't read: a program on a server
        // would name files on this computer. Clients send the data then.
        return Err("EINVAL:only direct transmission is supported");
    }
    let lenient = GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
    let data = GeneralPurpose::new(&base64::alphabet::STANDARD, lenient)
        .decode(payload.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect::<Vec<u8>>())
        .map_err(|_| "EINVAL:bad base64")?;
    let data = if controls.compressed {
        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, MAX_IMAGE_BYTES).map_err(|_| "EINVAL:bad zlib data")?
    } else {
        data
    };
    let (width, height, rgba) = match controls.format {
        24 | 32 => {
            let (width, height) = (controls.pixel_width, controls.pixel_height);
            check_size(width, height)?;
            let channels = if controls.format == 24 { 3 } else { 4 };
            let size = width as usize * height as usize * channels;
            if data.len() < size {
                return Err("ENODATA:too little pixel data");
            }
            let rgba = if channels == 4 {
                data[..size].to_vec()
            } else {
                data[..size].chunks_exact(3).flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255]).collect()
            };
            (width, height, rgba)
        }
        100 => decode_png(&data)?,
        _ => return Err("EINVAL:unknown format"),
    };
    Ok(Image { width, height, rgba: Arc::new(rgba), serial: 0, number: None })
}

fn check_size(width: u32, height: u32) -> Result<(), &'static str> {
    if width == 0 || height == 0 {
        return Err("EINVAL:no size");
    }
    if width > MAX_SIDE || height > MAX_SIDE || width as usize * height as usize * 4 > MAX_IMAGE_BYTES {
        return Err("EFBIG:image too large");
    }
    Ok(())
}

fn decode_png(data: &[u8]) -> Result<(u32, u32, Vec<u8>), &'static str> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|_| "EBADPNG:not a PNG")?;
    let info = reader.info();
    check_size(info.width, info.height)?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("EFBIG:image too large")?];
    let frame = reader.next_frame(&mut buffer).map_err(|_| "EBADPNG:broken PNG")?;
    buffer.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::GrayscaleAlpha => buffer.chunks_exact(2).flat_map(|ga| [ga[0], ga[0], ga[0], ga[1]]).collect(),
        png::ColorType::Rgb => buffer.chunks_exact(3).flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255]).collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("EBADPNG:unexpanded palette"),
    };
    Ok((frame.width, frame.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graphics() -> Graphics {
        Graphics { enabled: true, cell: (10.0, 20.0), next_free_id: u32::MAX, ..Graphics::default() }
    }

    fn b64(data: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(data)
    }

    fn reply(outcome: &Outcome) -> String {
        String::from_utf8(outcome.reply.clone().unwrap_or_default()).unwrap().replace('\x1b', "⎋")
    }

    /// A 2×1 PNG, red then half-transparent blue, made with the png crate.
    fn png() -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        out
    }

    #[test]
    fn transmit_and_show_at_once() {
        let mut graphics = graphics();
        // 25×30 pixels in 10×20 cells: 3 columns, 2 rows.
        let rgb = vec![7u8; 25 * 30 * 3];
        let outcome = graphics.command(format!("Gf=24,s=25,v=30,i=5;{}", b64(&rgb)).as_bytes());
        assert_eq!(reply(&outcome), "⎋_Gi=5;OK⎋\\");
        let anchor = outcome.anchor.unwrap();
        assert_eq!((anchor.columns, anchor.rows, anchor.move_cursor), (3, 2, true));
        let placement = graphics.placement(anchor.key).unwrap();
        assert_eq!((placement.image, placement.source, placement.size), (5, [0, 0, 25, 30], Size::Native));
        let image = graphics.image(5).unwrap();
        assert_eq!((image.width, image.height, image.rgba.len()), (25, 30, 25 * 30 * 4));
        assert_eq!(&image.rgba[..4], [7, 7, 7, 255]);
        assert_eq!(
            String::from_utf8(anchor.bytes()).unwrap(),
            format!("\x1b]8;;terminaal-image:{}\x1b\\ \x1b]8;;\x1b\\\x08\n\x1b[3C", anchor.key)
        );
    }

    #[test]
    fn png_in_chunks_then_placed_twice() {
        let mut graphics = graphics();
        let data = b64(&png());
        let (first, rest) = data.split_at(8);
        assert_eq!(graphics.command(format!("Ga=t,f=100,i=9,q=0,m=1;{first}").as_bytes()), Outcome::default());
        let done = graphics.command(format!("Gm=0;{rest}").as_bytes());
        assert_eq!(reply(&done), "⎋_Gi=9;OK⎋\\");
        assert_eq!(done.anchor, None, "only transmitted");
        assert_eq!(graphics.image(9).unwrap().rgba.as_slice(), [255, 0, 0, 255, 0, 0, 255, 128]);

        // Placed in 4 columns, rows to keep its proportions; no cursor
        // movement, and an OK with the placement id.
        let placed = graphics.command(b"Ga=p,i=9,p=2,c=4,C=1");
        assert_eq!(reply(&placed), "⎋_Gi=9,p=2;OK⎋\\");
        let anchor = placed.anchor.unwrap();
        assert_eq!((anchor.columns, anchor.rows, anchor.move_cursor), (4, 1, false));
        assert_eq!(graphics.placement(anchor.key).unwrap().size, Size::Fit);
        let tile = graphics.command(b"Ga=p,i=9,x=1,w=1,c=1,r=1,C=1").anchor.unwrap();
        assert_eq!(graphics.placement(tile.key).map(|placement| (placement.size, placement.source)), Some((Size::Fill, [1, 0, 1, 1])));
        assert!(!String::from_utf8(anchor.bytes()).unwrap().contains('\n'));
        // The same placement id again moves it.
        let again = graphics.command(b"Ga=p,i=9,p=2,q=1").anchor.unwrap();
        assert_eq!(graphics.placement(anchor.key), None);
        assert!(graphics.placement(again.key).is_some());
    }

    #[test]
    fn queries_errors_and_quiet() {
        let mut graphics = graphics();
        // yazi's and icat's probe: a 1×1 RGB image, not kept.
        let probe = graphics.command(b"Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA");
        assert_eq!(reply(&probe), "⎋_Gi=31;OK⎋\\");
        assert!(graphics.image(31).is_none());
        // Files are never read.
        let file = graphics.command(format!("Gi=32,a=q,t=f,f=100;{}", b64(b"/etc/passwd")).as_bytes());
        assert!(reply(&file).starts_with("⎋_Gi=32;EINVAL"), "{}", reply(&file));
        assert_eq!(graphics.command(b"Ga=p,i=77").reply.map(|r| r.starts_with(b"\x1b_Gi=77;ENOENT")), Some(true));
        assert_eq!(graphics.command(b"Ga=p,i=77,q=2"), Outcome::default());
        assert_eq!(graphics.command(b"Gi=33,s=4,v=4,f=32;AAAA").reply.map(|r| r.starts_with(b"\x1b_Gi=33;ENODATA")), Some(true));
        // Without an id, no answer either way.
        assert_eq!(graphics.command(b"Gs=1,v=1,f=24;AAAA").reply, None);
    }

    #[test]
    fn numbers_compression_and_deleting() {
        let mut graphics = graphics();
        let pixels = [9u8; 4];
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&pixels, 6);
        let outcome = graphics.command(format!("Ga=T,I=3,f=32,o=z,s=1,v=1;{}", b64(&packed)).as_bytes());
        assert_eq!(reply(&outcome), "⎋_GI=3;OK⎋\\");
        let first = outcome.anchor.unwrap().key;
        let second = graphics.command(b"Ga=p,I=3").anchor.unwrap().key;
        assert_eq!(graphics.tallest(), 1);
        graphics.command(b"Ga=d,d=n,I=3");
        assert!(graphics.placement(first).is_none() && graphics.placement(second).is_none());
        assert_eq!(graphics.images.len(), 1, "lowercase keeps the image");
        graphics.command(b"Ga=d,d=A");
        assert!(graphics.images.is_empty() && !graphics.has_placements());
        assert_eq!(graphics.bytes, 0);
    }

    #[test]
    fn the_quota_drops_the_oldest() {
        let mut graphics = graphics();
        let image = |_| Image { width: 1, height: 1, rgba: Arc::new(vec![0; QUOTA_BYTES / 3]), serial: 0, number: None };
        let controls = |id| Controls { image: id, ..Controls::parse(b"") };
        for id in 1..=4 {
            graphics.store(&controls(id), image(id));
        }
        let mut kept: Vec<u32> = graphics.images.keys().copied().collect();
        kept.sort();
        assert_eq!(kept, [2, 3, 4]);
    }
}
