//! Core wgpu setup: instance, adapter, device, queue and the window
//! surface. Just the device-level plumbing -- the actual draw calls live
//! in `render/`.

use std::sync::Arc;

use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

pub struct GpuState {
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub format: wgpu::TextureFormat,
    /// How the surface can be composited with what's behind the window.
    alpha_modes: Vec<wgpu::CompositeAlphaMode>,
}

impl GpuState {
    pub async fn new(window: Arc<Window>, event_loop: &ActiveEventLoop) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));

        let surface = instance.create_surface(window.clone()).expect("create wgpu surface");

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
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            // A terminal is all about input latency; one frame in flight
            // is plenty for what little it draws.
            desired_maximum_frame_latency: 1,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        Self { instance, device, queue, surface, surface_config, format, alpha_modes }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface_config.width = width.max(1);
        self.surface_config.height = height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// The window can be see-through: the surface takes premultiplied
    /// alpha, which is what the quad, text and egui passes produce.
    pub fn supports_translucency(&self) -> bool {
        self.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied)
    }

    /// Whether the surface is see-through right now.
    pub fn translucent(&self) -> bool {
        self.surface_config.alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied
    }

    /// Make the surface see-through (where supported) or opaque.
    pub fn set_translucent(&mut self, on: bool) {
        let mode = if on && self.supports_translucency() {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Opaque
        };
        if mode != self.surface_config.alpha_mode {
            self.surface_config.alpha_mode = mode;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }
}
