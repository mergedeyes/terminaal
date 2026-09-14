//! What the app draws into: a winit window, or the drop-down window's
//! layer surface (`quake::layer`), which winit knows nothing about. The
//! methods are the handful of winit's `Window` that `app.rs` uses; for the
//! layer surface most of them have nothing to do.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use winit::dpi::PhysicalSize;
use winit::window::{CursorIcon, UserAttentionType, Window};

use crate::gpu::Target;
use crate::quake::layer::Layer;

pub enum AppWindow {
    Winit(Arc<Window>),
    Layer(Box<LayerWindow>),
}

/// The layer surface, and what winit would otherwise keep track of.
pub struct LayerWindow {
    pub layer: RefCell<Layer>,
    pub display: NonNull<c_void>,
    /// Physical size and scale as last configured; kept while hidden.
    pub size: Cell<PhysicalSize<u32>>,
    pub scale: Cell<f64>,
    /// A redraw was asked for; `app.rs` draws before the loop sleeps.
    pub redraw: Cell<bool>,
}

impl AppWindow {
    pub fn request_redraw(&self) {
        match self {
            Self::Winit(window) => window.request_redraw(),
            Self::Layer(layer) => layer.redraw.set(true),
        }
    }

    pub fn scale_factor(&self) -> f64 {
        match self {
            Self::Winit(window) => window.scale_factor(),
            Self::Layer(layer) => layer.scale.get(),
        }
    }

    pub fn inner_size(&self) -> PhysicalSize<u32> {
        match self {
            Self::Winit(window) => window.inner_size(),
            Self::Layer(layer) => layer.size.get(),
        }
    }

    pub fn set_title(&self, title: &str) {
        if let Self::Winit(window) = self {
            window.set_title(title);
        }
    }

    pub fn set_cursor(&self, icon: CursorIcon) {
        match self {
            Self::Winit(window) => window.set_cursor(icon),
            Self::Layer(layer) => layer.layer.borrow_mut().set_cursor(icon),
        }
    }

    pub fn request_user_attention(&self, kind: Option<UserAttentionType>) {
        if let Self::Winit(window) = self {
            window.request_user_attention(kind);
        }
    }

    pub fn pre_present_notify(&self) {
        if let Self::Winit(window) = self {
            window.pre_present_notify();
        }
    }

    /// Layer surfaces have no opaque region to begin with.
    pub fn set_transparent(&self, on: bool) {
        if let Self::Winit(window) = self {
            window.set_transparent(on);
        }
    }

    /// KWin's blur, which only winit windows get.
    pub fn set_blur(&self, on: bool) {
        if let Self::Winit(window) = self {
            window.set_blur(on);
        }
    }

    /// What the GPU draws on; `None` while the drop-down window is hidden.
    pub fn gpu_target(&self) -> Option<Target> {
        match self {
            Self::Winit(window) => Some(Target::Window(window.clone())),
            Self::Layer(layer) => {
                let surface = layer.layer.borrow().surface_ptr().and_then(NonNull::new)?;
                Some(Target::Wayland { display: layer.display, surface })
            }
        }
    }

    pub fn winit(&self) -> Option<&Arc<Window>> {
        match self {
            Self::Winit(window) => Some(window),
            Self::Layer(_) => None,
        }
    }
}
