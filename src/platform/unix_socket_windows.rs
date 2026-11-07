//! Unix sockets on Windows: Windows 10 1803 and later have `AF_UNIX`
//! stream sockets, std just doesn't wrap them. The few calls port
//! forwarding needs, with std's names ([`UnixListener`], [`UnixStream`]);
//! they're WinSock sockets, so [`super::wait`] polls them like the rest.

use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::windows::io::{AsRawSocket, FromRawSocket, OwnedSocket, RawSocket};
use std::path::Path;
use std::sync::Once;

use windows_sys::Win32::Networking::WinSock::{
    self, AF_UNIX, FIONBIO, INVALID_SOCKET, SD_BOTH, SD_RECEIVE, SD_SEND, SOCK_STREAM, SOCKADDR, SOCKADDR_UN,
    SOCKET, SOCKET_ERROR, WSADATA,
};

/// A socket listening on a path.
pub struct UnixListener(OwnedSocket);

/// A connected Unix socket.
pub struct UnixStream(OwnedSocket);

impl UnixListener {
    /// Listen on `path`; fails if a file is there already.
    pub fn bind(path: &Path) -> io::Result<Self> {
        let socket = new_socket()?;
        let (addr, len) = sockaddr(path)?;
        // SAFETY: a valid socket and address of `len` bytes.
        check(unsafe { WinSock::bind(raw(&socket), (&raw const addr).cast::<SOCKADDR>(), len) })?;
        check(unsafe { WinSock::listen(raw(&socket), 128) })?;
        Ok(Self(socket))
    }

    /// The next connection. No address: a client's is unnamed anyway.
    pub fn accept(&self) -> io::Result<(UnixStream, ())> {
        // SAFETY: a valid socket; the peer's address isn't wanted.
        let socket = unsafe { WinSock::accept(raw(&self.0), std::ptr::null_mut(), std::ptr::null_mut()) };
        Ok((UnixStream(owned(socket)?), ()))
    }

    pub fn set_nonblocking(&self, on: bool) -> io::Result<()> {
        set_nonblocking(&self.0, on)
    }
}

impl UnixStream {
    /// Connect to the socket at `path`.
    pub fn connect(path: &Path) -> io::Result<Self> {
        let socket = new_socket()?;
        let (addr, len) = sockaddr(path)?;
        // SAFETY: a valid socket and address of `len` bytes.
        check(unsafe { WinSock::connect(raw(&socket), (&raw const addr).cast::<SOCKADDR>(), len) })?;
        Ok(Self(socket))
    }

    pub fn set_nonblocking(&self, on: bool) -> io::Result<()> {
        set_nonblocking(&self.0, on)
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => SD_RECEIVE,
            Shutdown::Write => SD_SEND,
            Shutdown::Both => SD_BOTH,
        };
        // SAFETY: a valid socket.
        check(unsafe { WinSock::shutdown(raw(&self.0), how) })
    }
}

impl Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(i32::MAX as usize) as i32;
        // SAFETY: a valid socket; `buf` holds `len` bytes.
        let read = unsafe { WinSock::recv(raw(&self.0), buf.as_mut_ptr(), len, 0) };
        check(read).map(|()| read as usize)
    }
}

impl Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(i32::MAX as usize) as i32;
        // SAFETY: a valid socket; `buf` holds `len` bytes.
        let written = unsafe { WinSock::send(raw(&self.0), buf.as_ptr(), len, 0) };
        check(written).map(|()| written as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsRawSocket for UnixListener {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

impl AsRawSocket for UnixStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}

/// A new `AF_UNIX` stream socket. WinSock has to be started first -- std
/// does that only for its own sockets, maybe not yet.
fn new_socket() -> io::Result<OwnedSocket> {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        // SAFETY: a plain struct to fill; never cleaned up, like std's.
        let mut data: WSADATA = unsafe { std::mem::zeroed() };
        unsafe { WinSock::WSAStartup(0x202, &mut data) };
    });
    // SAFETY: no pointers.
    owned(unsafe { WinSock::socket(i32::from(AF_UNIX), SOCK_STREAM, 0) })
}

fn owned(socket: SOCKET) -> io::Result<OwnedSocket> {
    if socket == INVALID_SOCKET {
        return Err(last_error());
    }
    // SAFETY: a socket just made, owned by nobody else.
    Ok(unsafe { OwnedSocket::from_raw_socket(socket as RawSocket) })
}

fn raw(socket: &OwnedSocket) -> SOCKET {
    socket.as_raw_socket() as SOCKET
}

/// The address of `path`: UTF-8 bytes, NUL-terminated, at most 107.
fn sockaddr(path: &Path) -> io::Result<(SOCKADDR_UN, i32)> {
    let bytes = path.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "socket path isn't UTF-8"))?.as_bytes();
    let mut addr = SOCKADDR_UN { sun_family: AF_UNIX, sun_path: [0; 108] };
    if bytes.is_empty() || bytes.len() >= addr.sun_path.len() || bytes.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("unusable socket path {}", path.display())));
    }
    for (to, from) in addr.sun_path.iter_mut().zip(bytes) {
        *to = *from as i8;
    }
    let len = std::mem::offset_of!(SOCKADDR_UN, sun_path) + bytes.len() + 1;
    Ok((addr, len as i32))
}

fn set_nonblocking(socket: &OwnedSocket, on: bool) -> io::Result<()> {
    let mut on = u32::from(on);
    // SAFETY: a valid socket and a u32 argument, as FIONBIO takes.
    check(unsafe { WinSock::ioctlsocket(raw(socket), FIONBIO, &mut on) })
}

fn check(result: i32) -> io::Result<()> {
    if result == SOCKET_ERROR { Err(last_error()) } else { Ok(()) }
}

/// WinSock's error codes are in std's table (`WSAEWOULDBLOCK` is
/// `WouldBlock`, and so on).
fn last_error() -> io::Error {
    // SAFETY: no arguments.
    io::Error::from_raw_os_error(unsafe { WinSock::WSAGetLastError() })
}

/// For tests: whether this system has `AF_UNIX` at all -- Wine doesn't
/// (`WSAEAFNOSUPPORT`); Windows has since 10 1803, CI's runners too.
#[cfg(test)]
pub fn supported() -> bool {
    match new_socket() {
        Err(err) if err.raw_os_error() == Some(WinSock::WSAEAFNOSUPPORT) => {
            eprintln!("no AF_UNIX here (Wine?): skipped");
            false
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_socket_carries_bytes_both_ways() {
        if !supported() {
            return;
        }
        let path = std::env::temp_dir().join(format!("terminaal-uds-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        assert!(super::super::is_socket_file(&path));
        let mut client = UnixStream::connect(&path).unwrap();
        let (mut server, ()) = listener.accept().unwrap();
        client.write_all(b"ping").unwrap();
        let mut buf = [0; 4];
        server.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ping");
        server.write_all(b"pong").unwrap();
        server.shutdown(Shutdown::Write).unwrap();
        let mut answer = Vec::new();
        client.read_to_end(&mut answer).unwrap();
        assert_eq!(answer, b"pong");

        // Non-blocking: nothing there yet is `WouldBlock`.
        listener.set_nonblocking(true).unwrap();
        assert_eq!(listener.accept().err().map(|err| err.kind()), Some(io::ErrorKind::WouldBlock));
        drop(listener);
        std::fs::remove_file(&path).unwrap();
        assert!(UnixStream::connect(&path).is_err(), "nobody there any more");
    }
}
