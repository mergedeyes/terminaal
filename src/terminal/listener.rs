//! Bridges `alacritty_terminal`'s event system to winit's.
//!
//! `alacritty_terminal` runs the PTY read loop on its own background
//! thread and calls back into an `EventListener` whenever something
//! happened that the UI needs to know about (new output, a title change,
//! the child process exiting, ...). We don't act on those events
//! ourselves here -- we just forward all of them to the winit event loop
//! via an `EventLoopProxy`, which wakes the main thread up so `app.rs` can
//! decide what to do (redraw, update the window title, answer a
//! PTY-write/color/size/clipboard request, close the tab, ...).
//!
//! `tab_id` is stamped onto every forwarded event because multiple tabs
//! each run their own PTY-reader thread against the same
//! `EventLoopProxy<UserEvent>` -- without it, `App::user_event` would
//! have no way to tell which tab's `Term`/PTY an event belongs to.

use alacritty_terminal::event::{Event, EventListener};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;

#[derive(Clone)]
pub struct EventProxyListener {
    proxy: EventLoopProxy<UserEvent>,
    tab_id: usize,
}

impl EventProxyListener {
    pub fn new(proxy: EventLoopProxy<UserEvent>, tab_id: usize) -> Self {
        Self { proxy, tab_id }
    }
}

impl EventListener for EventProxyListener {
    fn send_event(&self, event: Event) {
        // The event loop may already be gone (window closing); nothing
        // useful to do if the send fails.
        let _ = self.proxy.send_event(UserEvent::Terminal(self.tab_id, event));
    }
}
