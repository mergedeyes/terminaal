//! A ConPTY whose output runs through the shell integration filter
//! (`terminal::integration`) before `alacritty_terminal`'s event loop
//! parses it -- the Windows counterpart of `filtered_pty.rs`.
//!
//! alacritty's Windows PTY reads ConPTY's pipe on a thread of its own
//! already and hands the bytes over without blocking, so the filter runs
//! right in `read`. What it makes longer than the event loop's buffer
//! waits in `pending`; the loop may stop reading before that's empty and
//! would only come back for new output, so a completion packet tells its
//! poller there's more -- as alacritty's own reader does for its pipe.

use std::io::{self, Read};
use std::sync::Arc;

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite, PTY_READ_WRITE_TOKEN, Pty};
use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, PollMode, Poller};

use crate::terminal::graphics::SharedGraphics;
use crate::terminal::integration::{Filter, ShellEvent};

pub struct FilteredPty {
    output: Output,
}

/// The event loop's reader; owns the PTY so that it can read from it.
pub struct Output {
    pty: Pty,
    filter: Filter,
    on_event: Box<dyn Fn(ShellEvent) + Send>,
    /// Raw bytes from the PTY.
    raw: Vec<u8>,
    /// Filtered bytes not handed out yet.
    pending: Vec<u8>,
    events: Vec<ShellEvent>,
    /// The event loop's poller, while registered.
    poller: Option<Arc<Poller>>,
}

impl FilteredPty {
    /// Start filtering `pty`; what the shell says goes to `on_event`.
    pub fn new(pty: Pty, graphics: SharedGraphics, on_event: impl Fn(ShellEvent) + Send + 'static) -> io::Result<Self> {
        Ok(Self {
            output: Output {
                pty,
                filter: Filter::with_graphics(graphics),
                on_event: Box::new(on_event),
                raw: vec![0; 0x10000],
                pending: Vec::new(),
                events: Vec::new(),
                poller: None,
            },
        })
    }
}

impl Read for Output {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // A sequence the filter holds back yields nothing yet: read on.
        while self.pending.is_empty() {
            let n = self.pty.reader().read(&mut self.raw)?;
            if n == 0 {
                return Ok(0);
            }
            self.filter.feed(&self.raw[..n], &mut self.pending, &mut self.events);
            for event in self.events.drain(..) {
                (self.on_event)(event);
            }
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        if !self.pending.is_empty()
            && let Some(poller) = &self.poller
        {
            let _ = poller.post(CompletionPacket::new(Event::readable(PTY_READ_WRITE_TOKEN)));
        }
        Ok(n)
    }
}

impl EventedReadWrite for FilteredPty {
    type Reader = Output;
    type Writer = <Pty as EventedReadWrite>::Writer;

    unsafe fn register(&mut self, poll: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()> {
        self.output.poller = Some(poll.clone());
        unsafe { self.output.pty.register(poll, interest, mode) }
    }

    fn reregister(&mut self, poll: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()> {
        self.output.poller = Some(poll.clone());
        self.output.pty.reregister(poll, interest, mode)
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.output.poller = None;
        self.output.pty.deregister(poll)
    }

    fn reader(&mut self) -> &mut Output {
        &mut self.output
    }

    fn writer(&mut self) -> &mut Self::Writer {
        self.output.pty.writer()
    }
}

impl EventedPty for FilteredPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.output.pty.next_child_event()
    }
}

impl OnResize for FilteredPty {
    fn on_resize(&mut self, size: WindowSize) {
        self.output.pty.on_resize(size);
    }
}
