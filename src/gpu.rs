//! Core wgpu setup: instance, adapter, device, queue and the window
//! surface. Just the device-level plumbing -- the actual draw calls live
//! in `render/`.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use winit::window::Window;

/// What the frames go on.
pub enum Target {
    Window(Arc<Window>),
    /// A `wl_surface` of our own (the drop-down window's layer surface) on
    /// winit's display. Both must outlive the GPU surface.
    Wayland { display: NonNull<c_void>, surface: NonNull<c_void> },
}

pub struct GpuState {
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    adapter: wgpu::Adapter,
    /// `None` while the drop-down window is hidden.
    pub surface: Option<wgpu::Surface<'static>>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub format: wgpu::TextureFormat,
    /// How the surface can be composited with what's behind the window.
    alpha_modes: Vec<wgpu::CompositeAlphaMode>,
    /// The frames are see-through (`set_translucent`).
    see_through: bool,
}

/// Windows: whether frames go through DirectComposition (a swapchain on a
/// composition visual) rather than straight onto the window. Only that way
/// can they be see-through, so it's the default; the window is then made
/// without a redirection bitmap (`app.rs`), which would cover the visual.
/// `WGPU_DX12_PRESENTATION_SYSTEM=hwnd` goes back to the plain swapchain,
/// should a driver show nothing with the other.
#[cfg(windows)]
pub fn composition() -> bool {
    wgpu::Dx12SwapchainKind::from_env().unwrap_or(wgpu::Dx12SwapchainKind::DxgiFromVisual)
        == wgpu::Dx12SwapchainKind::DxgiFromVisual
}

/// DXGI keeps the alpha mode a swapchain was made with -- wgpu's
/// `configure` only resizes an existing one. So on Windows the surface is
/// premultiplied from the start wherever it can be, and an opaque window
/// simply draws everything with alpha 1 (the clear color does).
const ALPHA_FIXED: bool = cfg!(windows);

impl GpuState {
    /// `size` in physical pixels.
    pub async fn new(target: Target, size: (u32, u32), event_loop: &ActiveEventLoop) -> Self {
        #[allow(unused_mut)]
        let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(event_loop.owned_display_handle()));
        #[cfg(windows)]
        if composition() {
            descriptor.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        }
        let instance = wgpu::Instance::new(descriptor);

        let surface = create_surface(&instance, target);

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .expect("no compatible GPU adapter found");
        let info = adapter.get_info();
        log::info!("GPU adapter: {} ({:?}, {:?}, driver {})", info.name, info.device_type, info.backend, info.driver);
        let alpha_modes = surface.get_capabilities(&adapter).alpha_modes;
        log::info!("surface alpha modes: {alpha_modes:?}");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("failed to create wgpu device");

        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: alpha_mode(&alpha_modes, false),
            view_formats: vec![],
            // A terminal is all about input latency; one frame in flight
            // is plenty for what little it draws.
            desired_maximum_frame_latency: 1,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        Self { instance, adapter, device, queue, surface: Some(surface), surface_config, format, alpha_modes, see_through: false }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface_config.width = width.max(1);
        self.surface_config.height = height.max(1);
        self.configure();
    }

    /// Apply `surface_config` to the surface, if there is one.
    pub fn configure(&self) {
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.surface_config);
        }
    }

    /// Draw on `target` from now on, at `size` physical pixels.
    pub fn set_target(&mut self, target: Target, size: (u32, u32)) {
        self.surface = None;
        let surface = create_surface(&self.instance, target);
        self.alpha_modes = surface.get_capabilities(&self.adapter).alpha_modes;
        self.see_through &= self.supports_translucency();
        self.surface_config.alpha_mode = alpha_mode(&self.alpha_modes, self.see_through);
        self.surface = Some(surface);
        self.resize(size.0, size.1);
    }

    /// The window can be see-through: the surface takes premultiplied
    /// alpha, which is what the quad, text and egui passes produce.
    pub fn supports_translucency(&self) -> bool {
        self.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied)
    }

    /// Whether the surface is see-through right now.
    pub fn translucent(&self) -> bool {
        self.see_through
    }

    /// Make the surface see-through (where supported) or opaque.
    pub fn set_translucent(&mut self, on: bool) {
        self.see_through = on && self.supports_translucency();
        let mode = alpha_mode(&self.alpha_modes, self.see_through);
        if mode != self.surface_config.alpha_mode {
            self.surface_config.alpha_mode = mode;
            self.configure();
        }
    }
}

/// How to configure a surface offering `modes`, see-through or not.
fn alpha_mode(modes: &[wgpu::CompositeAlphaMode], see_through: bool) -> wgpu::CompositeAlphaMode {
    let premultiplied = modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied);
    if premultiplied && (see_through || ALPHA_FIXED) {
        wgpu::CompositeAlphaMode::PreMultiplied
    } else {
        wgpu::CompositeAlphaMode::Opaque
    }
}

fn create_surface(instance: &wgpu::Instance, target: Target) -> wgpu::Surface<'static> {
    match target {
        Target::Window(window) => instance.create_surface(window).expect("create wgpu surface"),
        Target::Wayland { display, surface } => {
            let target = wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display))),
                raw_window_handle: RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)),
            };
            // SAFETY: `Target::Wayland` promises both outlive the surface.
            unsafe { instance.create_surface_unsafe(target) }.expect("create wgpu surface")
        }
    }
}
