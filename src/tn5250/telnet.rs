//! The telnet layer under 5250 (RFC 854, RFC 1205, RFC 4777): option
//! negotiation, and cutting the byte stream into records at `IAC EOR`.
//! Pure bytes in, bytes out -- the worker does the I/O.
//!
//! The server asks for the terminal type and the environment, then the
//! binary and end-of-record options both ways; once those stand, it
//! creates the virtual device and sends 5250 records. The environment
//! carries the code page (KBDTYPE/CODEPAGE/CHARSET) and, if set, the
//! device name -- never a user or password: signing on happens on the
//! screen.

pub const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const EOR: u8 = 239;

const BINARY: u8 = 0;
const TERMINAL_TYPE: u8 = 24;
const END_OF_RECORD: u8 = 25;
const NEW_ENVIRON: u8 = 39;

/// Subnegotiation verbs and NEW-ENVIRON types.
const IS: u8 = 0;
const SEND: u8 = 1;
const VAR: u8 = 0;
const VALUE: u8 = 1;
const USERVAR: u8 = 3;

/// A record longer than this is dropped whole (up to its `EOR`): screens
/// take a few KB, and the server is not to be trusted with our memory.
pub const MAX_RECORD: usize = 1 << 20;
/// Subnegotiations are a few dozen bytes.
const MAX_SUBNEGOTIATION: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Data,
    Iac,
    /// After `IAC DO`/`DONT`/`WILL`/`WONT`: the option comes next.
    Verb(u8),
    Sub,
    SubIac,
}

pub struct Telnet {
    state: State,
    record: Vec<u8>,
    /// The record grew past [`MAX_RECORD`]: skip to its end.
    overflow: bool,
    sub: Vec<u8>,
    terminal_type: String,
    /// `USERVAR`s sent when the server asks for the environment.
    environment: Vec<(String, String)>,
    /// Options we said WILL to.
    ours: [bool; 256],
    /// Options we said DO to.
    theirs: [bool; 256],
    /// Options we refused, so a server asking again doesn't loop.
    refused: [bool; 256],
}

impl Telnet {
    pub fn new(terminal_type: &str, environment: Vec<(String, String)>) -> Self {
        Self {
            state: State::Data,
            record: Vec::new(),
            overflow: false,
            sub: Vec::new(),
            terminal_type: terminal_type.to_string(),
            environment,
            ours: [false; 256],
            theirs: [false; 256],
            refused: [false; 256],
        }
    }

    /// Take `data` from the server: complete records go to `records`,
    /// negotiation answers to `reply`.
    pub fn feed(&mut self, data: &[u8], records: &mut Vec<Vec<u8>>, reply: &mut Vec<u8>) {
        for &byte in data {
            self.state = match self.state {
                State::Data if byte == IAC => State::Iac,
                State::Data => {
                    self.push(byte);
                    State::Data
                }
                State::Iac => match byte {
                    IAC => {
                        self.push(IAC);
                        State::Data
                    }
                    EOR => {
                        let record = std::mem::take(&mut self.record);
                        if !std::mem::take(&mut self.overflow) && !record.is_empty() {
                            records.push(record);
                        }
                        State::Data
                    }
                    DO | DONT | WILL | WONT => State::Verb(byte),
                    SB => {
                        self.sub.clear();
                        State::Sub
                    }
                    // NOP, GA, ... mean nothing here.
                    _ => State::Data,
                },
                State::Verb(verb) => {
                    self.negotiate(verb, byte, reply);
                    State::Data
                }
                State::Sub if byte == IAC => State::SubIac,
                State::Sub => {
                    if self.sub.len() < MAX_SUBNEGOTIATION {
                        self.sub.push(byte);
                    }
                    State::Sub
                }
                State::SubIac => match byte {
                    SE => {
                        self.subnegotiation(reply);
                        State::Data
                    }
                    IAC => {
                        if self.sub.len() < MAX_SUBNEGOTIATION {
                            self.sub.push(IAC);
                        }
                        State::Sub
                    }
                    _ => State::Sub,
                },
            };
        }
    }

    fn push(&mut self, byte: u8) {
        if self.record.len() >= MAX_RECORD {
            self.overflow = true;
            self.record.clear();
        }
        if !self.overflow {
            self.record.push(byte);
        }
    }

    fn negotiate(&mut self, verb: u8, option: u8, reply: &mut Vec<u8>) {
        let index = usize::from(option);
        let supported = matches!(option, BINARY | TERMINAL_TYPE | END_OF_RECORD | NEW_ENVIRON);
        // Answer only changes; a refusal once -- otherwise two ends that
        // each confirm the other's confirmation never stop.
        match verb {
            DO if supported && !self.ours[index] => {
                self.ours[index] = true;
                reply.extend([IAC, WILL, option]);
            }
            DO if !supported && !self.refused[index] => {
                self.refused[index] = true;
                reply.extend([IAC, WONT, option]);
            }
            DONT if self.ours[index] => {
                self.ours[index] = false;
                reply.extend([IAC, WONT, option]);
            }
            WILL if matches!(option, BINARY | END_OF_RECORD) && !self.theirs[index] => {
                self.theirs[index] = true;
                reply.extend([IAC, DO, option]);
            }
            WILL if !matches!(option, BINARY | END_OF_RECORD) && !self.refused[index] => {
                self.refused[index] = true;
                reply.extend([IAC, DONT, option]);
            }
            WONT if self.theirs[index] => {
                self.theirs[index] = false;
                reply.extend([IAC, DONT, option]);
            }
            _ => {}
        }
    }

    fn subnegotiation(&mut self, reply: &mut Vec<u8>) {
        match self.sub.as_slice() {
            [TERMINAL_TYPE, SEND, ..] => {
                reply.extend([IAC, SB, TERMINAL_TYPE, IS]);
                reply.extend(self.terminal_type.bytes().filter(|&b| b != IAC));
                reply.extend([IAC, SE]);
            }
            // Whatever it asked for (IBM i names IBMRSEED for password
            // encryption, which only matters for signing on automatically):
            // what we have.
            [NEW_ENVIRON, SEND, ..] => {
                reply.extend([IAC, SB, NEW_ENVIRON, IS]);
                for (name, value) in &self.environment {
                    reply.push(USERVAR);
                    reply.extend(name.bytes().filter(u8::is_ascii_graphic));
                    reply.push(VALUE);
                    reply.extend(value.bytes().filter(u8::is_ascii_graphic));
                }
                reply.extend([IAC, SE]);
            }
            _ => {}
        }
        // `VAR` is unused, but part of the protocol.
        let _ = VAR;
    }
}

/// A record as it goes on the wire: `IAC` doubled, `IAC EOR` behind.
pub fn frame(record: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(record.len() + 4);
    for &byte in record {
        out.push(byte);
        if byte == IAC {
            out.push(IAC);
        }
    }
    out.extend([IAC, EOR]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn telnet() -> Telnet {
        Telnet::new("IBM-3179-2", vec![("KBDTYPE".into(), "USB".into()), ("DEVNAME".into(), "QPADEV01".into())])
    }

    fn feed(telnet: &mut Telnet, data: &[u8]) -> (Vec<Vec<u8>>, Vec<u8>) {
        let (mut records, mut reply) = (Vec::new(), Vec::new());
        telnet.feed(data, &mut records, &mut reply);
        (records, reply)
    }

    #[test]
    fn answers_what_an_ibm_i_server_asks() {
        let mut t = telnet();
        let (_, reply) = feed(&mut t, &[IAC, DO, NEW_ENVIRON, IAC, DO, TERMINAL_TYPE]);
        assert_eq!(reply, [IAC, WILL, NEW_ENVIRON, IAC, WILL, TERMINAL_TYPE]);

        let mut ask = vec![IAC, SB, NEW_ENVIRON, SEND, USERVAR];
        ask.extend(b"IBMRSEED");
        ask.extend([1, 2, 3, 4, 5, 6, 7, 8, VAR, USERVAR, IAC, SE]);
        let (_, reply) = feed(&mut t, &ask);
        let mut expected = vec![IAC, SB, NEW_ENVIRON, IS, USERVAR];
        expected.extend(b"KBDTYPE");
        expected.push(VALUE);
        expected.extend(b"USB");
        expected.push(USERVAR);
        expected.extend(b"DEVNAME");
        expected.push(VALUE);
        expected.extend(b"QPADEV01");
        expected.extend([IAC, SE]);
        assert_eq!(reply, expected);

        let (_, reply) = feed(&mut t, &[IAC, SB, TERMINAL_TYPE, SEND, IAC, SE]);
        let mut expected = vec![IAC, SB, TERMINAL_TYPE, IS];
        expected.extend(b"IBM-3179-2");
        expected.extend([IAC, SE]);
        assert_eq!(reply, expected);

        let (_, reply) =
            feed(&mut t, &[IAC, DO, END_OF_RECORD, IAC, WILL, END_OF_RECORD, IAC, DO, BINARY, IAC, WILL, BINARY]);
        assert_eq!(reply, [IAC, WILL, END_OF_RECORD, IAC, DO, END_OF_RECORD, IAC, WILL, BINARY, IAC, DO, BINARY]);
        // Asked again: already agreed, no answer -- no loop.
        let (_, reply) = feed(&mut t, &[IAC, DO, BINARY, IAC, WILL, BINARY]);
        assert!(reply.is_empty());
    }

    #[test]
    fn refuses_other_options_once() {
        let mut t = telnet();
        let (_, reply) = feed(&mut t, &[IAC, DO, 1, IAC, WILL, 3]);
        assert_eq!(reply, [IAC, WONT, 1, IAC, DONT, 3]);
        let (_, reply) = feed(&mut t, &[IAC, DO, 1, IAC, WILL, 3]);
        assert!(reply.is_empty());
    }

    #[test]
    fn cuts_records_at_eor_however_the_bytes_arrive() {
        let wire = [frame(&[0, 10, 0x12, 0xA0, IAC, 4]), frame(&[1, 2, 3])].concat();
        for split in 0..=wire.len() {
            let mut t = telnet();
            let (mut records, _) = feed(&mut t, &wire[..split]);
            let (rest, _) = feed(&mut t, &wire[split..]);
            records.extend(rest);
            assert_eq!(records, vec![vec![0, 10, 0x12, 0xA0, IAC, 4], vec![1, 2, 3]], "split at {split}");
        }
    }

    #[test]
    fn drops_a_record_that_grows_too_long() {
        let mut t = telnet();
        let (records, _) = feed(&mut t, &vec![0x40; MAX_RECORD + 10]);
        assert!(records.is_empty());
        let (records, _) = feed(&mut t, &[IAC, EOR, 7, IAC, EOR]);
        assert_eq!(records, vec![vec![7]]);
    }
}
