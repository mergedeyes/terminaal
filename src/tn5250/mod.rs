//! IBM i (AS/400) green screens: a 5250 display station over telnet
//! (TN5250, RFC 1205/4777), optionally over TLS.
//!
//! A 5250 terminal is no VT terminal: the server sends whole screens with
//! input fields, the client edits them locally and sends the changed
//! fields back when an AID key (Enter, F1-F24, ...) is pressed. So the
//! emulator is a worker thread that owns the protocol state
//! ([`screen::Screen`]) and draws it as VT output into the pane's `Term`
//! ([`render`]); keys reach it as the bytes the pane would send any
//! program, and it decodes them back ([`keys`]). To the rest of Terminaal
//! it is a terminal session like SSH: same tab, selection, search, themes.
//!
//! - [`telnet`]: option negotiation, records
//! - [`codepage`]: EBCDIC <-> text (CCSID 37, 273, 1141, ...)
//! - [`screen`]: the display, format table, keyboard and data stream
//! - [`render`]: the screen as VT, with a status line
//! - [`keys`]: typed bytes -> 5250 keys
//! - [`connection`]: the worker: TCP/TLS, the pump loop, reconnecting

pub mod codepage;
mod codepages;
mod connection;
mod keys;
mod render;
mod screen;
mod telnet;

use alacritty_terminal::vte::ansi::Rgb;

pub use connection::{Tn5250Handle, spawn};

/// Port of plain telnet, and of telnet over TLS.
pub const PORT: u16 = 23;
pub const TLS_PORT: u16 = 992;

/// Where to connect, and how the device should look.
#[derive(Clone, Debug, PartialEq)]
pub struct Tn5250Target {
    /// The tab title: the host's name in the sidebar.
    pub label: String,
    /// The host's name in the sidebar (session restore finds it by that).
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// EBCDIC code page of the device ([`codepage::CCSIDS`]).
    pub ccsid: u16,
    /// 27×132 as well as 24×80 (IBM-3477-FC instead of IBM-3179-2).
    pub wide: bool,
    /// Device name to ask for (DEVNAME); else the server picks one.
    pub device: Option<String>,
    /// The host's color and theme, as for SSH hosts.
    pub color: Option<Rgb>,
    pub theme: Option<String>,
}

impl Tn5250Target {
    /// The terminal type to negotiate, and its type and model number for
    /// the query reply.
    fn terminal_type(&self) -> (&'static str, (u16, u8)) {
        if self.wide { ("IBM-3477-FC", (3477, 0)) } else { ("IBM-3179-2", (3179, 2)) }
    }

    /// `host:port`, for messages.
    pub fn address(&self) -> String {
        if self.host.contains(':') { format!("[{}]:{}", self.host, self.port) } else { format!("{}:{}", self.host, self.port) }
    }
}
