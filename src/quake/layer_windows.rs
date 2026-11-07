//! No layer surfaces on Windows: [`Layer::new`] never makes one, so
//! `app.rs` keeps to its winit window. Same interface as `layer.rs`.

use std::time::Instant;

use winit::window::CursorIcon;

pub use super::LayerEvent;

pub enum Layer {}

impl Layer {
    /// # Safety
    /// None needed; the signature is `layer.rs`'s.
    pub unsafe fn new(_display: *mut std::ffi::c_void) -> Option<Self> {
        None
    }

    pub fn is_shown(&self) -> bool {
        match *self {}
    }

    pub fn show(&mut self, _height: f32) -> Option<(*mut std::ffi::c_void, (u32, u32), f64)> {
        match *self {}
    }

    pub fn hide(&mut self) {
        match *self {}
    }

    pub fn set_height(&mut self, _height: f32) {
        match *self {}
    }

    pub fn surface_ptr(&self) -> Option<*mut std::ffi::c_void> {
        match *self {}
    }

    pub fn set_cursor(&mut self, _icon: CursorIcon) {
        match *self {}
    }

    pub fn next_repeat(&self) -> Option<Instant> {
        match *self {}
    }

    pub fn dispatch(&mut self) -> Vec<LayerEvent> {
        match *self {}
    }
}
