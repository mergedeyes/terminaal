//! SSH agents on Windows listen on named pipes, not Unix sockets: Windows'
//! OpenSSH agent on `\\.\pipe\openssh-ssh-agent` (so do 1Password,
//! Bitwarden and KeePassXC), Pageant on the pipe `pageant
//! --openssh-config` names. The pump loop polls sockets, which a pipe
//! isn't, so [`connect`] gives it one end of a local socket pair and a
//! thread relays between the other end and the pipe.
//!
//! The relay goes message by message ([`relay`]): a request from the
//! client, then the agent's answer. A synchronous pipe handle can't read
//! and write at the same time, and the agent protocol never needs to --
//! the client always waits for the answer.

use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;

/// OpenSSH's limit for one agent message.
#[cfg_attr(unix, allow(dead_code))]
const MAX_MESSAGE: usize = 256 * 1024;

/// Windows' OpenSSH agent: the agent to use when `SSH_AUTH_SOCK` isn't set.
#[cfg_attr(unix, allow(dead_code))]
pub const OPENSSH_AGENT: &str = r"\\.\pipe\openssh-ssh-agent";

/// Whether `path` names a named pipe (`\\.\pipe\…`, also with `/`).
#[cfg_attr(unix, allow(dead_code))]
pub fn is_pipe(path: &Path) -> bool {
    let path = path.to_string_lossy();
    let prefix = path.get(..9).unwrap_or_default();
    prefix.eq_ignore_ascii_case(r"\\.\pipe\") || prefix.eq_ignore_ascii_case("//./pipe/")
}

/// A connection to the agent on pipe `path`, as a local stream the pump
/// can poll. Opening the pipe happens on the relay's thread; if that
/// fails, the stream just ends, like a refused connection.
#[cfg(windows)]
pub fn connect(path: &Path) -> io::Result<crate::platform::LocalStream> {
    let (ours, mut theirs) = crate::platform::pair()?;
    let path = path.to_path_buf();
    std::thread::Builder::new().name("agent pipe".into()).spawn(move || {
        let result = open(&path).and_then(|mut pipe| relay(&mut theirs, &mut pipe));
        if let Err(err) = result {
            log::debug!("agent {}: {err}", path.display());
        }
    })?;
    Ok(ours)
}

/// Open the pipe; when all its instances are busy, wait for one a few
/// times (the agent makes a new one for each client).
#[cfg(windows)]
fn open(path: &Path) -> io::Result<std::fs::File> {
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
    for _ in 0..5 {
        match std::fs::OpenOptions::new().read(true).write(true).open(path) {
            Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                wait(path, 1000);
            }
            result => return result,
        }
    }
    Err(io::Error::from_raw_os_error(ERROR_PIPE_BUSY as i32))
}

/// Wait up to `ms` for an instance of pipe `path`; false if there's no
/// such pipe at all.
#[cfg(windows)]
fn wait(path: &Path, ms: u32) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, GetLastError};
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
    let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a NUL-terminated wide string that outlives the call.
    unsafe { WaitNamedPipeW(name.as_ptr(), ms) != 0 || GetLastError() != ERROR_FILE_NOT_FOUND }
}

/// Whether there's an agent at `path`: a pipe that exists (asked without
/// connecting -- opening it would take an instance), else a file.
#[cfg(windows)]
pub fn exists(path: &Path) -> bool {
    if is_pipe(path) { wait(path, 1) } else { path.exists() }
}

#[cfg(unix)]
pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// Pass the client's requests to the agent and its answers back, one
/// message at a time, until the client hangs up (`Ok`) or either side
/// fails or sends something too big.
#[cfg_attr(unix, allow(dead_code))]
pub fn relay(client: &mut (impl Read + Write), agent: &mut (impl Read + Write)) -> io::Result<()> {
    let mut message = Vec::new();
    loop {
        if !read_message(client, &mut message)? {
            return Ok(());
        }
        agent.write_all(&message)?;
        agent.flush()?;
        if !read_message(agent, &mut message)? {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "the agent hung up"));
        }
        client.write_all(&message)?;
        client.flush()?;
    }
}

/// One message (length and body) into `message`; false at a clean end
/// before it.
#[cfg_attr(unix, allow(dead_code))]
fn read_message(from: &mut impl Read, message: &mut Vec<u8>) -> io::Result<bool> {
    let mut length = [0; 4];
    match from.read(&mut length[..1])? {
        0 => return Ok(false),
        _ => from.read_exact(&mut length[1..])?,
    }
    let size = u32::from_be_bytes(length) as usize;
    if size > MAX_MESSAGE {
        return Err(io::Error::new(ErrorKind::InvalidData, format!("agent message of {size} bytes")));
    }
    message.clear();
    message.extend_from_slice(&length);
    message.resize(4 + size, 0);
    from.read_exact(&mut message[4..])?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// One side of a conversation: reads from what it's given, records
    /// what's written to it.
    struct Peer {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Peer {
        fn new(input: &[u8]) -> Self {
            Self { input: Cursor::new(input.to_vec()), output: Vec::new() }
        }
    }

    impl Read for Peer {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for Peer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn message(body: &[u8]) -> Vec<u8> {
        let mut out = (body.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn requests_and_answers_pass_one_by_one() {
        // Two requests (list identities = 11, then a sign request), two answers.
        let requests = [message(&[11]), message(&[13, 1, 2, 3])].concat();
        let answers = [message(&[12, 0, 0, 0, 0]), message(&[14, 9, 9])].concat();
        let (mut client, mut agent) = (Peer::new(&requests), Peer::new(&answers));
        relay(&mut client, &mut agent).unwrap();
        assert_eq!(agent.output, requests);
        assert_eq!(client.output, answers);
    }

    #[test]
    fn a_silent_agent_or_an_oversized_message_ends_it() {
        let (mut client, mut agent) = (Peer::new(&message(&[11])), Peer::new(&[]));
        assert_eq!(relay(&mut client, &mut agent).unwrap_err().kind(), ErrorKind::UnexpectedEof);

        let huge = ((MAX_MESSAGE + 1) as u32).to_be_bytes();
        let (mut client, mut agent) = (Peer::new(&huge), Peer::new(&[]));
        assert_eq!(relay(&mut client, &mut agent).unwrap_err().kind(), ErrorKind::InvalidData);
        assert!(agent.output.is_empty(), "nothing reaches the agent");

        // Cut off within a message.
        let (mut client, mut agent) = (Peer::new(&message(&[13, 1, 2])[..5]), Peer::new(&[]));
        assert!(relay(&mut client, &mut agent).is_err());
    }

    #[test]
    fn pipes_are_told_from_socket_paths() {
        assert!(is_pipe(Path::new(OPENSSH_AGENT)));
        assert!(is_pipe(Path::new("//./pipe/pageant.jan.abc")));
        assert!(is_pipe(Path::new(r"\\.\PIPE\x")));
        assert!(!is_pipe(Path::new("/run/user/1000/ssh-agent.sock")));
        assert!(!is_pipe(Path::new(r"C:\Users\jan\agent.sock")));
        assert!(!is_pipe(Path::new("")));
    }

    /// Through a real named pipe: a fake agent answers one request, the
    /// pump's end is a socket.
    #[cfg(windows)]
    #[test]
    fn an_agent_on_a_named_pipe_answers_through_the_socket() {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
        use windows_sys::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_TYPE_BYTE, PIPE_WAIT};

        let name = format!(r"\\.\pipe\terminaal-test-agent-{}", std::process::id());
        let path = Path::new(&name);
        assert!(!exists(path), "no pipe yet");
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: a NUL-terminated name; no security attributes.
        let handle = unsafe {
            CreateNamedPipeW(wide.as_ptr(), PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE | PIPE_WAIT, 1, 4096, 4096, 0, std::ptr::null())
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE);
        assert!(exists(path));
        // SAFETY: the pipe just made, owned by the file from here on.
        let mut pipe = unsafe { std::fs::File::from_raw_handle(handle) };
        let agent = std::thread::spawn(move || {
            // SAFETY: the file's own handle, open for the call.
            unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) };
            let mut request = Vec::new();
            assert!(read_message(&mut pipe, &mut request).unwrap());
            assert_eq!(request, message(&[11]));
            pipe.write_all(&message(&[12, 0, 0, 0, 0])).unwrap();
        });

        let mut socket = connect(path).unwrap();
        socket.write_all(&message(&[11])).unwrap();
        let mut answer = Vec::new();
        assert!(read_message(&mut socket, &mut answer).unwrap());
        assert_eq!(answer, message(&[12, 0, 0, 0, 0]));
        agent.join().unwrap();
    }
}
