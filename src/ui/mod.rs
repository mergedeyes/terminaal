//! egui, for the app's chrome -- the sidebar, the terminal's right-click menu
//! and the startup splash. The terminal
//! grid and the tab bar stay on the hand-rolled renderer in `render/`;
//! egui draws in its own render pass on top of them.
//!
//! Every winit event goes through [`UiLayer::on_window_event`] first;
//! `app.rs` then decides whether the terminal should see it as well.

pub mod context_menu;
pub mod keys_panel;
pub mod sidebar;
pub mod splash;
pub mod ssh_panel;
pub mod theme;
pub mod widgets;

use std::time::Duration;

use winit::event::WindowEvent;
use winit::window::Window;

pub struct UiLayer {
    pub ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    /// Tessellated result of the last [`UiLayer::run`], drawn by the next
    /// [`UiLayer::paint`].
    frame: Option<PreparedFrame>,
    /// Textures egui is done with; freed once the frame is submitted.
    textures_to_free: Vec<egui::TextureId>,
}

struct PreparedFrame {
    primitives: Vec<egui::ClippedPrimitive>,
    pixels_per_point: f32,
}

impl UiLayer {
    pub fn new(window: &Window, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let renderer =
            egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions { msaa_samples: 1, ..Default::default() });
        Self { ctx, state, renderer, frame: None, textures_to_free: Vec::new() }
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// A sidebar widget (text field, button, ...) has keyboard focus.
    pub fn wants_keyboard(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }

    /// Take keyboard focus away from whatever sidebar widget has it.
    pub fn release_keyboard(&self) {
        self.ctx.memory_mut(|memory| memory.stop_text_input());
    }

    /// Run one egui pass and upload the textures it changed -- right away,
    /// so they stay in sync even if this frame never gets presented.
    /// Returns how soon egui wants to run again without new input
    /// (animations, a text cursor blinking), or `None` for "not until
    /// something happens".
    pub fn run(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        build: impl FnMut(&mut egui::Ui),
    ) -> Option<Duration> {
        let input = self.state.take_egui_input(window);
        let output = self.ctx.run_ui(input, build);
        self.state.handle_platform_output(window, output.platform_output);

        // `TexturesDelta` debug-asserts on drop that every delta was taken
        // out and handled, so empty it rather than iterate over it.
        let mut textures = output.textures_delta;
        for (id, deltas) in std::mem::take(&mut textures.set) {
            for delta in &deltas {
                self.renderer.update_texture(device, queue, id, delta);
            }
        }
        self.textures_to_free.extend(std::mem::take(&mut textures.free));

        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        self.frame = Some(PreparedFrame { primitives, pixels_per_point: output.pixels_per_point });

        output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|viewport| viewport.repaint_delay)
            .filter(|delay| *delay < Duration::from_secs(3600))
    }

    /// Record the last run's draw commands into `encoder`, on top of what
    /// is already in `view`. The returned command buffers (egui's buffer
    /// uploads) must be submitted before `encoder`.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size_in_pixels: [u32; 2],
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(frame) = self.frame.take() else { return Vec::new() };
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels, pixels_per_point: frame.pixels_per_point };
        let uploads = self.renderer.update_buffers(device, queue, encoder, &frame.primitives, &screen);

        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        self.renderer.render(&mut pass, &frame.primitives, &screen);
        uploads
    }

    /// Call after the frame that last used them has been submitted.
    pub fn free_textures(&mut self) {
        for id in self.textures_to_free.drain(..) {
            self.renderer.free_texture(&id);
        }
    }
}
