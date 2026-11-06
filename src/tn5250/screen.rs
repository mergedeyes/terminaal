//! The 5250 display station: the screen buffer, the format table (input
//! fields), the cursor and the keyboard, and the data stream that changes
//! them (IBM 5494 Functions Reference; lib5250 from tn5250 as the working
//! example of what IBM i actually sends).
//!
//! The screen is a buffer of EBCDIC bytes, row by row. Bytes 0x20..=0x3F
//! are attributes -- they take a position of their own, show as a blank
//! and set color and highlight for what follows. An input field is a run
//! of positions behind its attribute plus an entry in the format table:
//! its field format word (FFW: bypass, modified, data type, adjustment)
//! and length. Typing happens here, locally; nothing goes to the server
//! until an AID key (Enter, F1-F24, Roll, ...) answers its pending read
//! with the cursor position and the modified fields.
//!
//! Records in ([`Screen::record`]) and keys ([`Screen::key`]) both return
//! the [`Packet`]s to send. The server is not trusted: every address and
//! length is checked, bad ones end the record with a negative response.

use super::codepage::Codepage;

/// Record header opcodes (RFC 1205).
const OP_NO_OP: u8 = 0x00;
const OP_PUT_GET: u8 = 0x03;
const OP_SAVE_SCREEN: u8 = 0x04;
const OP_RESTORE_SCREEN: u8 = 0x05;
const OP_CANCEL_INVITE: u8 = 0x0A;
const OP_MESSAGE_ON: u8 = 0x0B;
const OP_MESSAGE_OFF: u8 = 0x0C;

/// Record header flags.
const FLAG_ERROR: u8 = 0x80;
const FLAG_ATTENTION: u8 = 0x40;
const FLAG_SYSTEM_REQUEST: u8 = 0x04;

/// Commands, each behind an ESC.
const ESC: u8 = 0x04;
const CMD_CLEAR_UNIT: u8 = 0x40;
const CMD_CLEAR_UNIT_ALTERNATE: u8 = 0x20;
const CMD_CLEAR_FORMAT_TABLE: u8 = 0x50;
const CMD_WRITE_TO_DISPLAY: u8 = 0x11;
const CMD_WRITE_ERROR_CODE: u8 = 0x21;
const CMD_WRITE_ERROR_CODE_WINDOW: u8 = 0x22;
const CMD_READ_INPUT_FIELDS: u8 = 0x42;
const CMD_READ_MDT_FIELDS: u8 = 0x52;
const CMD_READ_MDT_FIELDS_ALT: u8 = 0x82;
const CMD_READ_SCREEN_IMMEDIATE: u8 = 0x62;
const CMD_READ_IMMEDIATE: u8 = 0x72;
const CMD_SAVE_SCREEN: u8 = 0x02;
const CMD_SAVE_PARTIAL_SCREEN: u8 = 0x03;
const CMD_RESTORE_SCREEN: u8 = 0x12;
const CMD_RESTORE_PARTIAL_SCREEN: u8 = 0x13;
const CMD_ROLL: u8 = 0x23;
const CMD_WRITE_STRUCTURED_FIELD: u8 = 0xF3;

/// Orders inside Write To Display.
const SOH: u8 = 0x01;
const RA: u8 = 0x02;
const EA: u8 = 0x03;
const TD: u8 = 0x10;
const SBA: u8 = 0x11;
const WEA: u8 = 0x12;
const IC: u8 = 0x13;
const MC: u8 = 0x14;
const WDSF: u8 = 0x15;
const SF: u8 = 0x1D;

/// AID codes: what a key tells the server.
const AID_ENTER: u8 = 0xF1;
const AID_HELP: u8 = 0xF3;
const AID_ROLL_DOWN: u8 = 0xF4;
const AID_ROLL_UP: u8 = 0xF5;
const AID_PRINT: u8 = 0xF6;
const AID_RECORD_BACKSPACE: u8 = 0xF8;
const AID_QUERY_REPLY: u8 = 0x88;

/// Negative responses (5494 Functions Reference, "Data Stream Negative
/// Responses").
const NR_INVALID_COMMAND: u32 = 0x1003_0101;
const NR_INVALID_CLEAR_UNIT_ALT: u32 = 0x1003_0105;
const NR_INVALID_SOH_LENGTH: u32 = 0x1005_012B;
const NR_INVALID_ADDRESS: u32 = 0x1005_0122;
const NR_INVALID_STRUCTURED_FIELD: u32 = 0x1005_0111;

/// Field format word bits.
const FFW_BYPASS: u16 = 0x2000;
const FFW_MODIFIED: u16 = 0x0800;
const FFW_TYPE: u16 = 0x0700;
const FFW_ALPHA_ONLY: u16 = 0x0100;
const FFW_NUMERIC_ONLY: u16 = 0x0300;
const FFW_DIGITS_ONLY: u16 = 0x0500;
const FFW_MAGNETIC: u16 = 0x0600;
const FFW_SIGNED: u16 = 0x0700;
const FFW_AUTO_ENTER: u16 = 0x0080;
const FFW_EXIT_REQUIRED: u16 = 0x0040;
const FFW_MONOCASE: u16 = 0x0020;
const FFW_ADJUST: u16 = 0x0007;
const ADJUST_ZERO_FILL: u16 = 0x0005;
const ADJUST_BLANK_FILL: u16 = 0x0006;

/// Operator error codes, shown as "X II nnnn" with their text.
pub const ERR_PROTECTED: u16 = 0x0005;
pub const ERR_ALPHA_ONLY: u16 = 0x0008;
pub const ERR_NUMERIC_ONLY: u16 = 0x0009;
pub const ERR_DIGITS_ONLY: u16 = 0x0010;
pub const ERR_SIGN_POSITION: u16 = 0x0011;
pub const ERR_NO_ROOM: u16 = 0x0012;
pub const ERR_FIELD_MINUS: u16 = 0x0016;
pub const ERR_EXIT_REQUIRED: u16 = 0x0018;
pub const ERR_UNDEFINED_CHARACTER: u16 = 0x0027;

/// The display attribute every screen starts with: green, normal.
pub const ATTR_NORMAL: u8 = 0x20;

/// What goes to the server: a record with this header and data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub flags: u8,
    pub opcode: u8,
    pub data: Vec<u8>,
}

impl Packet {
    fn new(opcode: u8, data: Vec<u8>) -> Self {
        Self { flags: 0, opcode, data }
    }

    /// The whole record: length, record type 0x12A0 (display), the
    /// variable header (length 4, flags, reserved, opcode), the data.
    pub fn to_record(&self) -> Vec<u8> {
        let length = self.data.len() + 10;
        let mut out = Vec::with_capacity(length);
        out.extend((length as u16).to_be_bytes());
        out.extend([0x12, 0xA0, 0x00, 0x00, 0x04, self.flags, 0x00, self.opcode]);
        out.extend(&self.data);
        out
    }
}

/// The keys of a 5250 keyboard that do something here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    /// F1-F24.
    Function(u8),
    RollUp,
    RollDown,
    FieldExit,
    FieldPlus,
    FieldMinus,
    Tab,
    Backtab,
    /// From pasted text: the first input field of the next line.
    NewLine,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    Insert,
    EraseEof,
    Reset,
    Attention,
    SystemRequest,
    Print,
    Help,
}

/// Why the keyboard is inhibited ("X II").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inhibit {
    /// A key that wasn't allowed here; its operator error code.
    Operator(u16),
    /// The server wrote an error message (Write Error Code).
    Host,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyboard {
    Unlocked,
    /// "X SYSTEM": waiting for the server.
    Locked,
    Inhibited(Inhibit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Address of the first data position; the attribute sits before it.
    pub start: usize,
    pub len: usize,
    pub ffw: u16,
    /// Field control words, kept for save/restore.
    fcw: Vec<u16>,
    pub attr: u8,
}

impl Field {
    fn end(&self) -> usize {
        self.start + self.len - 1
    }

    fn contains(&self, addr: usize) -> bool {
        (self.start..self.start + self.len).contains(&addr)
    }

    fn bypass(&self) -> bool {
        self.ffw & FFW_BYPASS != 0
    }

    pub fn modified(&self) -> bool {
        self.ffw & FFW_MODIFIED != 0
    }

    fn kind(&self) -> u16 {
        self.ffw & FFW_TYPE
    }

    /// Signed numeric: the last position holds the sign (a field too
    /// short for one is taken as plain numeric).
    fn signed(&self) -> bool {
        self.kind() == FFW_SIGNED && self.len >= 2
    }
}

/// Reads a record's data, `None` past its end.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn next(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes([self.next()?, self.next()?]))
    }

    fn skip(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.data.len());
    }
}

/// A record ends early: the server sent something we can't take.
struct Reject(u32);

pub struct Screen {
    pub rows: usize,
    pub cols: usize,
    cells: Vec<u8>,
    /// The format table, in the order the server defined the fields --
    /// that's the order Tab follows and fields are sent in.
    fields: Vec<Field>,
    pub cursor: usize,
    /// Where the cursor goes when the keyboard unlocks (IC order).
    insert_cursor: Option<usize>,
    /// The format table header (SOH): error line, function key mask.
    header: Vec<u8>,
    /// The read command waiting for an AID key.
    read: Option<u8>,
    pub keyboard: Keyboard,
    pub insert: bool,
    /// A field that requires Field Exit was typed full.
    pub exit_required: bool,
    pub message_waiting: bool,
    /// The error line as it was before a Write Error Code; back on Reset.
    saved_message_line: Option<Vec<u8>>,
    /// The server asked for an alarm.
    bell: bool,
    pub codepage: Codepage,
    /// Terminal type as negotiated: its type and model number go into the
    /// query reply.
    device: (u16, u8),
}

impl Screen {
    /// `device` is the negotiated terminal type and model (3179, 2).
    pub fn new(codepage: Codepage, device: (u16, u8)) -> Self {
        let mut screen = Self {
            rows: 24,
            cols: 80,
            cells: Vec::new(),
            fields: Vec::new(),
            cursor: 0,
            insert_cursor: None,
            header: Vec::new(),
            read: None,
            keyboard: Keyboard::Locked,
            insert: false,
            exit_required: false,
            message_waiting: false,
            saved_message_line: None,
            bell: false,
            codepage,
            device,
        };
        screen.clear_unit(24, 80);
        screen
    }

    fn size(&self) -> usize {
        self.rows * self.cols
    }

    /// The EBCDIC byte at an address.
    pub fn cell(&self, addr: usize) -> u8 {
        self.cells[addr]
    }

    pub fn row_col(&self, addr: usize) -> (usize, usize) {
        (addr / self.cols, addr % self.cols)
    }

    /// The error line (format table header, else the last row).
    pub fn message_line(&self) -> usize {
        match self.header.get(3) {
            Some(&row) if (1..=self.rows).contains(&usize::from(row)) => usize::from(row) - 1,
            _ => self.rows - 1,
        }
    }

    /// Whether a bell is due; asking clears it.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell)
    }

    /// An input field covering `addr`.
    fn field_at(&self, addr: usize) -> Option<usize> {
        self.fields.iter().position(|field| field.contains(addr))
    }

    /// 1-based row and column to an address; `None` off the screen.
    fn address(&self, row: u8, col: u8) -> Option<usize> {
        let (row, col) = (usize::from(row), usize::from(col));
        ((1..=self.rows).contains(&row) && (1..=self.cols).contains(&col)).then(|| (row - 1) * self.cols + col - 1)
    }

    fn read_address(&self, reader: &mut Reader) -> Result<usize, Reject> {
        let (row, col) = (reader.next(), reader.next());
        row.zip(col).and_then(|(row, col)| self.address(row, col)).ok_or(Reject(NR_INVALID_ADDRESS))
    }

    fn put(&mut self, byte: u8) {
        let addr = self.cursor;
        self.cells[addr] = byte;
        self.cursor = (addr + 1) % self.size();
    }

    // -- Records from the server --

    /// Take one record (without the telnet framing); what to answer.
    pub fn record(&mut self, record: &[u8]) -> Vec<Packet> {
        let mut out = Vec::new();
        // Length (2), record type (2), reserved (2), then the variable
        // header: its length, flags, a reserved byte, the opcode.
        let Some(&header_len) = record.get(6) else { return out };
        let data_start = 6 + usize::from(header_len);
        if header_len < 4 || record.len() < data_start || record[2..4] != [0x12, 0xA0] {
            log::debug!("tn5250: ignoring a record that isn't a display record");
            return out;
        }
        let opcode = record[9];
        match opcode {
            OP_CANCEL_INVITE => out.push(Packet::new(OP_CANCEL_INVITE, Vec::new())),
            OP_MESSAGE_ON => {
                self.message_waiting = true;
                self.bell = true;
            }
            OP_MESSAGE_OFF => self.message_waiting = false,
            _ => {}
        }
        let mut reader = Reader { data: &record[data_start..], pos: 0 };
        if let Err(Reject(code)) = self.commands(&mut reader, opcode, &mut out) {
            log::debug!("tn5250: rejecting a record: {code:#010x}");
            out.push(Packet { flags: FLAG_ERROR, opcode: OP_NO_OP, data: code.to_be_bytes().to_vec() });
        }
        out
    }

    fn commands(&mut self, reader: &mut Reader, opcode: u8, out: &mut Vec<Packet>) -> Result<(), Reject> {
        while let Some(byte) = reader.next() {
            if byte != ESC {
                // Not a command: nothing more we can make sense of.
                log::debug!("tn5250: expected ESC, got {byte:#04x}");
                return Ok(());
            }
            let Some(command) = reader.next() else { return Ok(()) };
            match command {
                CMD_CLEAR_UNIT => self.clear_unit(24, 80),
                CMD_CLEAR_UNIT_ALTERNATE => match reader.next() {
                    Some(0x00 | 0x80) => self.clear_unit(27, 132),
                    _ => return Err(Reject(NR_INVALID_CLEAR_UNIT_ALT)),
                },
                CMD_CLEAR_FORMAT_TABLE => {
                    self.fields.clear();
                    self.read = None;
                    self.cursor = 0;
                    self.keyboard = Keyboard::Locked;
                    self.insert = false;
                }
                CMD_WRITE_TO_DISPLAY => self.write_to_display(reader, opcode)?,
                CMD_WRITE_ERROR_CODE | CMD_WRITE_ERROR_CODE_WINDOW => self.write_error_code(reader, command),
                CMD_READ_INPUT_FIELDS | CMD_READ_MDT_FIELDS | CMD_READ_MDT_FIELDS_ALT => {
                    let (Some(cc1), Some(cc2)) = (reader.next(), reader.next()) else { return Ok(()) };
                    self.control1(cc1);
                    self.control2(cc2);
                    if self.keyboard == Keyboard::Locked {
                        self.keyboard = Keyboard::Unlocked;
                    }
                    self.read = Some(command);
                }
                CMD_READ_SCREEN_IMMEDIATE => out.push(Packet::new(OP_NO_OP, self.cells.clone())),
                CMD_READ_IMMEDIATE => {
                    let pending = self.read.replace(CMD_READ_IMMEDIATE);
                    out.push(self.fields_packet(0));
                    self.read = pending;
                }
                CMD_SAVE_SCREEN => out.push(Packet::new(OP_SAVE_SCREEN, self.save())),
                CMD_SAVE_PARTIAL_SCREEN => {
                    // Flag, top row, left column, depth, width: we save it all.
                    reader.skip(5);
                    out.push(Packet::new(OP_SAVE_SCREEN, self.save()));
                }
                // What follows is the Write To Display our save produced.
                CMD_RESTORE_SCREEN | CMD_RESTORE_PARTIAL_SCREEN => {}
                CMD_ROLL => {
                    let (Some(direction), Some(top), Some(bottom)) = (reader.next(), reader.next(), reader.next()) else {
                        return Ok(());
                    };
                    self.roll(direction, top, bottom)?;
                }
                CMD_WRITE_STRUCTURED_FIELD => self.structured_field(reader, out)?,
                // Read Screen Extended/to Print and the like: for enhanced
                // or printing stations, which we don't claim to be.
                0x0A | 0x64 | 0x66 | 0x68 | 0x6A | 0x6C | 0x83 => {}
                _ => {
                    log::debug!("tn5250: unknown command {command:#04x}");
                    return Err(Reject(NR_INVALID_COMMAND));
                }
            }
        }
        Ok(())
    }

    fn clear_unit(&mut self, rows: usize, cols: usize) {
        self.rows = rows;
        self.cols = cols;
        self.cells = vec![0; rows * cols];
        self.fields.clear();
        self.header.clear();
        self.cursor = 0;
        self.insert_cursor = None;
        self.read = None;
        self.keyboard = Keyboard::Locked;
        self.insert = false;
        self.exit_required = false;
        self.saved_message_line = None;
    }

    /// The first control character of Write To Display and the reads:
    /// lock the keyboard, reset modified flags, null fields.
    fn control1(&mut self, cc1: u8) {
        let (lock, reset_mdt, reset_all_mdt, null_mdt, null_all) = match cc1 & 0xE0 {
            0x00 => (false, false, false, false, false),
            0x40 => (true, true, false, false, false),
            0x60 => (true, false, true, false, false),
            0x80 => (true, false, false, true, false),
            0xA0 => (true, true, false, false, true),
            0xC0 => (true, true, false, true, false),
            0xE0 => (true, false, true, false, true),
            _ => (true, false, false, false, false),
        };
        if lock {
            self.keyboard = Keyboard::Locked;
        }
        for i in 0..self.fields.len() {
            let field = &self.fields[i];
            let bypass = field.bypass();
            if !bypass && (null_all || (null_mdt && field.modified())) {
                let (start, len) = (field.start, field.len);
                self.cells[start..start + len].fill(0);
            }
            if reset_all_mdt || (reset_mdt && !bypass) {
                self.fields[i].ffw &= !FFW_MODIFIED;
            }
        }
    }

    /// The second one: message light, alarm, unlocking.
    fn control2(&mut self, cc2: u8) {
        if cc2 & 0x01 != 0 {
            self.message_waiting = true;
        } else if cc2 & 0x02 != 0 {
            self.message_waiting = false;
        }
        if cc2 & 0x04 != 0 {
            self.bell = true;
        }
        if cc2 & 0x08 != 0 && self.keyboard == Keyboard::Locked {
            self.keyboard = Keyboard::Unlocked;
        }
    }

    fn write_to_display(&mut self, reader: &mut Reader, opcode: u8) -> Result<(), Reject> {
        let (Some(cc1), Some(cc2)) = (reader.next(), reader.next()) else { return Ok(()) };
        self.control1(cc1);
        let old_cursor = self.cursor;
        let mut move_cursor = None;
        while let Some(order) = reader.peek() {
            if order == ESC {
                break;
            }
            reader.next();
            match order {
                SOH => {
                    self.fields.clear();
                    self.insert_cursor = None;
                    if self.keyboard == Keyboard::Unlocked {
                        self.keyboard = Keyboard::Locked;
                    }
                    let n = usize::from(reader.next().unwrap_or(0));
                    if n > 7 {
                        return Err(Reject(NR_INVALID_SOH_LENGTH));
                    }
                    self.header = (0..n).filter_map(|_| reader.next()).collect();
                }
                RA => {
                    let end = self.read_address(reader)?;
                    let byte = reader.next().unwrap_or(0);
                    if end < self.cursor {
                        return Err(Reject(NR_INVALID_ADDRESS));
                    }
                    for addr in self.cursor..=end {
                        self.cells[addr] = byte;
                    }
                    self.cursor = (end + 1) % self.size();
                }
                EA => {
                    let end = self.read_address(reader)?;
                    let length = usize::from(reader.next().unwrap_or(0));
                    if end < self.cursor || !(2..=5).contains(&length) {
                        return Err(Reject(NR_INVALID_ADDRESS));
                    }
                    let types: Vec<u8> = (1..length).filter_map(|_| reader.next()).collect();
                    // 0xFF: everything; the other types are extended
                    // attributes, which we don't keep.
                    if types.contains(&0xFF) {
                        self.cells[self.cursor..=end].fill(0);
                    }
                    self.cursor = (end + 1) % self.size();
                }
                TD => {
                    let length = usize::from(reader.u16().unwrap_or(0));
                    if self.cursor + length > self.size() {
                        return Err(Reject(NR_INVALID_ADDRESS));
                    }
                    for _ in 0..length {
                        let Some(byte) = reader.next() else { break };
                        self.put(byte);
                    }
                }
                SBA => self.cursor = self.read_address(reader)?,
                // Extended attributes (color, ...): not claimed, not kept.
                WEA => reader.skip(2),
                IC => self.insert_cursor = Some(self.read_address(reader)?),
                MC => move_cursor = Some(self.read_address(reader)?),
                // Windows, selection fields, scroll bars: for enhanced
                // stations only. The length counts its own two bytes.
                WDSF => {
                    let length = usize::from(reader.u16().unwrap_or(0));
                    reader.skip(length.saturating_sub(2));
                }
                SF => self.start_field(reader)?,
                _ => self.put(order),
            }
        }
        // Where the cursor ends up: a Move Cursor order, else home when the
        // keyboard unlocks (unless asked not to) or a screen comes back,
        // else where it was.
        let unlock = cc2 & 0x08 != 0;
        let keep = cc2 & 0x40 != 0;
        self.cursor = match move_cursor {
            Some(addr) if !keep => addr,
            _ if (unlock && !keep) || opcode == OP_RESTORE_SCREEN => self.home(),
            _ => old_cursor,
        };
        self.control2(cc2);
        Ok(())
    }

    fn start_field(&mut self, reader: &mut Reader) -> Result<(), Reject> {
        let Some(mut byte) = reader.next() else { return Ok(()) };
        // An attribute right away: an output-only field, nothing to enter.
        let mut input = None;
        if byte & 0xE0 != 0x20 {
            let ffw = u16::from_be_bytes([byte, reader.next().unwrap_or(0)]);
            let mut fcw = Vec::new();
            loop {
                byte = reader.next().ok_or(Reject(NR_INVALID_ADDRESS))?;
                if byte & 0xE0 == 0x20 {
                    break;
                }
                fcw.push(u16::from_be_bytes([byte, reader.next().unwrap_or(0)]));
                if fcw.len() > 16 {
                    return Err(Reject(NR_INVALID_ADDRESS));
                }
            }
            if self.keyboard == Keyboard::Unlocked {
                self.keyboard = Keyboard::Locked;
            }
            input = Some((ffw, fcw));
        }
        let attr = byte;
        self.put(attr);
        let len = usize::from(reader.u16().unwrap_or(0));
        let start = self.cursor;
        if start + len > self.size() {
            return Err(Reject(NR_INVALID_ADDRESS));
        }
        let Some((ffw, fcw)) = input else { return Ok(()) };
        if len == 0 {
            return Ok(());
        }
        match self.fields.iter_mut().find(|field| field.start == start) {
            // The same field again (a screen rewritten in place): new
            // format word and attribute, same data.
            Some(field) => {
                field.ffw = ffw;
                field.attr = attr;
            }
            None => {
                let new = Field { start, len, ffw, fcw, attr };
                self.fields.retain(|field| field.end() < new.start || field.start > new.end());
                self.fields.push(new);
            }
        }
        // The field ends with a normal attribute.
        let after = (start + len) % self.size();
        self.cells[after] = ATTR_NORMAL;
        Ok(())
    }

    fn write_error_code(&mut self, reader: &mut Reader, command: u8) {
        if command == CMD_WRITE_ERROR_CODE_WINDOW {
            reader.skip(2);
        }
        let mut cursor = self.cursor;
        let mut message = Vec::new();
        while let Some(byte) = reader.peek() {
            if byte == ESC {
                break;
            }
            reader.next();
            if byte == IC {
                if let (Some(row), Some(col)) = (reader.next(), reader.next())
                    && let Some(addr) = self.address(row, col)
                {
                    cursor = addr;
                }
                continue;
            }
            if message.len() < self.cols {
                message.push(byte);
            }
        }
        let line = self.message_line() * self.cols;
        if self.saved_message_line.is_none() {
            self.saved_message_line = Some(self.cells[line..line + self.cols].to_vec());
        }
        message.resize(self.cols, 0);
        self.cells[line..line + self.cols].copy_from_slice(&message);
        self.cursor = cursor;
        self.keyboard = Keyboard::Inhibited(Inhibit::Host);
    }

    fn roll(&mut self, direction: u8, top: u8, bottom: u8) -> Result<(), Reject> {
        let (top, bottom) = (usize::from(top), usize::from(bottom));
        let lines = usize::from(direction & 0x1F);
        if top == 0 || bottom > self.rows || top > bottom {
            return Err(Reject(NR_INVALID_ADDRESS));
        }
        if lines == 0 {
            return Ok(());
        }
        let (first, last) = ((top - 1) * self.cols, bottom * self.cols);
        let shift = (lines * self.cols).min(last - first);
        let region = &mut self.cells[first..last];
        if direction & 0x80 != 0 {
            region.rotate_right(shift);
            region[..shift].fill(0);
        } else {
            region.rotate_left(shift);
            let len = region.len();
            region[len - shift..].fill(0);
        }
        Ok(())
    }

    fn structured_field(&mut self, reader: &mut Reader, out: &mut Vec<Packet>) -> Result<(), Reject> {
        let length = usize::from(reader.u16().unwrap_or(0));
        let (class, kind) = (reader.next(), reader.next());
        if length < 5 || class != Some(0xD9) {
            return Err(Reject(NR_INVALID_STRUCTURED_FIELD));
        }
        // Length counts itself, class and type.
        reader.skip(length - 4);
        if kind == Some(0x70) {
            out.push(Packet::new(OP_NO_OP, self.query_reply()));
        }
        Ok(())
    }

    /// The answer to the 5250 Query: a plain (not enhanced) display
    /// station of the negotiated type, byte for byte what lib5250 sends.
    fn query_reply(&self) -> Vec<u8> {
        let mut reply = vec![0u8; 61];
        reply[2] = AID_QUERY_REPLY;
        reply[3..8].copy_from_slice(&[0x00, 0x3A, 0xD9, 0x70, 0x80]);
        // Controller hardware class: another emulator; code level 1.1.0.
        reply[8..13].copy_from_slice(&[0x06, 0x00, 0x01, 0x01, 0x00]);
        // A display, not a printer.
        reply[29] = 0x01;
        let digits = |text: String| text.bytes().map(|b| b - b'0' + 0xF0).collect::<Vec<u8>>();
        reply[30..34].copy_from_slice(&digits(format!("{:04}", self.device.0 % 10000)));
        reply[35..37].copy_from_slice(&digits(format!("{:02}", self.device.1 % 100)));
        // Standard keyboard; serial number; up to 65535 input fields.
        reply[37] = 0x02;
        reply[41] = 0x61;
        reply[42] = 0x50;
        reply[44] = 0xFF;
        reply[45] = 0xFF;
        // Controller/display capability.
        reply[49] = 0x23;
        reply[50] = 0x31;
        reply
    }

    /// The data stream that puts this screen back as it is: sent with
    /// Save Screen, it comes back after Restore Screen.
    fn save(&self) -> Vec<u8> {
        let mut out = match self.rows {
            24 => vec![ESC, CMD_CLEAR_UNIT],
            _ => vec![ESC, CMD_CLEAR_UNIT_ALTERNATE, 0x00],
        };
        out.extend([ESC, CMD_WRITE_TO_DISPLAY, 0x00, 0x00]);
        if !self.header.is_empty() {
            out.extend([SOH, self.header.len() as u8]);
            out.extend(&self.header);
        }
        out.extend([SBA, 1, 1]);
        let size = self.size();
        for addr in 0..size {
            if let Some(field) = self.fields.iter().find(|field| field.start == addr + 1) {
                out.push(SF);
                out.extend(field.ffw.to_be_bytes());
                for fcw in &field.fcw {
                    out.extend(fcw.to_be_bytes());
                }
                out.push(field.attr);
                out.extend((field.len as u16).to_be_bytes());
                continue;
            }
            let byte = self.cells[addr];
            if matches!(byte, SOH | RA | EA | ESC | TD | SBA | WEA | IC | MC | WDSF | SF) {
                // Can't be written as data; leave it out.
                let next = (addr + 1) % size;
                let (row, col) = self.row_col(next);
                out.extend([SBA, row as u8 + 1, col as u8 + 1]);
            } else {
                out.push(byte);
            }
        }
        let (row, col) = self.row_col(self.cursor);
        out.extend([IC, row as u8 + 1, col as u8 + 1]);
        if let Some(read) = self.read {
            out.extend([ESC, read, 0x00, 0x00]);
        }
        out
    }

    // -- The keyboard --

    /// Where the cursor goes home to: the IC position, else the first
    /// input field, else the top left.
    fn home(&self) -> usize {
        self.insert_cursor
            .or_else(|| self.fields.iter().find(|field| !field.bypass()).map(|field| field.start))
            .unwrap_or(0)
    }

    fn inhibit(&mut self, code: u16) {
        self.keyboard = Keyboard::Inhibited(Inhibit::Operator(code));
        self.bell = true;
    }

    /// Reset: the keyboard is free again, the error line as it was.
    fn uninhibit(&mut self) {
        if let Some(line) = self.saved_message_line.take() {
            let start = self.message_line() * self.cols;
            self.cells[start..start + self.cols].copy_from_slice(&line);
        }
        self.keyboard = Keyboard::Unlocked;
    }

    /// Whether typing goes anywhere right now.
    pub fn typing(&self) -> bool {
        self.keyboard == Keyboard::Unlocked
    }

    /// The cursor is in a numeric field: keypad + and − are Field+ and
    /// Field− there.
    pub fn in_numeric_field(&self) -> bool {
        self.input_field().is_some_and(|i| matches!(self.fields[i].kind(), FFW_SIGNED | FFW_NUMERIC_ONLY))
    }

    /// A key, typed; what to send.
    pub fn key(&mut self, key: Key) -> Vec<Packet> {
        match self.keyboard {
            Keyboard::Unlocked => {}
            // Waiting for the server: only what interrupts it.
            Keyboard::Locked => {
                if !matches!(key, Key::Attention | Key::SystemRequest) {
                    self.bell = true;
                    return Vec::new();
                }
            }
            // After an error, Reset only clears it; any other key clears it
            // and goes on -- friendlier than insisting on Reset.
            Keyboard::Inhibited(_) => {
                self.uninhibit();
                if key == Key::Reset {
                    return Vec::new();
                }
            }
        }

        // A field that needs Field Exit is full: little else will do.
        let mut clear_exit = false;
        if self.exit_required {
            match key {
                Key::Left => {
                    self.exit_required = false;
                    return Vec::new();
                }
                Key::Up | Key::Down | Key::Right => self.exit_required = false,
                Key::Enter
                | Key::FieldExit
                | Key::FieldPlus
                | Key::FieldMinus
                | Key::Tab
                | Key::Backtab
                | Key::Reset
                | Key::Help
                | Key::Function(_)
                | Key::RollUp
                | Key::RollDown
                | Key::Attention
                | Key::SystemRequest => clear_exit = true,
                _ => {
                    self.inhibit(ERR_EXIT_REQUIRED);
                    return Vec::new();
                }
            }
        }

        let mut out = Vec::new();
        match key {
            Key::Reset => self.uninhibit(),
            Key::Char(c) => out.extend(self.type_char(c)),
            Key::Enter => out.extend(self.aid(AID_ENTER)),
            Key::Function(n @ 1..=12) => out.extend(self.aid(0x30 + n)),
            Key::Function(n @ 13..=24) => out.extend(self.aid(0xB0 + n - 12)),
            Key::Function(_) => {}
            Key::RollUp => out.extend(self.aid(AID_ROLL_UP)),
            Key::RollDown => out.extend(self.aid(AID_ROLL_DOWN)),
            Key::Help => out.extend(self.aid(AID_HELP)),
            Key::Print => {
                if self.read.is_some() {
                    out.push(self.short_aid(AID_PRINT));
                }
            }
            Key::FieldExit => out.extend(self.field_exit(Exit::Plain)),
            Key::FieldPlus => out.extend(self.field_exit(Exit::Plus)),
            Key::FieldMinus => out.extend(self.field_exit(Exit::Minus)),
            Key::Tab => self.cursor = self.next_field(),
            Key::Backtab => {
                self.cursor = match self.field_at(self.cursor) {
                    Some(i) if self.fields[i].start != self.cursor && !self.fields[i].bypass() => self.fields[i].start,
                    _ => self.previous_field(),
                }
            }
            Key::NewLine => {
                let (row, _) = self.row_col(self.cursor);
                self.cursor = ((row + 1) % self.rows) * self.cols;
                if !self.field_at(self.cursor).is_some_and(|i| !self.fields[i].bypass()) {
                    self.cursor = self.next_field();
                }
            }
            Key::Left => self.cursor = (self.cursor + self.size() - 1) % self.size(),
            Key::Right => self.cursor = (self.cursor + 1) % self.size(),
            Key::Up => self.cursor = (self.cursor + self.size() - self.cols) % self.size(),
            Key::Down => self.cursor = (self.cursor + self.cols) % self.size(),
            Key::Home => {
                let home = self.home();
                if self.cursor == home && self.read.is_some() {
                    out.push(self.short_aid(AID_RECORD_BACKSPACE));
                } else {
                    self.cursor = home;
                }
            }
            Key::End => match self.input_field() {
                Some(i) => {
                    let field = &self.fields[i];
                    let data = &self.cells[field.start..=field.end()];
                    // Behind the last character; on the last position if full.
                    self.cursor = match data.iter().rposition(|&b| b != 0) {
                        Some(last) if last + 1 < field.len => field.start + last + 1,
                        Some(_) => field.end(),
                        None => field.start,
                    };
                }
                None => self.inhibit(ERR_PROTECTED),
            },
            Key::Backspace => match self.input_field() {
                Some(i) if self.fields[i].start == self.cursor => {
                    let previous = self.previous_field();
                    self.cursor = match self.field_at(previous) {
                        Some(p) => self.fields[p].end(),
                        None => previous,
                    };
                }
                Some(_) => {
                    self.cursor -= 1;
                    self.delete_char();
                }
                None => self.inhibit(ERR_PROTECTED),
            },
            Key::Delete => match self.input_field() {
                Some(_) => self.delete_char(),
                None => self.inhibit(ERR_PROTECTED),
            },
            Key::Insert => self.insert = !self.insert,
            Key::EraseEof => match self.input_field() {
                Some(i) => {
                    let end = self.fields[i].end();
                    self.cells[self.cursor..=end].fill(0);
                    self.fields[i].ffw |= FFW_MODIFIED;
                }
                None => self.inhibit(ERR_PROTECTED),
            },
            Key::Attention => out.push(Packet { flags: FLAG_ATTENTION, opcode: OP_NO_OP, data: Vec::new() }),
            Key::SystemRequest => out.push(Packet { flags: FLAG_SYSTEM_REQUEST, opcode: OP_NO_OP, data: Vec::new() }),
        }
        if clear_exit {
            self.exit_required = false;
        }
        out
    }

    /// The input field under the cursor, unless it can't be typed in.
    fn input_field(&self) -> Option<usize> {
        self.field_at(self.cursor).filter(|&i| !self.fields[i].bypass())
    }

    /// Start of the next field to type in, after the cursor's field (or
    /// position) in format table order, round the end.
    fn next_field(&self) -> usize {
        let candidates = self.fields.iter().filter(|field| !field.bypass());
        let after: Vec<&Field> = match self.field_at(self.cursor) {
            Some(i) => self.fields[i + 1..].iter().chain(&self.fields[..=i]).filter(|f| !f.bypass()).collect(),
            None => {
                let mut fields: Vec<&Field> = candidates.collect();
                // Not in a field: the first one after the cursor on screen.
                fields.sort_by_key(|field| (field.start + self.size() - self.cursor) % self.size());
                fields
            }
        };
        after.first().map_or_else(|| self.home(), |field| field.start)
    }

    fn previous_field(&self) -> usize {
        let before: Vec<&Field> = match self.field_at(self.cursor) {
            Some(i) => self.fields[..i].iter().rev().chain(self.fields[i..].iter().rev()).filter(|f| !f.bypass()).collect(),
            None => {
                let mut fields: Vec<&Field> = self.fields.iter().filter(|field| !field.bypass()).collect();
                fields.sort_by_key(|field| (self.cursor + self.size() - field.start) % self.size());
                fields
            }
        };
        before.first().map_or_else(|| self.home(), |field| field.start)
    }

    /// Delete at the cursor: the rest of the field moves left.
    fn delete_char(&mut self) {
        let Some(i) = self.input_field() else { return };
        let end = self.fields[i].end();
        self.cells.copy_within(self.cursor + 1..=end, self.cursor);
        self.cells[end] = 0;
        self.fields[i].ffw |= FFW_MODIFIED;
    }

    fn type_char(&mut self, c: char) -> Vec<Packet> {
        let Some(i) = self.input_field() else {
            self.inhibit(ERR_PROTECTED);
            return Vec::new();
        };
        let field = self.fields[i].clone();
        let c = if field.ffw & FFW_MONOCASE != 0 { c.to_uppercase().next().unwrap_or(c) } else { c };
        let allowed = match field.kind() {
            FFW_ALPHA_ONLY => c.is_alphabetic() || matches!(c, ',' | '.' | '-' | ' '),
            FFW_NUMERIC_ONLY => c.is_ascii_digit() || matches!(c, ',' | '.' | '-' | ' '),
            FFW_DIGITS_ONLY | FFW_SIGNED => c.is_ascii_digit(),
            FFW_MAGNETIC => false,
            _ => true,
        };
        if !allowed {
            self.inhibit(match field.kind() {
                FFW_ALPHA_ONLY => ERR_ALPHA_ONLY,
                FFW_NUMERIC_ONLY => ERR_NUMERIC_ONLY,
                _ => ERR_DIGITS_ONLY,
            });
            return Vec::new();
        }
        let Some(byte) = self.codepage.to_byte(c) else {
            self.inhibit(ERR_UNDEFINED_CHARACTER);
            return Vec::new();
        };
        let at_end = self.cursor == field.end();
        // The last position of a signed field holds the sign.
        if at_end && field.signed() {
            self.inhibit(ERR_SIGN_POSITION);
            return Vec::new();
        }
        if self.insert {
            let last = if field.signed() { field.end() - 1 } else { field.end() };
            if !matches!(self.cells[last], 0 | 0x40) {
                self.inhibit(ERR_NO_ROOM);
                return Vec::new();
            }
            self.cells.copy_within(self.cursor..last, self.cursor + 1);
        }
        self.cells[self.cursor] = byte;
        self.fields[i].ffw |= FFW_MODIFIED;
        if !at_end {
            self.cursor += 1;
            return Vec::new();
        }
        // Typed full.
        if field.ffw & FFW_EXIT_REQUIRED != 0 {
            self.exit_required = true;
            return Vec::new();
        }
        self.adjust(i);
        if field.ffw & FFW_AUTO_ENTER != 0 {
            return self.aid(AID_ENTER);
        }
        self.cursor = self.next_field();
        Vec::new()
    }

    /// Right-adjust a field as its format word asks, zeros or blanks in
    /// front; signed fields always (the sign position stays).
    fn adjust(&mut self, i: usize) {
        let field = &self.fields[i];
        let fill = match field.ffw & FFW_ADJUST {
            _ if field.signed() => 0x40,
            ADJUST_ZERO_FILL => 0xF0,
            ADJUST_BLANK_FILL => 0x40,
            _ => return,
        };
        let end = if field.signed() { field.end() - 1 } else { field.end() };
        let data = &mut self.cells[field.start..=end];
        let content: Vec<u8> = data.iter().copied().filter(|&b| b != 0 && b != 0x40).collect();
        // Blanks inside stay where they are relative to the end.
        let first = data.iter().position(|&b| b != 0 && b != 0x40);
        let last = data.iter().rposition(|&b| b != 0 && b != 0x40);
        if let (Some(first), Some(last)) = (first, last) {
            let kept = data[first..=last].to_vec();
            let len = data.len();
            data.fill(fill);
            data[len - kept.len()..].copy_from_slice(&kept);
        } else if content.is_empty() {
            data.fill(fill);
        }
        self.fields[i].ffw |= FFW_MODIFIED;
    }

    fn field_exit(&mut self, exit: Exit) -> Vec<Packet> {
        let Some(i) = self.input_field() else {
            self.inhibit(ERR_PROTECTED);
            return Vec::new();
        };
        let field = self.fields[i].clone();
        let numeric = matches!(field.kind(), FFW_SIGNED | FFW_NUMERIC_ONLY);
        if exit == Exit::Minus && !numeric {
            self.inhibit(ERR_FIELD_MINUS);
            return Vec::new();
        }
        // The rest of the field goes, unless it was typed full.
        if !self.exit_required {
            let end = if field.signed() { field.end() - 1 } else { field.end() };
            if self.cursor <= end {
                self.cells[self.cursor..=end].fill(0);
            }
        }
        self.adjust(i);
        match exit {
            Exit::Plus if field.signed() => self.cells[field.end()] = 0,
            Exit::Minus if field.signed() => self.cells[field.end()] = 0x60,
            // Numeric only: the last digit's zone becomes negative.
            Exit::Minus => {
                let last = &mut self.cells[field.end()];
                *last = (*last & 0x0F) | 0xD0;
            }
            _ => {}
        }
        if field.ffw & FFW_AUTO_ENTER != 0 {
            return self.aid(AID_ENTER);
        }
        self.cursor = self.next_field();
        Vec::new()
    }

    /// An AID key ending the read with the fields it asks for.
    fn aid(&mut self, aid: u8) -> Vec<Packet> {
        if self.read.is_none() {
            return Vec::new();
        }
        vec![self.fields_packet(aid)]
    }

    /// Print and Record Backspace: only the cursor and the key.
    fn short_aid(&self, aid: u8) -> Packet {
        let (row, col) = self.row_col(self.cursor);
        Packet::new(OP_NO_OP, vec![row as u8 + 1, col as u8 + 1, aid])
    }

    /// Whether the format table header lets this function key send data.
    fn sends_data(&self, aid: u8) -> bool {
        let bit = match aid {
            0x31..=0x3C => usize::from(aid - 0x31),
            0xB1..=0xBC => usize::from(aid - 0xB1) + 12,
            _ => return true,
        };
        if self.header.len() <= 6 {
            return true;
        }
        // F1-F8 in byte 6, F9-F16 in 5, F17-F24 in 4; a set bit: no data.
        let byte = self.header[6 - bit / 8];
        byte & (0x01 << (bit % 8)) == 0
    }

    /// The answer to the pending read: cursor, AID, fields.
    fn fields_packet(&mut self, aid: u8) -> Packet {
        let (row, col) = self.row_col(self.cursor);
        let mut data = vec![row as u8 + 1, col as u8 + 1, aid];
        let read = self.read.unwrap_or(CMD_READ_MDT_FIELDS);
        let any_modified = self.fields.iter().any(Field::modified);
        match read {
            CMD_READ_INPUT_FIELDS | CMD_READ_IMMEDIATE => {
                if any_modified && (read == CMD_READ_IMMEDIATE || self.sends_data(aid)) {
                    for field in &self.fields {
                        self.whole_field(field, &mut data);
                    }
                }
            }
            _ => {
                if self.sends_data(aid) {
                    for field in self.fields.iter().filter(|field| field.modified()) {
                        self.modified_field(field, read == CMD_READ_MDT_FIELDS, &mut data);
                    }
                }
            }
        }
        self.read = None;
        self.keyboard = Keyboard::Locked;
        self.insert = false;
        Packet::new(OP_PUT_GET, data)
    }

    /// Read Input Fields: every position, nulls as blanks; a signed field's
    /// sign goes into the zone of its last digit.
    fn whole_field(&self, field: &Field, data: &mut Vec<u8>) {
        let cells = &self.cells[field.start..=field.end()];
        let blank = |b: u8| if b == 0 { 0x40 } else { b };
        if field.signed() {
            // As lib5250 does it: the digits, then the last one again in
            // the sign position, its zone negative for a minus.
            data.extend(cells[..field.len - 1].iter().map(|&b| blank(b)));
            let last = cells[field.len - 2];
            data.push(if cells[field.len - 1] == 0x60 { 0xD0 | (last & 0x0F) } else { last });
        } else {
            data.extend(cells.iter().map(|&b| blank(b)));
        }
    }

    /// Read MDT Fields: address, then the data without trailing nulls; for
    /// the plain read, inner nulls as blanks.
    fn modified_field(&self, field: &Field, blanks: bool, data: &mut Vec<u8>) {
        let (row, col) = self.row_col(field.start);
        data.extend([SBA, row as u8 + 1, col as u8 + 1]);
        let mut cells = self.cells[field.start..=field.end()].to_vec();
        if field.signed() {
            let sign = cells.pop();
            if let Some(last) = cells.last_mut()
                && sign == Some(0x60)
                && (0xF0..=0xF9).contains(last)
            {
                *last = 0xD0 | (*last & 0x0F);
            }
        }
        while cells.last() == Some(&0) {
            cells.pop();
        }
        data.extend(cells.into_iter().map(|b| if blanks && b == 0 { 0x40 } else { b }));
    }
}

/// Field Exit, Field+ or Field−.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
    Plain,
    Plus,
    Minus,
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub fn screen() -> Screen {
        Screen::new(Codepage::new(37).unwrap(), (3179, 2))
    }

    /// A display record with this opcode and data.
    pub fn record(opcode: u8, data: &[u8]) -> Vec<u8> {
        Packet::new(opcode, data.to_vec()).to_record()
    }

    pub fn ebcdic(text: &str) -> Vec<u8> {
        let page = Codepage::new(37).unwrap();
        text.chars().map(|c| page.to_byte(c).unwrap()).collect()
    }

    /// The screen's text, row by row, trailing blanks cut.
    pub fn text(screen: &Screen) -> Vec<String> {
        (0..screen.rows)
            .map(|row| {
                (0..screen.cols)
                    .map(|col| screen.codepage.to_char(screen.cell(row * screen.cols + col)))
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// A sign-on-like screen: a title, a user field (10, monocase), a
    /// password field (nondisplay), a numeric field; then Read MDT Fields.
    pub fn sign_on() -> Vec<u8> {
        let mut data = vec![ESC, CMD_CLEAR_UNIT, ESC, CMD_WRITE_TO_DISPLAY, 0x00, 0x18];
        data.extend([SOH, 7, 0, 0, 0, 24, 0, 0, 0]);
        data.extend([SBA, 1, 30, 0x22]);
        data.extend(ebcdic("Sign On"));
        data.extend([SBA, 6, 17, 0x20]);
        data.extend(ebcdic("User"));
        data.extend([SBA, 6, 52, SF, 0x40, 0x20, 0x24, 0x00, 10]);
        data.extend([SBA, 7, 52, SF, 0x40, 0x00, 0x27, 0x00, 10]);
        data.extend([SBA, 8, 52, SF, 0x43, 0x05, 0x24, 0x00, 5]);
        data.extend([IC, 6, 53]);
        data.extend([ESC, CMD_READ_MDT_FIELDS, 0x00, 0x00]);
        record(OP_PUT_GET, &data)
    }

    fn type_text(screen: &mut Screen, text: &str) -> Vec<Packet> {
        text.chars().flat_map(|c| screen.key(Key::Char(c))).collect()
    }

    #[test]
    fn draws_a_screen_with_fields_and_unlocks_for_the_read() {
        let mut s = screen();
        assert!(s.record(&sign_on()).is_empty());
        let lines = text(&s);
        assert_eq!(lines[0], format!("{}Sign On", " ".repeat(30)));
        assert_eq!(lines[5], format!("{}User", " ".repeat(17)));
        assert_eq!(s.fields.len(), 3);
        assert_eq!(s.fields[0].start, 5 * 80 + 52);
        // The attribute before the field, a normal one behind it.
        assert_eq!(s.cell(5 * 80 + 51), 0x24);
        assert_eq!(s.cell(5 * 80 + 62), ATTR_NORMAL);
        assert_eq!(s.keyboard, Keyboard::Unlocked);
        // Home is the IC position.
        assert_eq!(s.cursor, 5 * 80 + 52);
        assert_eq!(s.message_line(), 23);
    }

    #[test]
    fn typing_fills_fields_and_enter_sends_the_modified_ones() {
        let mut s = screen();
        s.record(&sign_on());
        // Monocase: upper case on the screen.
        assert!(type_text(&mut s, "qsecofr").is_empty());
        s.key(Key::Tab);
        assert_eq!(s.cursor, 6 * 80 + 52);
        type_text(&mut s, "secret");
        let sent = s.key(Key::Enter);
        assert_eq!(s.keyboard, Keyboard::Locked);
        let mut expected = vec![7, 59, AID_ENTER, SBA, 6, 53];
        expected.extend(ebcdic("QSECOFR"));
        expected.extend([SBA, 7, 53]);
        expected.extend(ebcdic("secret"));
        let length = 10 + expected.len() as u8;
        assert_eq!(sent, vec![Packet::new(OP_PUT_GET, expected)]);
        assert_eq!(sent[0].to_record()[..10], [0, length, 0x12, 0xA0, 0, 0, 4, 0, 0, OP_PUT_GET]);
        // Locked until the server answers.
        assert!(s.key(Key::Char('x')).is_empty());
        assert!(s.key(Key::Enter).is_empty());
    }

    #[test]
    fn typing_a_field_full_moves_on_and_right_adjusts() {
        let mut s = screen();
        s.record(&sign_on());
        s.key(Key::Tab);
        s.key(Key::Tab);
        assert_eq!(s.cursor, 7 * 80 + 52);
        // Numbers only.
        s.key(Key::Char('x'));
        assert_eq!(s.keyboard, Keyboard::Inhibited(Inhibit::Operator(ERR_NUMERIC_ONLY)));
        s.key(Key::Reset);
        assert_eq!(s.keyboard, Keyboard::Unlocked);
        type_text(&mut s, "42");
        // Field Exit: right-adjusted with zeros, on to the next field.
        s.key(Key::FieldExit);
        assert_eq!(&s.cells[7 * 80 + 52..7 * 80 + 57], &ebcdic("00042")[..]);
        assert_eq!(s.cursor, 5 * 80 + 52);
        // Typed full: the next field.
        type_text(&mut s, "ABCDEFGHIJ");
        assert_eq!(s.cursor, 6 * 80 + 52);
    }

    #[test]
    fn typing_outside_a_field_inhibits_until_any_key() {
        let mut s = screen();
        s.record(&sign_on());
        s.key(Key::Up);
        s.key(Key::Char('a'));
        assert_eq!(s.keyboard, Keyboard::Inhibited(Inhibit::Operator(ERR_PROTECTED)));
        assert!(s.take_bell());
        // The next key clears it and still counts.
        s.key(Key::Down);
        assert_eq!(s.keyboard, Keyboard::Unlocked);
        assert_eq!(s.cursor, 5 * 80 + 52);
    }

    #[test]
    fn editing_keys_insert_delete_and_erase() {
        let mut s = screen();
        s.record(&sign_on());
        type_text(&mut s, "ACE");
        s.key(Key::Home);
        s.key(Key::Right);
        s.key(Key::Insert);
        type_text(&mut s, "B");
        assert_eq!(text(&s)[5][52..], *"ABCE");
        s.key(Key::Insert);
        s.key(Key::End);
        assert_eq!(s.cursor, 5 * 80 + 52 + 4);
        s.key(Key::Backspace);
        assert_eq!(text(&s)[5][52..], *"ABC");
        s.key(Key::Home);
        s.key(Key::Delete);
        assert_eq!(text(&s)[5][52..], *"BC");
        s.key(Key::Right);
        s.key(Key::EraseEof);
        assert_eq!(text(&s)[5][52..], *"B");
        // Backspace at a field's start goes to the end of the previous one.
        s.key(Key::Home);
        s.key(Key::Backspace);
        assert_eq!(s.cursor, 7 * 80 + 56);
    }

    #[test]
    fn home_on_home_sends_record_backspace() {
        let mut s = screen();
        s.record(&sign_on());
        let sent = s.key(Key::Home);
        assert_eq!(sent, vec![Packet::new(OP_NO_OP, vec![6, 53, AID_RECORD_BACKSPACE])]);
    }

    #[test]
    fn function_keys_follow_the_header_mask() {
        let mut s = screen();
        s.record(&sign_on());
        type_text(&mut s, "X");
        // The header masks nothing: F3 sends the field.
        let sent = s.key(Key::Function(3));
        assert_eq!(sent[0].data[2], 0x33);
        assert!(sent[0].data.len() > 3);

        let mut s = screen();
        let mut stream = sign_on();
        // Byte 6 of the header, bit for F3 (0x04): no data with F3.
        let soh = 10 + stream[10..].iter().position(|&b| b == SOH).unwrap();
        stream[soh + 2 + 6] = 0x04;
        s.record(&stream);
        type_text(&mut s, "X");
        assert_eq!(s.key(Key::Function(3)), vec![Packet::new(OP_PUT_GET, vec![6, 54, 0x33])]);
        s.record(&stream);
        type_text(&mut s, "X");
        assert_eq!(s.key(Key::Function(15))[0].data[2], 0xB3);
    }

    #[test]
    fn read_input_fields_sends_every_field_padded() {
        let mut s = screen();
        let mut stream = sign_on();
        let read = stream.iter().rposition(|&b| b == CMD_READ_MDT_FIELDS).unwrap();
        stream[read] = CMD_READ_INPUT_FIELDS;
        s.record(&stream);
        type_text(&mut s, "AB");
        let sent = s.key(Key::Enter);
        let data = &sent[0].data;
        assert_eq!(data.len(), 3 + 10 + 10 + 5);
        assert_eq!(&data[3..5], &ebcdic("AB")[..]);
        assert!(data[5..].iter().all(|&b| b == 0x40));
    }

    #[test]
    fn write_error_code_shows_on_the_error_line_until_reset() {
        let mut s = screen();
        s.record(&sign_on());
        let mut data = vec![ESC, CMD_WRITE_ERROR_CODE, 0x22];
        data.extend(ebcdic("Password not correct"));
        data.extend([IC, 7, 53]);
        s.record(&record(OP_PUT_GET, &data));
        assert_eq!(text(&s)[23], " Password not correct");
        assert_eq!(s.keyboard, Keyboard::Inhibited(Inhibit::Host));
        assert_eq!(s.cursor, 6 * 80 + 52);
        s.key(Key::Reset);
        assert_eq!(text(&s)[23], "");
        assert_eq!(s.keyboard, Keyboard::Unlocked);
    }

    #[test]
    fn save_and_restore_bring_back_screen_fields_and_read() {
        let mut s = screen();
        s.record(&sign_on());
        type_text(&mut s, "ME");
        let saved = s.record(&record(OP_NO_OP, &[ESC, CMD_SAVE_SCREEN]));
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].opcode, OP_SAVE_SCREEN);
        let before = (text(&s), s.fields.clone(), s.cursor);

        // Something else on the screen (a System Request line, say).
        s.record(&record(OP_PUT_GET, &[ESC, CMD_CLEAR_UNIT, ESC, CMD_WRITE_TO_DISPLAY, 0, 0, 0xC1]));
        assert_ne!(text(&s), before.0);

        let mut restore = vec![ESC, CMD_RESTORE_SCREEN];
        restore.extend(&saved[0].data);
        s.record(&record(OP_RESTORE_SCREEN, &restore));
        assert_eq!((text(&s), s.fields.clone(), s.cursor), before);
        assert_eq!(s.keyboard, Keyboard::Unlocked);
        // The typed text is still modified and goes out.
        assert_eq!(s.key(Key::Enter)[0].data[3..6], [SBA, 6, 53]);
    }

    #[test]
    fn answers_the_query_and_read_screen() {
        let mut s = screen();
        let out = s.record(&record(OP_NO_OP, &[ESC, CMD_WRITE_STRUCTURED_FIELD, 0, 5, 0xD9, 0x70, 0x00]));
        assert_eq!(out.len(), 1);
        let reply = &out[0].data;
        assert_eq!(reply.len(), 61);
        assert_eq!(&reply[..8], &[0, 0, AID_QUERY_REPLY, 0, 0x3A, 0xD9, 0x70, 0x80]);
        assert_eq!(&reply[30..34], &ebcdic("3179")[..]);
        assert_eq!(&reply[35..37], &ebcdic("02")[..]);

        let out = s.record(&record(OP_NO_OP, &[ESC, CMD_READ_SCREEN_IMMEDIATE]));
        assert_eq!(out[0].data.len(), 24 * 80);
    }

    #[test]
    fn repeat_erase_and_roll() {
        let mut s = screen();
        let mut data = vec![ESC, CMD_WRITE_TO_DISPLAY, 0, 0, SBA, 2, 1, RA, 2, 10];
        data.extend(ebcdic("-"));
        data.extend([SBA, 3, 1]);
        data.extend(ebcdic("abcdef"));
        data.extend([SBA, 3, 2, EA, 3, 3, 2, 0xFF]);
        s.record(&record(OP_PUT_GET, &data));
        assert_eq!(text(&s)[1], "----------");
        assert_eq!(text(&s)[2], "a  def");
        s.record(&record(OP_PUT_GET, &[ESC, CMD_ROLL, 0x81, 1, 24]));
        assert_eq!(text(&s)[2], "----------");
        assert_eq!(text(&s)[1], "");
        s.record(&record(OP_PUT_GET, &[ESC, CMD_ROLL, 0x02, 1, 24]));
        assert_eq!(text(&s)[0], "----------");
        assert_eq!(text(&s)[1], "a  def");
    }

    #[test]
    fn bad_addresses_and_commands_are_refused_not_trusted() {
        for data in [
            vec![ESC, CMD_WRITE_TO_DISPLAY, 0, 0, SBA, 25, 1],
            vec![ESC, CMD_WRITE_TO_DISPLAY, 0, 0, SBA, 1, 81],
            vec![ESC, CMD_WRITE_TO_DISPLAY, 0, 0, SBA, 2, 1, RA, 1, 1, 0x40],
            vec![ESC, CMD_WRITE_TO_DISPLAY, 0, 0, SBA, 24, 70, SF, 0x40, 0, 0x20, 0x00, 0x20],
            vec![ESC, CMD_WRITE_TO_DISPLAY, 0, 0, TD, 0xFF, 0xFF],
            vec![ESC, CMD_WRITE_TO_DISPLAY, 0, 0, SOH, 9],
            vec![ESC, 0x99],
        ] {
            let mut s = screen();
            let out = s.record(&record(OP_PUT_GET, &data));
            assert_eq!(out.len(), 1, "{data:x?}");
            assert_eq!(out[0].flags, FLAG_ERROR);
        }
        // Cut short anywhere: no panic.
        let full = sign_on();
        for len in 0..full.len() {
            screen().record(&full[..len]);
        }
    }

    #[test]
    fn signed_numeric_fields_carry_the_sign_in_the_zone() {
        let mut s = screen();
        let mut data = vec![ESC, CMD_CLEAR_UNIT, ESC, CMD_WRITE_TO_DISPLAY, 0, 0x08];
        data.extend([SBA, 2, 10, SF, 0x47, 0x00, 0x20, 0x00, 4]);
        data.extend([ESC, CMD_READ_MDT_FIELDS, 0, 0]);
        s.record(&record(OP_PUT_GET, &data));
        type_text(&mut s, "12");
        s.key(Key::FieldMinus);
        assert_eq!(&s.cells[90..94], &[0x40, 0xF1, 0xF2, 0x60]);
        let sent = s.key(Key::Enter);
        assert_eq!(&sent[0].data[3..], &[SBA, 2, 11, 0x40, 0xF1, 0xD2]);
    }

    #[test]
    fn attention_and_system_request_go_out_even_while_locked() {
        let mut s = screen();
        assert_eq!(s.keyboard, Keyboard::Locked);
        assert_eq!(s.key(Key::Attention)[0].flags, FLAG_ATTENTION);
        assert_eq!(s.key(Key::SystemRequest)[0].flags, FLAG_SYSTEM_REQUEST);
        assert!(s.key(Key::Char('a')).is_empty());
    }
}
