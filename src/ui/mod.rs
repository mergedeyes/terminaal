//! egui, for the app's chrome -- the sidebar, the terminal's right-click menu
//! and the startup splash. The terminal
//! grid and the tab bar stay on the hand-rolled renderer in `render/`;
//! egui draws in its own render pass on top of them.
//!
//! Every winit event goes through [`UiLayer::on_window_event`] first;
//! `app.rs` then decides whether the terminal should see it as well. The
//! drop-down window's layer surface has no winit window for egui-winit to
//! work with: its events come in through [`UiLayer::on_layer_event`] and
//! are turned into egui's input here.

pub mod commands_panel;
pub mod context_menu;
pub mod files_panel;
pub mod keys_panel;
pub mod settings_panel;
pub mod sidebar;
pub mod splash;
pub mod ssh_panel;
pub mod theme;
pub mod widgets;

use std::time::{Duration, Instant};

use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, ModifiersState};
use winit::window::{CursorIcon, Window};

use crate::quake::layer::LayerEvent;
use crate::window::AppWindow;

pub struct UiLayer {
    pub ctx: egui::Context,
    input: Input,
    renderer: egui_wgpu::Renderer,
    /// Tessellated result of the last [`UiLayer::run`], drawn by the next
    /// [`UiLayer::paint`].
    frame: Option<PreparedFrame>,
    /// Textures egui is done with; freed once the frame is submitted.
    textures_to_free: Vec<egui::TextureId>,
}

enum Input {
    Winit(Box<egui_winit::State>),
    /// Built by hand, for the layer surface.
    Manual(Box<Manual>),
}

#[derive(Default)]
struct Manual {
    raw: egui::RawInput,
    modifiers: egui::Modifiers,
    start: Option<Instant>,
    /// Text egui put on the clipboard, for `app.rs` to take.
    copied: Option<String>,
    /// The pointer shape egui wanted last.
    cursor: egui::CursorIcon,
    /// It wants another one, for `app.rs` to set.
    cursor_changed: Option<CursorIcon>,
}

struct PreparedFrame {
    primitives: Vec<egui::ClippedPrimitive>,
    pixels_per_point: f32,
}

impl UiLayer {
    /// For a winit window, or with `None` for the layer surface.
    pub fn new(window: Option<&Window>, device: &wgpu::Device, format: wgpu::TextureFormat, colors: &crate::theme::UiColors) -> Self {
        let ctx = egui::Context::default();
        theme::apply(&ctx, colors, 1.0);
        let input = match window {
            Some(window) => Input::Winit(Box::new(egui_winit::State::new(
                ctx.clone(),
                egui::ViewportId::ROOT,
                window,
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(device.limits().max_texture_dimension_2d as usize),
            ))),
            None => {
                let raw = egui::RawInput { max_texture_side: Some(device.limits().max_texture_dimension_2d as usize), ..Default::default() };
                Input::Manual(Box::new(Manual { raw, start: Some(Instant::now()), ..Default::default() }))
            }
        };
        let renderer =
            egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions { msaa_samples: 1, ..Default::default() });
        Self { ctx, input, renderer, frame: None, textures_to_free: Vec::new() }
    }

    /// Returns whether egui wants to repaint for it.
    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        match &mut self.input {
            Input::Winit(state) => state.on_window_event(window, event).repaint,
            Input::Manual(_) => false,
        }
    }

    /// An event of the layer surface; `paste` gets the clipboard for
    /// Ctrl+V. Returns whether egui wants to repaint for it.
    pub fn on_layer_event(&mut self, event: &LayerEvent, paste: impl FnOnce() -> Option<String>) -> bool {
        let ppp = self.ctx.pixels_per_point();
        let Input::Manual(manual) = &mut self.input else { return false };
        let modifiers = manual.modifiers;
        let events = &mut manual.raw.events;
        match event {
            LayerEvent::PointerMoved(position) => {
                events.push(egui::Event::PointerMoved(egui::pos2(position.x as f32 / ppp, position.y as f32 / ppp)));
            }
            LayerEvent::PointerLeft => events.push(egui::Event::PointerGone),
            LayerEvent::Button(button, state) => {
                let button = match button {
                    MouseButton::Left => egui::PointerButton::Primary,
                    MouseButton::Right => egui::PointerButton::Secondary,
                    MouseButton::Middle => egui::PointerButton::Middle,
                    MouseButton::Back => egui::PointerButton::Extra1,
                    MouseButton::Forward => egui::PointerButton::Extra2,
                    MouseButton::Other(_) => return false,
                };
                let Some(pos) = self.ctx.input(|input| input.pointer.latest_pos()) else { return false };
                events.push(egui::Event::PointerButton { pos, button, pressed: *state == ElementState::Pressed, modifiers });
            }
            LayerEvent::Wheel(delta) => {
                let (unit, delta) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (egui::MouseWheelUnit::Line, egui::vec2(*x, *y)),
                    MouseScrollDelta::PixelDelta(p) => (egui::MouseWheelUnit::Point, egui::vec2(p.x as f32, p.y as f32) / ppp),
                };
                events.push(egui::Event::MouseWheel { unit, delta, phase: egui::TouchPhase::Move, modifiers });
            }
            LayerEvent::Modifiers(state) => {
                manual.modifiers = egui_modifiers(*state);
            }
            LayerEvent::Focused(focused) => {
                manual.raw.focused = *focused;
                events.push(egui::Event::WindowFocused(*focused));
            }
            LayerEvent::Key(key) => {
                let pressed = key.state == ElementState::Pressed;
                let Some(egui_key) = egui_key(&key.key_without_modifiers) else {
                    return push_text(events, key, modifiers);
                };
                if pressed && modifiers.command {
                    match egui_key {
                        egui::Key::C => return push(events, egui::Event::Copy),
                        egui::Key::X => return push(events, egui::Event::Cut),
                        egui::Key::V => {
                            if let Some(text) = paste().map(|text| text.replace("\r\n", "\n")).filter(|text| !text.is_empty()) {
                                events.push(egui::Event::Paste(text));
                            }
                            return true;
                        }
                        _ => {}
                    }
                }
                events.push(egui::Event::Key { key: egui_key, physical_key: None, pressed, repeat: false, modifiers });
                push_text(events, key, modifiers);
            }
            LayerEvent::Resized { .. } | LayerEvent::Closed => {}
        }
        true
    }

    /// Text egui copied since the last call (layer surface only).
    pub fn take_copied(&mut self) -> Option<String> {
        match &mut self.input {
            Input::Manual(manual) => manual.copied.take(),
            Input::Winit(_) => None,
        }
    }

    /// The pointer shape egui switched to since the last call (layer
    /// surface only).
    pub fn take_cursor(&mut self) -> Option<CursorIcon> {
        match &mut self.input {
            Input::Manual(manual) => manual.cursor_changed.take(),
            Input::Winit(_) => None,
        }
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
        window: &AppWindow,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        build: impl FnMut(&mut egui::Ui),
    ) -> Option<Duration> {
        let output = match (&mut self.input, window) {
            (Input::Winit(state), AppWindow::Winit(window)) => {
                let input = state.take_egui_input(window);
                let output = self.ctx.run_ui(input, build);
                state.handle_platform_output(window, output.platform_output.clone());
                output
            }
            (Input::Manual(manual), _) => {
                let scale = window.scale_factor() as f32;
                let size = window.inner_size();
                let mut input = std::mem::take(&mut manual.raw);
                manual.raw.focused = input.focused;
                manual.raw.max_texture_side = input.max_texture_side;
                input.time = manual.start.map(|start| start.elapsed().as_secs_f64());
                input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size.width as f32 / scale, size.height as f32 / scale),
                ));
                input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(scale);
                let output = self.ctx.run_ui(input, build);
                for command in &output.platform_output.commands {
                    match command {
                        egui::OutputCommand::CopyText(text) => manual.copied = Some(text.clone()),
                        egui::OutputCommand::OpenUrl(url) => log::info!("egui wants to open {}; not in the drop-down window", url.url),
                        egui::OutputCommand::CopyImage(_) => {}
                    }
                }
                if output.platform_output.cursor_icon != manual.cursor {
                    manual.cursor = output.platform_output.cursor_icon;
                    manual.cursor_changed = Some(winit_cursor(manual.cursor));
                }
                output
            }
            (Input::Winit(_), AppWindow::Layer(_)) => unreachable!("egui-winit only runs for a winit window"),
        };

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

fn push(events: &mut Vec<egui::Event>, event: egui::Event) -> bool {
    events.push(event);
    true
}

/// The text a key press types, unless a command key is held.
fn push_text(events: &mut Vec<egui::Event>, key: &crate::input::KeyInput, modifiers: egui::Modifiers) -> bool {
    if key.state == ElementState::Pressed
        && !modifiers.ctrl
        && let Some(text) = &key.text
        && !text.is_empty()
        && !text.chars().any(char::is_control)
    {
        events.push(egui::Event::Text(text.to_string()));
    }
    true
}

fn egui_modifiers(state: ModifiersState) -> egui::Modifiers {
    egui::Modifiers {
        alt: state.alt_key(),
        ctrl: state.control_key(),
        shift: state.shift_key(),
        mac_cmd: false,
        command: state.control_key(),
    }
}

/// egui's name for a key; winit's named keys are spelled the same.
fn egui_key(key: &Key) -> Option<egui::Key> {
    match key {
        Key::Named(named) => egui::Key::from_name(&format!("{named:?}")),
        Key::Character(text) => egui::Key::from_name(text),
        _ => None,
    }
}

fn winit_cursor(icon: egui::CursorIcon) -> CursorIcon {
    match icon {
        egui::CursorIcon::PointingHand => CursorIcon::Pointer,
        egui::CursorIcon::Text => CursorIcon::Text,
        egui::CursorIcon::ResizeHorizontal | egui::CursorIcon::ResizeColumn => CursorIcon::ColResize,
        egui::CursorIcon::ResizeVertical | egui::CursorIcon::ResizeRow => CursorIcon::RowResize,
        egui::CursorIcon::Grab => CursorIcon::Grab,
        egui::CursorIcon::Grabbing => CursorIcon::Grabbing,
        egui::CursorIcon::NotAllowed | egui::CursorIcon::NoDrop => CursorIcon::NotAllowed,
        egui::CursorIcon::Wait | egui::CursorIcon::Progress => CursorIcon::Wait,
        egui::CursorIcon::Crosshair => CursorIcon::Crosshair,
        egui::CursorIcon::Move | egui::CursorIcon::AllScroll => CursorIcon::Move,
        _ => CursorIcon::Default,
    }
}
