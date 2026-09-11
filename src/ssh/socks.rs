//! The server side of a SOCKS handshake (versions 4, 4a and 5, CONNECT
//! only, no authentication) for dynamic forwards -- as a pure state
//! machine over bytes, so the same code serves a local socket
//! (`DynamicForward`) and a channel from the server (`RemoteForward`
//! with only a port). OpenSSH offers exactly this subset.

use std::net::{Ipv4Addr, Ipv6Addr};

/// A client that hasn't finished its request after this many bytes is
/// not speaking SOCKS.
const MAX_HANDSHAKE: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    V4,
    V5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Wait for more bytes.
    NeedMore,
    /// Send this to the client, then go on.
    Reply(Vec<u8>),
    /// The client wants a connection there. Answer with [`reply`] once it
    /// stands (or doesn't); bytes it sent after the request are in
    /// [`Handshake::into_rest`].
    Connect { host: String, port: u16, version: Version },
    /// Send this (may be empty), then close.
    Fail(Vec<u8>),
}

#[derive(Default)]
pub struct Handshake {
    buf: Vec<u8>,
    /// SOCKS5: the method selection is done, the request comes next.
    greeted: bool,
}

impl Handshake {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Process what has arrived. Call again after a [`Step::Reply`]; after
    /// `Connect` or `Fail`, the handshake is over.
    pub fn step(&mut self) -> Step {
        let step = match self.buf.first() {
            None => Step::NeedMore,
            Some(4) => self.socks4(),
            Some(5) if self.greeted => self.socks5_request(),
            Some(5) => self.socks5_greeting(),
            Some(_) => Step::Fail(Vec::new()),
        };
        if step == Step::NeedMore && self.buf.len() > MAX_HANDSHAKE {
            return Step::Fail(Vec::new());
        }
        step
    }

    /// What the client sent after its request: already meant for the target.
    pub fn into_rest(self) -> Vec<u8> {
        self.buf
    }

    /// `VN CD DSTPORT DSTIP USERID\0`, and for 4a (`DSTIP` 0.0.0.x) the
    /// host name after the user ID, also `\0`-terminated.
    fn socks4(&mut self) -> Step {
        let buf = &self.buf;
        if buf.len() < 9 {
            return Step::NeedMore;
        }
        let Some(user_end) = buf[8..].iter().position(|&b| b == 0).map(|i| 8 + i) else { return Step::NeedMore };
        let port = u16::from_be_bytes([buf[2], buf[3]]);
        let ip = Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
        let (host, end) = if buf[4..7] == [0, 0, 0] && buf[7] != 0 {
            let start = user_end + 1;
            let Some(len) = buf[start..].iter().position(|&b| b == 0) else { return Step::NeedMore };
            (String::from_utf8_lossy(&buf[start..start + len]).into_owned(), start + len + 1)
        } else {
            (ip.to_string(), user_end + 1)
        };
        if buf[1] != 1 || host.is_empty() {
            return Step::Fail(reply(Version::V4, false));
        }
        self.buf.drain(..end);
        Step::Connect { host, port, version: Version::V4 }
    }

    /// `VER NMETHODS METHODS…`; only "no authentication" (0) is offered.
    fn socks5_greeting(&mut self) -> Step {
        let Some(&count) = self.buf.get(1) else { return Step::NeedMore };
        let end = 2 + usize::from(count);
        if self.buf.len() < end {
            return Step::NeedMore;
        }
        let methods: Vec<u8> = self.buf.drain(..end).skip(2).collect();
        if !methods.contains(&0) {
            return Step::Fail(vec![5, 0xff]);
        }
        self.greeted = true;
        Step::Reply(vec![5, 0])
    }

    /// `VER CMD RSV ATYP DST.ADDR DST.PORT`.
    fn socks5_request(&mut self) -> Step {
        let buf = &self.buf;
        if buf.len() < 4 {
            return Step::NeedMore;
        }
        let (host, addr_end) = match buf[3] {
            1 => (buf.get(4..8).map(|a| Ipv4Addr::new(a[0], a[1], a[2], a[3]).to_string()), 8),
            3 => {
                let Some(&len) = buf.get(4) else { return Step::NeedMore };
                let len = usize::from(len);
                (buf.get(5..5 + len).map(|name| String::from_utf8_lossy(name).into_owned()), 5 + len)
            }
            4 => (buf.get(4..20).map(|a| Ipv6Addr::from(<[u8; 16]>::try_from(a).expect("16 bytes")).to_string()), 20),
            _ => return Step::Fail(socks5_reply(8)),
        };
        let (Some(host), Some(port)) = (host, buf.get(addr_end..addr_end + 2)) else { return Step::NeedMore };
        let port = u16::from_be_bytes([port[0], port[1]]);
        if buf[1] != 1 {
            return Step::Fail(socks5_reply(7));
        }
        if host.is_empty() {
            return Step::Fail(socks5_reply(1));
        }
        self.buf.drain(..addr_end + 2);
        Step::Connect { host, port, version: Version::V5 }
    }
}

/// The answer to a `Connect`: whether the connection stands. The bound
/// address in it is zeroed, as OpenSSH does -- clients don't use it.
pub fn reply(version: Version, ok: bool) -> Vec<u8> {
    match version {
        Version::V4 => vec![0, if ok { 90 } else { 91 }, 0, 0, 0, 0, 0, 0],
        // 5: connection refused.
        Version::V5 => socks5_reply(if ok { 0 } else { 5 }),
    }
}

fn socks5_reply(code: u8) -> Vec<u8> {
    vec![5, code, 0, 1, 0, 0, 0, 0, 0, 0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(bytes: &[u8]) -> (Vec<Step>, Handshake) {
        let mut handshake = Handshake::default();
        let mut steps = Vec::new();
        // Byte by byte: every prefix must just wait for more.
        for (i, &byte) in bytes.iter().enumerate() {
            handshake.push(&[byte]);
            loop {
                match handshake.step() {
                    Step::NeedMore => break,
                    step @ Step::Reply(_) => steps.push(step),
                    step => {
                        steps.push(step);
                        // What comes after the request, as if sent along.
                        handshake.push(&bytes[i + 1..]);
                        return (steps, handshake);
                    }
                }
            }
        }
        (steps, handshake)
    }

    fn connect(host: &str, port: u16, version: Version) -> Step {
        Step::Connect { host: host.into(), port, version }
    }

    #[test]
    fn socks4_and_4a() {
        let (steps, _) = run(&[4, 1, 0x1f, 0x90, 10, 0, 0, 7, b'u', 0]);
        assert_eq!(steps, [connect("10.0.0.7", 8080, Version::V4)]);

        let mut request = vec![4, 1, 0, 80, 0, 0, 0, 1, 0];
        request.extend(b"example.org\0GET");
        let (steps, handshake) = run(&request);
        assert_eq!(steps, [connect("example.org", 80, Version::V4)]);
        assert_eq!(handshake.into_rest(), b"GET");

        // BIND isn't supported.
        assert_eq!(run(&[4, 2, 0, 80, 1, 2, 3, 4, 0]).0, [Step::Fail(reply(Version::V4, false))]);
    }

    #[test]
    fn socks5_address_types() {
        let greeting = [5, 2, 2, 0];
        let with = |request: &[u8]| run(&[&greeting[..], request].concat()).0;
        let accepted = Step::Reply(vec![5, 0]);

        assert_eq!(with(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 22]), [accepted.clone(), connect("127.0.0.1", 22, Version::V5)]);
        let mut named = vec![5, 1, 0, 3, 6];
        named.extend(b"db.lan");
        named.extend(5432u16.to_be_bytes());
        assert_eq!(with(&named), [accepted.clone(), connect("db.lan", 5432, Version::V5)]);
        let mut v6 = vec![5, 1, 0, 4];
        v6.extend(Ipv6Addr::LOCALHOST.octets());
        v6.extend([0, 80]);
        assert_eq!(with(&v6), [accepted.clone(), connect("::1", 80, Version::V5)]);

        // UDP ASSOCIATE and unknown address types are refused with a reason.
        assert_eq!(with(&[5, 3, 0, 1, 0, 0, 0, 0, 0, 0]), [accepted.clone(), Step::Fail(socks5_reply(7))]);
        assert_eq!(with(&[5, 1, 0, 9]), [accepted, Step::Fail(socks5_reply(8))]);
    }

    #[test]
    fn socks5_needs_no_auth_and_keeps_pipelined_data() {
        // Only username/password offered.
        assert_eq!(run(&[5, 1, 2]).0, [Step::Fail(vec![5, 0xff])]);

        let (steps, handshake) = run(&[5, 1, 0, 5, 1, 0, 1, 1, 2, 3, 4, 0, 80, b'h', b'i']);
        assert_eq!(steps.last(), Some(&connect("1.2.3.4", 80, Version::V5)));
        assert_eq!(handshake.into_rest(), b"hi");
    }

    #[test]
    fn rejects_other_protocols_and_endless_requests() {
        assert_eq!(run(b"GET / HTTP/1.1\r\n").0, [Step::Fail(Vec::new())]);
        // A SOCKS4 user ID that never ends.
        let mut endless = vec![4, 1, 0, 80, 1, 2, 3, 4];
        endless.extend([b'a'; MAX_HANDSHAKE]);
        assert_eq!(run(&endless).0, [Step::Fail(Vec::new())]);
    }
}
