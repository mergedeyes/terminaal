//! An SFTP client (protocol version 3, as OpenSSH's `sftp-server` speaks
//! it) over any byte stream -- in Terminaal a socket whose other end the
//! SSH worker ties to an `sftp` subsystem channel ([`crate::ssh::forward`]).
//! Blocking, but requests can be pipelined: [`Client::send`] returns the
//! request's id right away, [`Client::reply`] waits for the reply to one id
//! and keeps the others that arrive meanwhile. Transfers use that to keep
//! many reads or writes in flight at once; one at a time, every chunk would
//! cost a round trip.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, ErrorKind, Read, Write};

const VERSION: u32 = 3;
/// Replies beyond this are refused: a broken stream, not a real packet.
const MAX_PACKET: usize = 4 * 1024 * 1024;
/// Chunk size without the `limits@openssh.com` extension (OpenSSH's own).
pub const DEFAULT_CHUNK: u32 = 32 * 1024;
/// Chunk size ceiling even when the server allows more.
const MAX_CHUNK: u32 = 255 * 1024;

mod packet {
    pub const INIT: u8 = 1;
    pub const VERSION: u8 = 2;
    pub const OPEN: u8 = 3;
    pub const CLOSE: u8 = 4;
    pub const READ: u8 = 5;
    pub const WRITE: u8 = 6;
    pub const LSTAT: u8 = 7;
    pub const FSTAT: u8 = 8;
    pub const SETSTAT: u8 = 9;
    pub const OPENDIR: u8 = 11;
    pub const READDIR: u8 = 12;
    pub const REMOVE: u8 = 13;
    pub const MKDIR: u8 = 14;
    pub const RMDIR: u8 = 15;
    pub const REALPATH: u8 = 16;
    pub const STAT: u8 = 17;
    pub const RENAME: u8 = 18;
    pub const STATUS: u8 = 101;
    pub const HANDLE: u8 = 102;
    pub const DATA: u8 = 103;
    pub const NAME: u8 = 104;
    pub const ATTRS: u8 = 105;
    pub const EXTENDED: u8 = 200;
    pub const EXTENDED_REPLY: u8 = 201;
}

/// `SSH_FXF_*` open flags.
pub mod open {
    pub const READ: u32 = 0x01;
    pub const WRITE: u32 = 0x02;
    pub const CREATE: u32 = 0x08;
    pub const TRUNCATE: u32 = 0x10;
    pub const EXCLUSIVE: u32 = 0x20;
}

const ATTR_SIZE: u32 = 0x01;
const ATTR_UIDGID: u32 = 0x02;
const ATTR_PERMISSIONS: u32 = 0x04;
const ATTR_ACMODTIME: u32 = 0x08;
const ATTR_EXTENDED: u32 = 0x8000_0000;

/// `SSH_FX_*` status codes.
mod status {
    pub const OK: u32 = 0;
    pub const EOF: u32 = 1;
    pub const NO_SUCH_FILE: u32 = 2;
    pub const PERMISSION_DENIED: u32 = 3;
    pub const OP_UNSUPPORTED: u32 = 8;
}

/// File attributes; what the server didn't send is `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attrs {
    pub size: Option<u64>,
    pub uid_gid: Option<(u32, u32)>,
    /// Type and mode bits, as `st_mode`.
    pub permissions: Option<u32>,
    /// Seconds since the epoch.
    pub atime_mtime: Option<(u32, u32)>,
}

impl Attrs {
    pub fn is_dir(&self) -> bool {
        self.file_type() == Some(libc::S_IFDIR)
    }

    pub fn is_file(&self) -> bool {
        self.file_type() == Some(libc::S_IFREG)
    }

    fn file_type(&self) -> Option<u32> {
        self.permissions.map(|mode| mode & libc::S_IFMT)
    }

    pub fn mtime(&self) -> Option<u32> {
        self.atime_mtime.map(|(_, mtime)| mtime)
    }

    /// Only the mode bits (no file type), for creating or `SETSTAT`.
    pub fn mode(mode: u32) -> Self {
        Self { permissions: Some(mode & 0o7777), ..Self::default() }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        let mut flags = 0;
        if self.size.is_some() {
            flags |= ATTR_SIZE;
        }
        if self.uid_gid.is_some() {
            flags |= ATTR_UIDGID;
        }
        if self.permissions.is_some() {
            flags |= ATTR_PERMISSIONS;
        }
        if self.atime_mtime.is_some() {
            flags |= ATTR_ACMODTIME;
        }
        put_u32(out, flags);
        if let Some(size) = self.size {
            out.extend(size.to_be_bytes());
        }
        if let Some((uid, gid)) = self.uid_gid {
            put_u32(out, uid);
            put_u32(out, gid);
        }
        if let Some(permissions) = self.permissions {
            put_u32(out, permissions);
        }
        if let Some((atime, mtime)) = self.atime_mtime {
            put_u32(out, atime);
            put_u32(out, mtime);
        }
    }

    fn decode(reader: &mut Reader) -> io::Result<Self> {
        let flags = reader.u32()?;
        let mut attrs = Attrs::default();
        if flags & ATTR_SIZE != 0 {
            attrs.size = Some(reader.u64()?);
        }
        if flags & ATTR_UIDGID != 0 {
            attrs.uid_gid = Some((reader.u32()?, reader.u32()?));
        }
        if flags & ATTR_PERMISSIONS != 0 {
            attrs.permissions = Some(reader.u32()?);
        }
        if flags & ATTR_ACMODTIME != 0 {
            attrs.atime_mtime = Some((reader.u32()?, reader.u32()?));
        }
        if flags & ATTR_EXTENDED != 0 {
            for _ in 0..reader.u32()? {
                reader.bytes()?;
                reader.bytes()?;
            }
        }
        Ok(attrs)
    }
}

/// One entry of a directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub attrs: Attrs,
}

/// A failed request: the server's status, or the stream broke.
#[derive(Debug)]
pub enum Error {
    Status { code: u32, message: String },
    Io(io::Error),
}

impl Error {
    pub fn not_found(&self) -> bool {
        matches!(self, Error::Status { code: status::NO_SUCH_FILE, .. })
    }

    pub fn permission_denied(&self) -> bool {
        matches!(self, Error::Status { code: status::PERMISSION_DENIED, .. })
    }

    /// The connection is gone; nothing more will work.
    pub fn is_fatal(&self) -> bool {
        matches!(self, Error::Io(_))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Status { message, .. } if !message.is_empty() => f.write_str(message),
            Error::Status { code, .. } => write!(f, "SFTP status {code}"),
            Error::Io(err) => err.fmt(f),
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A server's answer to one request.
#[derive(Debug)]
pub enum Reply {
    Status { code: u32, message: String },
    Handle(Vec<u8>),
    Data(Vec<u8>),
    Name(Vec<Entry>),
    Attrs(Attrs),
    Extended(Vec<u8>),
}

impl Reply {
    /// `Ok` for a status OK, the status as an error otherwise; `other` for
    /// any other reply.
    fn into_error(self) -> Error {
        match self {
            Reply::Status { code, message } => Error::Status { code, message },
            other => Error::Io(io::Error::new(ErrorKind::InvalidData, format!("unexpected SFTP reply {other:?}"))),
        }
    }

    pub fn ok(self) -> Result<()> {
        match self {
            Reply::Status { code: status::OK, .. } => Ok(()),
            other => Err(other.into_error()),
        }
    }

    pub fn handle(self) -> Result<Vec<u8>> {
        match self {
            Reply::Handle(handle) => Ok(handle),
            other => Err(other.into_error()),
        }
    }

    pub fn attrs(self) -> Result<Attrs> {
        match self {
            Reply::Attrs(attrs) => Ok(attrs),
            other => Err(other.into_error()),
        }
    }

    /// Data, or `None` at the end of the file.
    pub fn data(self) -> Result<Option<Vec<u8>>> {
        match self {
            Reply::Data(data) => Ok(Some(data)),
            Reply::Status { code: status::EOF, .. } => Ok(None),
            other => Err(other.into_error()),
        }
    }

    /// Names, or `None` at the end of a listing.
    pub fn names(self) -> Result<Option<Vec<Entry>>> {
        match self {
            Reply::Name(names) => Ok(Some(names)),
            Reply::Status { code: status::EOF, .. } => Ok(None),
            other => Err(other.into_error()),
        }
    }
}

pub struct Client<R, W> {
    reader: R,
    writer: W,
    next_id: u32,
    /// Replies that arrived while waiting for another.
    stashed: HashMap<u32, Reply>,
    extensions: Vec<(String, Vec<u8>)>,
    /// How much one read or write may carry.
    pub chunk: u32,
}

impl<R: Read, W: Write> Client<R, W> {
    /// Say hello and learn what the server offers.
    pub fn new(reader: R, writer: W) -> Result<Self> {
        let mut client =
            Self { reader, writer, next_id: 1, stashed: HashMap::new(), extensions: Vec::new(), chunk: DEFAULT_CHUNK };
        let mut init = vec![packet::INIT];
        put_u32(&mut init, VERSION);
        client.write_packet(&init)?;
        let (kind, body) = client.read_packet()?;
        if kind != packet::VERSION {
            return Err(io::Error::new(ErrorKind::InvalidData, "no SFTP version from the server").into());
        }
        let mut reader = Reader(&body);
        let version = reader.u32()?;
        if version < VERSION {
            return Err(io::Error::new(ErrorKind::InvalidData, format!("SFTP version {version} not supported")).into());
        }
        while !reader.0.is_empty() {
            let name = reader.string()?;
            let data = reader.bytes()?.to_vec();
            client.extensions.push((name, data));
        }
        if client.has_extension("limits@openssh.com") {
            let limits = client.call(packet::EXTENDED, |out| put_str(out, "limits@openssh.com"));
            if let Ok(Reply::Extended(data)) = limits {
                let mut reader = Reader(&data);
                // max packet, max read, max write, max open handles
                let (_, read, write) = (reader.u64(), reader.u64(), reader.u64());
                if let (Ok(read), Ok(write)) = (read, write)
                    && read > 0
                    && write > 0
                {
                    client.chunk = read.min(write).min(u64::from(MAX_CHUNK)) as u32;
                }
            }
        }
        Ok(client)
    }

    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.iter().any(|(ext, _)| ext == name)
    }

    /// Send a request; its reply comes from [`Client::reply`] with the id.
    pub fn send(&mut self, kind: u8, body: impl FnOnce(&mut Vec<u8>)) -> Result<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let mut out = vec![kind];
        put_u32(&mut out, id);
        body(&mut out);
        self.write_packet(&out)?;
        Ok(id)
    }

    /// Wait for the reply to request `id`, keeping others for later.
    pub fn reply(&mut self, id: u32) -> Result<Reply> {
        if let Some(reply) = self.stashed.remove(&id) {
            return Ok(reply);
        }
        loop {
            let (got, reply) = self.read_reply()?;
            if got == id {
                return Ok(reply);
            }
            self.stashed.insert(got, reply);
        }
    }

    /// Wait for the reply to any of `ids`.
    pub fn reply_any(&mut self, ids: &[u32]) -> Result<(u32, Reply)> {
        if let Some(&id) = ids.iter().find(|id| self.stashed.contains_key(id)) {
            return Ok((id, self.stashed.remove(&id).expect("just checked")));
        }
        loop {
            let (got, reply) = self.read_reply()?;
            if ids.contains(&got) {
                return Ok((got, reply));
            }
            self.stashed.insert(got, reply);
        }
    }

    fn call(&mut self, kind: u8, body: impl FnOnce(&mut Vec<u8>)) -> Result<Reply> {
        let id = self.send(kind, body)?;
        self.reply(id)
    }

    /// The absolute, canonical form of `path` (`.` is the home directory).
    pub fn realpath(&mut self, path: &str) -> Result<String> {
        match self.call(packet::REALPATH, |out| put_str(out, path))? {
            Reply::Name(mut names) if !names.is_empty() => Ok(names.swap_remove(0).name),
            other => Err(other.into_error()),
        }
    }

    pub fn stat(&mut self, path: &str) -> Result<Attrs> {
        self.call(packet::STAT, |out| put_str(out, path))?.attrs()
    }

    pub fn lstat(&mut self, path: &str) -> Result<Attrs> {
        self.call(packet::LSTAT, |out| put_str(out, path))?.attrs()
    }

    pub fn fstat(&mut self, handle: &[u8]) -> Result<Attrs> {
        self.call(packet::FSTAT, |out| put_bytes(out, handle))?.attrs()
    }

    pub fn setstat(&mut self, path: &str, attrs: Attrs) -> Result<()> {
        self.call(packet::SETSTAT, |out| {
            put_str(out, path);
            attrs.encode(out);
        })?
        .ok()
    }

    /// Every entry of `path` but `.` and `..`, as the server lists them.
    pub fn list(&mut self, path: &str) -> Result<Vec<Entry>> {
        let handle = self.call(packet::OPENDIR, |out| put_str(out, path))?.handle()?;
        let mut entries = Vec::new();
        let result = loop {
            match self.call(packet::READDIR, |out| put_bytes(out, &handle)).and_then(Reply::names) {
                Ok(Some(names)) => entries.extend(names.into_iter().filter(|e| e.name != "." && e.name != "..")),
                Ok(None) => break Ok(()),
                Err(err) => break Err(err),
            }
        };
        let closed = self.close(&handle);
        result.and(closed).map(|()| entries)
    }

    pub fn open(&mut self, path: &str, flags: u32, attrs: Attrs) -> Result<Vec<u8>> {
        self.call(packet::OPEN, |out| {
            put_str(out, path);
            put_u32(out, flags);
            attrs.encode(out);
        })?
        .handle()
    }

    pub fn close(&mut self, handle: &[u8]) -> Result<()> {
        self.call(packet::CLOSE, |out| put_bytes(out, handle))?.ok()
    }

    pub fn send_read(&mut self, handle: &[u8], offset: u64, len: u32) -> Result<u32> {
        self.send(packet::READ, |out| {
            put_bytes(out, handle);
            out.extend(offset.to_be_bytes());
            put_u32(out, len);
        })
    }

    pub fn send_write(&mut self, handle: &[u8], offset: u64, data: &[u8]) -> Result<u32> {
        self.send(packet::WRITE, |out| {
            put_bytes(out, handle);
            out.extend(offset.to_be_bytes());
            put_bytes(out, data);
        })
    }

    pub fn remove(&mut self, path: &str) -> Result<()> {
        self.call(packet::REMOVE, |out| put_str(out, path))?.ok()
    }

    pub fn mkdir(&mut self, path: &str, attrs: Attrs) -> Result<()> {
        self.call(packet::MKDIR, |out| {
            put_str(out, path);
            attrs.encode(out);
        })?
        .ok()
    }

    pub fn rmdir(&mut self, path: &str) -> Result<()> {
        self.call(packet::RMDIR, |out| put_str(out, path))?.ok()
    }

    /// Rename, failing if `to` exists (plain SFTP v3).
    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        self.call(packet::RENAME, |out| {
            put_str(out, from);
            put_str(out, to);
        })?
        .ok()
    }

    /// Rename over an existing `to` in one step, as `rename(2)` does. Only
    /// with OpenSSH's `posix-rename@openssh.com`; `unsupported` otherwise.
    pub fn posix_rename(&mut self, from: &str, to: &str) -> Result<()> {
        if !self.has_extension("posix-rename@openssh.com") {
            return Err(Error::Status { code: status::OP_UNSUPPORTED, message: String::new() });
        }
        self.call(packet::EXTENDED, |out| {
            put_str(out, "posix-rename@openssh.com");
            put_str(out, from);
            put_str(out, to);
        })?
        .ok()
    }

    /// Read a whole (small) file.
    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>> {
        let handle = self.open(path, open::READ, Attrs::default())?;
        let mut data = Vec::new();
        let result = loop {
            let id = match self.send_read(&handle, data.len() as u64, self.chunk) {
                Ok(id) => id,
                Err(err) => break Err(err),
            };
            match self.reply(id).and_then(Reply::data) {
                Ok(Some(chunk)) => data.extend(chunk),
                Ok(None) => break Ok(()),
                Err(err) => break Err(err),
            }
        };
        let closed = self.close(&handle);
        result.and(closed).map(|()| data)
    }

    /// Write `data` to `path` from the start, creating it with `mode` if
    /// it's new and cutting it to size if not.
    pub fn write_file(&mut self, path: &str, data: &[u8], flags: u32, mode: u32) -> Result<()> {
        let handle = self.open(path, open::WRITE | open::CREATE | flags, Attrs::mode(mode))?;
        let result = self.write_all_at(&handle, data);
        let closed = self.close(&handle);
        result.and(closed)
    }

    /// Write `data` from offset 0 with many writes in flight at once.
    pub fn write_all_at(&mut self, handle: &[u8], data: &[u8]) -> Result<()> {
        let mut in_flight = Vec::new();
        let mut first_error = None;
        for (i, chunk) in data.chunks(self.chunk as usize).enumerate() {
            in_flight.push(self.send_write(handle, i as u64 * u64::from(self.chunk), chunk)?);
            if in_flight.len() >= PIPELINE {
                let (id, reply) = self.reply_any(&in_flight)?;
                in_flight.retain(|&other| other != id);
                if let Err(err) = reply.ok() {
                    first_error.get_or_insert(err);
                }
            }
        }
        while !in_flight.is_empty() {
            let (id, reply) = self.reply_any(&in_flight)?;
            in_flight.retain(|&other| other != id);
            if let Err(err) = reply.ok() {
                first_error.get_or_insert(err);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn read_reply(&mut self) -> Result<(u32, Reply)> {
        let (kind, body) = self.read_packet()?;
        let mut reader = Reader(&body);
        let id = reader.u32()?;
        let reply = match kind {
            packet::STATUS => {
                let code = reader.u32()?;
                // Both strings are optional in practice.
                let message = reader.string().unwrap_or_default();
                Reply::Status { code, message }
            }
            packet::HANDLE => Reply::Handle(reader.bytes()?.to_vec()),
            packet::DATA => Reply::Data(reader.bytes()?.to_vec()),
            packet::NAME => {
                let count = reader.u32()?;
                let mut names = Vec::with_capacity(count.min(1024) as usize);
                for _ in 0..count {
                    let name = reader.string()?;
                    let _long_name = reader.bytes()?;
                    names.push(Entry { name, attrs: Attrs::decode(&mut reader)? });
                }
                Reply::Name(names)
            }
            packet::ATTRS => Reply::Attrs(Attrs::decode(&mut reader)?),
            packet::EXTENDED_REPLY => Reply::Extended(reader.0.to_vec()),
            other => {
                return Err(io::Error::new(ErrorKind::InvalidData, format!("unknown SFTP packet {other}")).into());
            }
        };
        Ok((id, reply))
    }

    fn write_packet(&mut self, body: &[u8]) -> io::Result<()> {
        let mut packet = Vec::with_capacity(body.len() + 4);
        put_u32(&mut packet, body.len() as u32);
        packet.extend(body);
        self.writer.write_all(&packet)?;
        self.writer.flush()
    }

    fn read_packet(&mut self) -> io::Result<(u8, Vec<u8>)> {
        let mut len = [0u8; 4];
        self.reader.read_exact(&mut len)?;
        let len = u32::from_be_bytes(len) as usize;
        if len == 0 || len > MAX_PACKET {
            return Err(io::Error::new(ErrorKind::InvalidData, format!("bad SFTP packet length {len}")));
        }
        let mut body = vec![0u8; len];
        self.reader.read_exact(&mut body)?;
        let kind = body.remove(0);
        Ok((kind, body))
    }
}

/// Requests a transfer keeps in flight at once.
pub const PIPELINE: usize = 32;

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_be_bytes());
}

fn put_bytes(out: &mut Vec<u8>, data: &[u8]) {
    put_u32(out, data.len() as u32);
    out.extend(data);
}

fn put_str(out: &mut Vec<u8>, text: &str) {
    put_bytes(out, text.as_bytes());
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if self.0.len() < n {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "short SFTP packet"));
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("4 bytes")))
    }

    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("8 bytes")))
    }

    fn bytes(&mut self) -> io::Result<&'a [u8]> {
        let len = self.u32()? as usize;
        self.take(len)
    }

    /// Names on the wire are bytes; anything not UTF-8 comes out lossy.
    fn string(&mut self) -> io::Result<String> {
        Ok(String::from_utf8_lossy(self.bytes()?).into_owned())
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

    use super::*;

    /// OpenSSH's server, talking over pipes -- no sshd needed.
    pub struct Server {
        child: Child,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    pub fn sftp_server() -> Option<PathBuf> {
        ["/usr/lib/ssh/sftp-server", "/usr/lib/openssh/sftp-server", "/usr/libexec/sftp-server", "/usr/libexec/openssh/sftp-server"]
            .iter()
            .map(PathBuf::from)
            .find(|path| path.exists())
    }

    /// A client connected to a fresh `sftp-server`, or `None` (test
    /// skipped) where OpenSSH's server isn't installed.
    pub fn connect() -> Option<(Server, Client<ChildStdout, ChildStdin>)> {
        let Some(path) = sftp_server() else {
            eprintln!("sftp-server not installed, skipping");
            return None;
        };
        let mut child = Command::new(path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let (stdout, stdin) = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
        let client = Client::new(stdout, stdin).unwrap();
        Some((Server { child }, client))
    }

    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("terminaal-sftp-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn s(path: &Path) -> String {
        path.to_str().unwrap().to_string()
    }

    #[test]
    fn attrs_round_trip() {
        let attrs =
            Attrs { size: Some(1 << 40), uid_gid: Some((1000, 100)), permissions: Some(0o100644), atime_mtime: Some((1, 2)) };
        let mut out = Vec::new();
        attrs.encode(&mut out);
        assert_eq!(Attrs::decode(&mut Reader(&out)).unwrap(), attrs);
        let mut out = Vec::new();
        Attrs::default().encode(&mut out);
        assert_eq!(out, [0, 0, 0, 0]);
        assert!(attrs.is_file() && !attrs.is_dir());
        assert_eq!(Attrs::mode(0o100755).permissions, Some(0o755));
    }

    #[test]
    fn short_packets_are_errors_not_panics() {
        assert!(Reader(&[0, 0]).u32().is_err());
        assert!(Reader(&[0, 0, 0, 9, 1]).bytes().is_err());
    }

    #[test]
    fn lists_reads_writes_and_renames_against_openssh() {
        let Some((_server, mut client)) = connect() else { return };
        let dir = scratch("basic");
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();

        assert_eq!(client.realpath(&format!("{}/sub/..", s(&dir))).unwrap(), s(&dir.canonicalize().unwrap()));
        let mut names: Vec<(String, bool)> =
            client.list(&s(&dir)).unwrap().into_iter().map(|e| (e.name, e.attrs.is_dir())).collect();
        names.sort();
        assert_eq!(names, [("a.txt".to_string(), false), ("sub".to_string(), true)]);

        assert_eq!(client.read_file(&s(&dir.join("a.txt"))).unwrap(), b"hello");
        // Bigger than a chunk and than the pipeline, not a multiple of either.
        let big: Vec<u8> = (0..client.chunk as usize * (PIPELINE + 3) + 17).map(|i| (i * 7 % 251) as u8).collect();
        client.write_file(&s(&dir.join("big")), &big, open::TRUNCATE, 0o640).unwrap();
        assert_eq!(std::fs::read(dir.join("big")).unwrap(), big);
        assert_eq!(client.read_file(&s(&dir.join("big"))).unwrap(), big);
        let attrs = client.stat(&s(&dir.join("big"))).unwrap();
        assert_eq!((attrs.size, attrs.permissions.map(|p| p & 0o777)), (Some(big.len() as u64), Some(0o640)));

        client.rename(&s(&dir.join("big")), &s(&dir.join("b"))).unwrap();
        assert!(client.rename(&s(&dir.join("b")), &s(&dir.join("a.txt"))).is_err(), "v3 rename won't replace");
        client.posix_rename(&s(&dir.join("b")), &s(&dir.join("a.txt"))).unwrap();
        assert_eq!(std::fs::read(dir.join("a.txt")).unwrap().len(), big.len());

        let missing = client.stat(&s(&dir.join("nope"))).unwrap_err();
        assert!(missing.not_found() && !missing.is_fatal(), "{missing:?}");
        client.mkdir(&s(&dir.join("new")), Attrs::mode(0o700)).unwrap();
        client.rmdir(&s(&dir.join("new"))).unwrap();
        client.remove(&s(&dir.join("a.txt"))).unwrap();
        assert!(!dir.join("a.txt").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn permission_denied_is_told_apart() {
        let Some((_server, mut client)) = connect() else { return };
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = scratch("denied");
        let file = dir.join("locked");
        std::fs::write(&file, "x").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let err = client.read_file(&s(&file)).unwrap_err();
        assert!(err.permission_denied(), "{err:?}");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
