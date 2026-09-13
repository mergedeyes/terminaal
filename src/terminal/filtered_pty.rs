//! A local PTY whose output runs through the shell integration filter
//! (`terminal::integration`) before `alacritty_terminal`'s event loop
//! parses it.
//!
//! The event loop reads through `EventedReadWrite::reader` and knows
//! nothing else about the PTY, so [`FilteredPty`] hands it one end of a
//! socket pair instead of the PTY master. A thread of its own reads the
//! master, filters and writes into the other end, keeping the order of
//! the bytes -- prompt marks land exactly where the shell sent them.
//! Writing (input), resizing and child exit still go to the PTY itself.
//!
//! Filtering can make output longer, which is why it doesn't happen in
//! `read` itself: the event loop may stop reading with a full buffer and
//! only come back once the file is readable again, so anything held back
//! there could sit until the shell prints something else.

use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite, Pty};
use polling::{Event, PollMode, Poller};

use crate::terminal::integration::{Filter, ShellEvent};

pub struct FilteredPty {
    pty: Pty,
    /// The event loop's end: filtered output, non-blocking.
    output: UnixStream,
    /// Dropping it wakes the filter thread so it lets go of the PTY.
    _shutdown: UnixStream,
}

impl FilteredPty {
    /// Start filtering `pty`; what the shell says goes to `on_event`.
    pub fn new(pty: Pty, on_event: impl Fn(ShellEvent) + Send + 'static) -> io::Result<Self> {
        let (output, input) = UnixStream::pair()?;
        output.set_nonblocking(true)?;
        let (shutdown, shutdown_watch) = UnixStream::pair()?;
        // The master is non-blocking (the event loop needs that) and a
        // duplicate shares that, so the thread polls before reading.
        let master = pty.file().try_clone()?;
        std::thread::Builder::new()
            .name("PTY filter".into())
            .spawn(move || pump(master, input, shutdown_watch, on_event))?;
        Ok(Self { pty, output, _shutdown: shutdown })
    }
}

fn pump(mut master: File, mut input: UnixStream, shutdown: UnixStream, on_event: impl Fn(ShellEvent)) {
    let mut filter = Filter::default();
    let mut buf = vec![0; 0x10000];
    let (mut out, mut events) = (Vec::new(), Vec::new());
    loop {
        let mut fds = [
            libc::pollfd { fd: master.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: shutdown.as_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: `fds` is a valid array of two pollfds for the call.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if fds[1].revents != 0 {
            return;
        }
        let n = match master.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => continue,
            // EIO: the shell is gone. The event loop learns that from the
            // child exit; closing `input` here would only make it spin on
            // the end of the stream until then.
            Err(_) => {
                let _ = shutdown_wait(&shutdown);
                return;
            }
        };
        filter.feed(&buf[..n], &mut out, &mut events);
        if input.write_all(&out).is_err() {
            return;
        }
        out.clear();
        for event in events.drain(..) {
            on_event(event);
        }
    }
}

/// Block until the `FilteredPty` is dropped.
fn shutdown_wait(mut shutdown: &UnixStream) -> io::Result<usize> {
    shutdown.read(&mut [0])
}

fn write_only(mut interest: Event) -> Event {
    interest.readable = false;
    interest
}

impl EventedReadWrite for FilteredPty {
    type Reader = UnixStream;
    type Writer = File;

    unsafe fn register(&mut self, poll: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()> {
        // Readable comes from the socket, writable and the child's signals
        // from the PTY -- under the same key, which the event loop matches on.
        unsafe {
            self.pty.register(poll, write_only(interest), mode)?;
            poll.add_with_mode(&self.output, Event::readable(interest.key), mode)
        }
    }

    fn reregister(&mut self, poll: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()> {
        self.pty.reregister(poll, write_only(interest), mode)?;
        poll.modify_with_mode(&self.output, Event::readable(interest.key), mode)
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.pty.deregister(poll)?;
        poll.delete(&self.output)
    }

    fn reader(&mut self) -> &mut UnixStream {
        &mut self.output
    }

    fn writer(&mut self) -> &mut File {
        self.pty.writer()
    }
}

impl EventedPty for FilteredPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.pty.next_child_event()
    }
}

impl OnResize for FilteredPty {
    fn on_resize(&mut self, size: WindowSize) {
        self.pty.on_resize(size);
    }
}
