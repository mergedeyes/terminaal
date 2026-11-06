//! EBCDIC code pages: what the bytes on an IBM i screen are as text, and
//! back. The server sends text in the device's code page (CCSID), which
//! the client asks for while connecting ([`Codepage::environment`]).
//!
//! Only the single-byte Latin ones -- the US/Western European CCSIDs and
//! their euro variants; double-byte (Japanese, Chinese, Korean) pages
//! would need shift-in/shift-out handling the screen doesn't have.

use std::collections::HashMap;

use super::codepages::*;

/// The CCSIDs there are tables for, in the order the host form lists them.
pub const CCSIDS: [u16; 20] = [
    37, 1140, 273, 1141, 277, 1142, 278, 1143, 280, 1144, 284, 1145, 285, 1146, 297, 1147, 500, 1148, 871, 1149,
];

/// What a new host gets: US English, the most common.
pub const DEFAULT_CCSID: u16 = 37;

#[derive(Clone)]
pub struct Codepage {
    ccsid: u16,
    /// What 0x40..=0xFF show as.
    chars: Vec<char>,
    /// Back again; where two bytes show alike, the first wins (0x40, the
    /// blank, rather than 0x41, the no-break space).
    bytes: HashMap<char, u8>,
}

impl Codepage {
    /// `None` for a CCSID without a table.
    pub fn new(ccsid: u16) -> Option<Self> {
        let table = match ccsid {
            37 => CP037,
            273 => CP273,
            277 => CP277,
            278 => CP278,
            280 => CP280,
            284 => CP284,
            285 => CP285,
            297 => CP297,
            500 => CP500,
            871 => CP871,
            1140 => CP1140,
            1141 => CP1141,
            1142 => CP1142,
            1143 => CP1143,
            1144 => CP1144,
            1145 => CP1145,
            1146 => CP1146,
            1147 => CP1147,
            1148 => CP1148,
            1149 => CP1149,
            _ => return None,
        };
        let chars: Vec<char> = table.chars().collect();
        debug_assert_eq!(chars.len(), 0xC0);
        let mut bytes = HashMap::new();
        for (i, &c) in chars.iter().enumerate() {
            bytes.entry(c).or_insert(0x40 + i as u8);
        }
        Some(Self { ccsid, chars, bytes })
    }

    /// The character an EBCDIC byte shows as; controls and attributes
    /// (below 0x40) as a blank.
    pub fn to_char(&self, byte: u8) -> char {
        match byte {
            0x40.. => self.chars[usize::from(byte - 0x40)],
            _ => ' ',
        }
    }

    /// The byte for a typed character; `None` if this page hasn't got it.
    pub fn to_byte(&self, c: char) -> Option<u8> {
        self.bytes.get(&c).copied()
    }

    /// The `NEW-ENVIRON` user variables that make the server create the
    /// device with this code page (RFC 4777): the keyboard type of the
    /// country, the code page and the character set -- 695 has the euro,
    /// 697 is the older Latin-1 set.
    pub fn environment(&self) -> [(&'static str, String); 3] {
        let keyboard = match self.ccsid {
            273 | 1141 => "AGB",
            277 | 1142 => "DMB",
            278 | 1143 => "FNB",
            280 | 1144 => "ITB",
            284 | 1145 => "SPB",
            285 | 1146 => "UKB",
            297 | 1147 => "FAB",
            500 | 1148 => "BLI",
            871 | 1149 => "ICB",
            _ => "USB",
        };
        let charset = if self.ccsid >= 1140 { "695" } else { "697" };
        [("KBDTYPE", keyboard.to_string()), ("CODEPAGE", self.ccsid.to_string()), ("CHARSET", charset.to_string())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_ccsid_has_a_full_table() {
        for ccsid in CCSIDS {
            let page = Codepage::new(ccsid).unwrap();
            assert_eq!(page.chars.len(), 0xC0, "{ccsid}");
            assert_eq!(page.to_char(0x40), ' ');
            assert_eq!(page.to_byte(' '), Some(0x40), "{ccsid}");
            // Letters and digits sit in the same place in all of them.
            assert_eq!(page.to_char(0xC1), 'A');
            assert_eq!(page.to_char(0x81), 'a');
            assert_eq!(page.to_char(0xF0), '0');
            assert_eq!(page.to_byte('Z'), Some(0xE9));
        }
        assert!(Codepage::new(930).is_none());
    }

    #[test]
    fn national_characters_and_the_euro_differ_by_page() {
        let us = Codepage::new(37).unwrap();
        let german = Codepage::new(273).unwrap();
        let german_euro = Codepage::new(1141).unwrap();
        assert_eq!(us.to_char(0x7C), '@');
        assert_eq!(german.to_char(0x7C), '§');
        assert_eq!(german.to_byte('ä'), Some(0xC0));
        assert_eq!(german.to_byte('ß'), Some(0xA1));
        assert_eq!(german.to_byte('€'), None);
        assert_eq!(german_euro.to_char(0x9F), '€');
        assert_eq!(us.to_byte('€'), None);
        // Controls and attributes show as blanks.
        assert_eq!(us.to_char(0x00), ' ');
        assert_eq!(us.to_char(0x22), ' ');
    }

    #[test]
    fn the_environment_names_keyboard_page_and_set() {
        let env = Codepage::new(1141).unwrap().environment();
        assert_eq!(env[0], ("KBDTYPE", "AGB".to_string()));
        assert_eq!(env[1], ("CODEPAGE", "1141".to_string()));
        assert_eq!(env[2], ("CHARSET", "695".to_string()));
        assert_eq!(Codepage::new(37).unwrap().environment()[2].1, "697");
    }
}
