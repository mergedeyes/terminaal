//! No `ext_background_effect_v1` on Windows: there's never a [`Blur`].
//! winit's own `set_blur`, which `app.rs` calls as well, is what's left.
//! Same interface as `blur.rs`.

use winit::window::Window;

pub enum Blur {}

impl Blur {
    pub fn new(_window: &Window) -> Option<Self> {
        None
    }

    /// # Safety
    /// None needed; the signature is `blur.rs`'s.
    pub unsafe fn for_surface(_display: *mut std::ffi::c_void, _surface: *mut std::ffi::c_void) -> Option<Self> {
        None
    }

    pub fn set(&mut self, _on: bool, _size: (i32, i32)) {
        match *self {}
    }
}
