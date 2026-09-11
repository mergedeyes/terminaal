//! Agent forwarding (`ForwardAgent`). libssh2 accepts the server's
//! `auth-agent@openssh.com` channels by itself, but hands them over only
//! through a session callback the ssh2 crate doesn't expose: it's set here
//! through the C API and notes the channels on the worker's thread. Such a
//! channel can't become an `ssh2::Channel`, so [`AgentChannel`] makes the
//! few calls a tunnel needs itself. The pump ties each one to a new
//! connection to the local agent ([`super::forward::Forwards`]).

use std::cell::RefCell;
use std::io::{self, ErrorKind, Read, Write};
use std::os::raw::{c_int, c_ulong, c_void};
use std::path::{Path, PathBuf};
use std::ptr;

use libssh2_sys as raw;
use ssh2::{ErrorCode, Session};

/// `LIBSSH2_CALLBACK_AUTHAGENT`, missing from libssh2-sys.
const CALLBACK_AUTHAGENT: c_int = 7;

type AuthAgentFn = unsafe extern "C" fn(*mut raw::LIBSSH2_SESSION, *mut raw::LIBSSH2_CHANNEL, *mut *mut c_void);

unsafe extern "C" {
    /// Not bound by libssh2-sys. Takes and returns a `libssh2_cb_generic *`,
    /// that is `void (*)(void)`.
    fn libssh2_session_callback_set2(
        session: *mut raw::LIBSSH2_SESSION,
        cbtype: c_int,
        callback: Option<unsafe extern "C" fn()>,
    ) -> Option<unsafe extern "C" fn()>;
}

type Opened = (*mut raw::LIBSSH2_SESSION, *mut raw::LIBSSH2_CHANNEL);

thread_local! {
    /// Channels libssh2 opened on the server's request, with their session.
    /// The callback runs inside whichever libssh2 call read the request,
    /// on the thread that drives the session.
    static OPENED: RefCell<Vec<Opened>> = const { RefCell::new(Vec::new()) };
}

/// Called by libssh2 with the session locked: only note the channel.
unsafe extern "C" fn opened(
    session: *mut raw::LIBSSH2_SESSION,
    channel: *mut raw::LIBSSH2_CHANNEL,
    _abstract: *mut *mut c_void,
) {
    let _ = OPENED.try_with(|opened| {
        if let Ok(mut opened) = opened.try_borrow_mut() {
            opened.push((session, channel));
        }
    });
}

fn set_callback(session: &Session, callback: Option<AuthAgentFn>) {
    // SAFETY: only the pointer's type changes; libssh2 calls it with the
    // authagent signature.
    let generic = callback.map(|f| unsafe { std::mem::transmute::<AuthAgentFn, unsafe extern "C" fn()>(f) });
    let mut locked = session.raw();
    // SAFETY: the session is alive and locked.
    unsafe { libssh2_session_callback_set2(&mut *locked, CALLBACK_AUTHAGENT, generic) };
}

fn raw_session(session: &Session) -> *mut raw::LIBSSH2_SESSION {
    &mut *session.raw()
}

/// Agent forwarding on one session, from [`Self::enable`] until dropped.
pub struct AgentForwarding {
    session: Session,
    socket: PathBuf,
}

impl AgentForwarding {
    /// Accept the server's agent channels from now on, for the agent at
    /// `socket`. On the thread that drives `session`.
    pub fn enable(session: &Session, socket: PathBuf) -> Self {
        set_callback(session, Some(opened));
        Self { session: session.clone(), socket }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// The channels opened since the last call.
    pub fn take(&self) -> Vec<AgentChannel> {
        let session = raw_session(&self.session);
        let mut taken = Vec::new();
        OPENED.with_borrow_mut(|opened| {
            opened.retain(|&(owner, channel)| {
                let mine = owner == session;
                if mine {
                    taken.push(channel);
                }
                !mine
            });
        });
        taken.into_iter().map(|channel| AgentChannel::new(channel, self.session.clone())).collect()
    }
}

impl Drop for AgentForwarding {
    fn drop(&mut self) {
        // Refused from now on. Channels not taken yet go with the session;
        // forgotten here, so a later session at the same address can't
        // pick them up.
        set_callback(&self.session, None);
        let session = raw_session(&self.session);
        let _ = OPENED.try_with(|opened| opened.borrow_mut().retain(|&(owner, _)| owner != session));
    }
}

/// A channel libssh2 opened for the server's agent request, driven through
/// the C API under the session's lock, as ssh2 does with its own channels.
/// Keeps the session alive.
pub struct AgentChannel {
    raw: *mut raw::LIBSSH2_CHANNEL,
    session: Session,
}

// SAFETY (every `unsafe` call on `chan` below): the channel belongs to the
// session, which is alive and locked for the call.
impl AgentChannel {
    fn new(raw: *mut raw::LIBSSH2_CHANNEL, session: Session) -> Self {
        let channel = Self { raw, session };
        // Agent channels carry no stderr; merged, stray extended data can't
        // sit unread (see `forward::merge_stderr`).
        channel.call(|chan| unsafe {
            raw::libssh2_channel_handle_extended_data2(chan, raw::LIBSSH2_CHANNEL_EXTENDED_DATA_MERGE)
        });
        channel
    }

    /// Run `f` on the channel with the session locked.
    fn call<R>(&self, f: impl FnOnce(*mut raw::LIBSSH2_CHANNEL) -> R) -> R {
        let _locked = self.session.raw();
        f(self.raw)
    }

    pub fn eof(&self) -> bool {
        self.call(|chan| unsafe { raw::libssh2_channel_eof(chan) }) != 0
    }

    pub fn send_eof(&mut self) -> Result<(), ssh2::Error> {
        check(self.call(|chan| unsafe { raw::libssh2_channel_send_eof(chan) }))
    }

    pub fn close(&mut self) -> Result<(), ssh2::Error> {
        check(self.call(|chan| unsafe { raw::libssh2_channel_close(chan) }))
    }

    /// Data libssh2 already holds for this channel.
    pub fn available(&self) -> bool {
        let mut available: c_ulong = 0;
        self.call(|chan| unsafe { raw::libssh2_channel_window_read_ex(chan, &mut available, ptr::null_mut()) });
        available > 0
    }
}

impl Read for AgentChannel {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        transferred(self.call(|chan| unsafe { raw::libssh2_channel_read_ex(chan, 0, buf.as_mut_ptr().cast(), buf.len()) }))
    }
}

impl Write for AgentChannel {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        transferred(self.call(|chan| unsafe { raw::libssh2_channel_write_ex(chan, 0, buf.as_ptr().cast(), buf.len()) }))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for AgentChannel {
    fn drop(&mut self) {
        self.call(|chan| unsafe { raw::libssh2_channel_free(chan) });
    }
}

fn check(rc: c_int) -> Result<(), ssh2::Error> {
    if rc < 0 { Err(ssh2::Error::from_errno(ErrorCode::Session(rc))) } else { Ok(()) }
}

/// A read's or write's result: bytes, or libssh2's error code.
fn transferred(rc: isize) -> io::Result<usize> {
    match usize::try_from(rc) {
        Ok(n) => Ok(n),
        Err(_) if rc == raw::LIBSSH2_ERROR_EAGAIN as isize => Err(ErrorKind::WouldBlock.into()),
        Err(_) => Err(io::Error::other(ssh2::Error::from_errno(ErrorCode::Session(rc as c_int)))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_libssh2_results_to_io() {
        assert_eq!(transferred(5).unwrap(), 5);
        assert_eq!(transferred(raw::LIBSSH2_ERROR_EAGAIN as isize).unwrap_err().kind(), ErrorKind::WouldBlock);
        assert_eq!(transferred(-7).unwrap_err().kind(), ErrorKind::Other);
    }
}
