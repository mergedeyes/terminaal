//! The drop-down window as a layer surface (`zwlr_layer_shell_v1`): along
//! the top edge of the screen, above other windows, where no normal
//! (xdg) window may place itself. winit can't make one, so this speaks
//! Wayland itself -- on winit's connection with an event queue of its own,
//! like `blur.rs`. winit wakes up for anything arriving on the socket;
//! `app.rs` then calls [`Layer::dispatch`] and gets the events.
//!
//! Hidden means gone: [`Layer::hide`] destroys the surface and
//! [`Layer::show`] makes a new one (the GPU surface on top of it is made
//! anew too). Its height is a share of the room the output has left
//! beside panels: the first configure of a surface anchored to all four
//! edges says how much that is, before anything is drawn.

use std::time::{Duration, Instant};

use wayland_backend::client::Backend;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_keyboard::{self, KeyState, KeymapFormat, WlKeyboard};
use wayland_client::protocol::wl_pointer::{self, Axis, AxisSource, ButtonState, WlPointer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, Capability, WlSeat};
use wayland_client::protocol::wl_surface::{self, WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{self, WpFractionalScaleV1};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::{self, ZwlrLayerShellV1};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::ModifiersState;
use winit::window::CursorIcon;

use super::keys::Keyboard;
use crate::input::KeyInput;

/// What happened to the drop-down window, for `app.rs`.
#[derive(Debug)]
pub enum LayerEvent {
    /// A new size in physical pixels, or a new scale.
    Resized { width: u32, height: u32, scale: f64 },
    /// The compositor took the surface away (its output went, say).
    Closed,
    Focused(bool),
    Modifiers(ModifiersState),
    Key(KeyInput),
    /// In physical pixels.
    PointerMoved(PhysicalPosition<f64>),
    PointerLeft,
    Button(MouseButton, ElementState),
    Wheel(MouseScrollDelta),
}

pub struct Layer {
    conn: Connection,
    queue: EventQueue<Wl>,
    wl: Wl,
}

struct Wl {
    compositor: WlCompositor,
    shell: ZwlrLayerShellV1,
    fractional: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    cursor_shape: Option<WpCursorShapeManagerV1>,
    /// Kept bound: keyboard and pointer come from it.
    _seat: Option<WlSeat>,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    cursor_device: Option<WpCursorShapeDeviceV1>,
    shown: Option<Shown>,
    events: Vec<LayerEvent>,
    keys: Keyboard,
    /// The key being held, what it types, and when it types again.
    repeat: Option<(u32, KeyInput, Instant)>,
    pointer_serial: u32,
    cursor: CursorIcon,
    /// Wheel movement of the current pointer frame.
    axis: AxisFrame,
}

#[derive(Default)]
struct AxisFrame {
    source: Option<AxisSource>,
    /// Notches in 120ths, per axis (x, y).
    value120: (i32, i32),
    /// Surface-local distance, per axis.
    value: (f64, f64),
}

struct Shown {
    surface: WlSurface,
    layer: ZwlrLayerSurfaceV1,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    /// Size in surface-local (logical) pixels, as last configured.
    size: (u32, u32),
    /// Scale in 120ths.
    scale120: u32,
    /// Logical height the output has room for, from the first probe.
    room: u32,
    /// [`Layer::show`] is sizing it and handles configures itself.
    probing: bool,
    /// The latest configure while probing: serial and size.
    configure: Option<(u32, u32, u32)>,
}

impl Shown {
    fn scale(&self) -> f64 {
        f64::from(self.scale120) / 120.0
    }

    fn physical(&self) -> (u32, u32) {
        let scale = self.scale();
        let px = |logical: u32| ((f64::from(logical) * scale).round() as u32).max(1);
        (px(self.size.0), px(self.size.1))
    }
}

/// When nothing says otherwise.
const FALLBACK_HEIGHT: u32 = 400;

impl Layer {
    /// On winit's Wayland connection; `None` without a layer shell.
    ///
    /// # Safety
    /// `display` must be winit's live `wl_display`, outliving this.
    pub unsafe fn new(display: *mut std::ffi::c_void) -> Option<Self> {
        let conn = Connection::from_backend(unsafe { Backend::from_foreign_display(display.cast()) });
        let (globals, queue) = registry_queue_init::<Wl>(&conn).ok()?;
        let qh = queue.handle();
        let Ok(shell) = globals.bind::<ZwlrLayerShellV1, _, _>(&qh, 1..=4, ()) else {
            log::info!("the compositor has no layer shell");
            return None;
        };
        let compositor: WlCompositor = globals.bind(&qh, 1..=6, ()).ok()?;
        let seat: Option<WlSeat> = globals.bind(&qh, 1..=8, ()).ok();
        let wl = Wl {
            compositor,
            shell,
            fractional: globals.bind(&qh, 1..=1, ()).ok(),
            viewporter: globals.bind(&qh, 1..=1, ()).ok(),
            cursor_shape: globals.bind(&qh, 1..=1, ()).ok(),
            _seat: seat,
            keyboard: None,
            pointer: None,
            cursor_device: None,
            shown: None,
            events: Vec::new(),
            keys: Keyboard::new(),
            repeat: None,
            pointer_serial: 0,
            cursor: CursorIcon::Default,
            axis: AxisFrame::default(),
        };
        let mut layer = Self { conn, queue, wl };
        // The seat's capabilities, and with them keyboard and pointer.
        layer.queue.roundtrip(&mut layer.wl).ok()?;
        Some(layer)
    }

    pub fn is_shown(&self) -> bool {
        self.wl.shown.is_some()
    }

    /// Put the window up with `height` percent of the output's height.
    /// Returns the `wl_surface` for the GPU, its physical size and scale,
    /// once the compositor configured it.
    pub fn show(&mut self, height: f32) -> Option<(*mut std::ffi::c_void, (u32, u32), f64)> {
        if self.wl.shown.is_none() {
            self.create_surface();
            let shown = self.wl.shown.as_mut()?;
            shown.probing = true;
            // First: how much room there is.
            shown.layer.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
            shown.layer.set_size(0, 0);
            shown.surface.commit();
            let (serial, _, room) = self.wait_for_configure()?;
            let shown = self.wl.shown.as_mut()?;
            shown.layer.ack_configure(serial);
            shown.room = if room == 0 { FALLBACK_HEIGHT * 2 } else { room };
        }
        let shown = self.wl.shown.as_mut()?;
        shown.probing = true;
        let room = shown.room;
        let wanted = ((room as f32 * height.clamp(10.0, 100.0) / 100.0).round() as u32).max(1);
        shown.layer.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
        shown.layer.set_size(0, wanted);
        shown.surface.commit();
        let (serial, width, height) = self.wait_for_configure()?;
        let shown = self.wl.shown.as_mut()?;
        shown.layer.ack_configure(serial);
        shown.probing = false;
        shown.size = (width.max(1), if height == 0 { wanted } else { height });
        if let (Some(viewport), true) = (&shown.viewport, shown.fractional.is_some()) {
            viewport.set_destination(shown.size.0 as i32, shown.size.1 as i32);
        }
        let ptr = shown.surface.id().as_ptr().cast();
        let (physical, scale) = (shown.physical(), shown.scale());
        self.flush();
        Some((ptr, physical, scale))
    }

    fn create_surface(&mut self) {
        let qh = self.queue.handle();
        let wl = &mut self.wl;
        let surface = wl.compositor.create_surface(&qh, ());
        let layer = wl.shell.get_layer_surface(&surface, None, zwlr_layer_shell_v1::Layer::Top, "terminaal".into(), &qh, ());
        layer.set_exclusive_zone(0);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        let (fractional, viewport) = match (&wl.fractional, &wl.viewporter) {
            (Some(manager), Some(viewporter)) => {
                (Some(manager.get_fractional_scale(&surface, &qh, ())), Some(viewporter.get_viewport(&surface, &qh, ())))
            }
            _ => (None, None),
        };
        wl.shown = Some(Shown { surface, layer, fractional, viewport, size: (0, 0), scale120: 120, room: 0, probing: false, configure: None });
    }

    /// Dispatch until the surface got a configure: serial, width, height.
    fn wait_for_configure(&mut self) -> Option<(u32, u32, u32)> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(configure) = self.wl.shown.as_mut()?.configure.take() {
                return Some(configure);
            }
            if Instant::now() > deadline {
                log::warn!("the compositor didn't configure the drop-down window");
                self.hide();
                return None;
            }
            if let Err(err) = self.queue.roundtrip(&mut self.wl) {
                log::warn!("wayland roundtrip failed: {err}");
                return None;
            }
        }
    }

    /// Take the window away. The GPU surface on it must be gone already.
    pub fn hide(&mut self) {
        if let Some(shown) = self.wl.shown.take() {
            if let Some(fractional) = shown.fractional {
                fractional.destroy();
            }
            if let Some(viewport) = shown.viewport {
                viewport.destroy();
            }
            shown.layer.destroy();
            shown.surface.destroy();
        }
        self.wl.repeat = None;
        self.flush();
    }

    /// Change the height of the window that's up.
    pub fn set_height(&mut self, height: f32) {
        if self.is_shown() {
            self.show(height);
            if let Some(shown) = &self.wl.shown {
                let (width, height) = shown.physical();
                self.wl.events.push(LayerEvent::Resized { width, height, scale: shown.scale() });
            }
        }
    }

    /// The `wl_surface` while the window is up, for the blur.
    pub fn surface_ptr(&self) -> Option<*mut std::ffi::c_void> {
        self.wl.shown.as_ref().map(|shown| shown.surface.id().as_ptr().cast())
    }

    pub fn set_cursor(&mut self, icon: CursorIcon) {
        self.wl.cursor = icon;
        self.wl.apply_cursor();
        self.flush();
    }

    /// When a held key types again.
    pub fn next_repeat(&self) -> Option<Instant> {
        self.wl.repeat.as_ref().map(|(_, _, at)| *at)
    }

    /// Everything that happened since the last call: queued Wayland events
    /// and key repeats that are due.
    pub fn dispatch(&mut self) -> Vec<LayerEvent> {
        if let Err(err) = self.queue.dispatch_pending(&mut self.wl) {
            log::warn!("wayland dispatch failed: {err}");
        }
        let now = Instant::now();
        if let Some((_, key, at)) = &mut self.wl.repeat
            && now >= *at
        {
            let (rate, _) = self.wl.keys.repeat.unwrap_or((25, Duration::ZERO));
            let interval = Duration::from_secs_f64(1.0 / f64::from(rate.max(1)));
            // Catch up without flooding after a stall.
            *at = (*at + interval).max(now);
            self.wl.events.push(LayerEvent::Key(key.clone()));
        }
        self.flush();
        std::mem::take(&mut self.wl.events)
    }

    fn flush(&self) {
        if let Err(err) = self.conn.flush() {
            log::warn!("wayland flush failed: {err}");
        }
    }
}

impl Wl {
    fn is_current(&self, surface: &WlSurface) -> bool {
        self.shown.as_ref().is_some_and(|shown| shown.surface == *surface)
    }

    fn apply_cursor(&self) {
        let Some(device) = &self.cursor_device else { return };
        device.set_shape(self.pointer_serial, shape_of(self.cursor));
    }

    fn resized(&mut self) {
        if let Some(shown) = &self.shown {
            let (width, height) = shown.physical();
            self.events.push(LayerEvent::Resized { width, height, scale: shown.scale() });
        }
    }
}

fn shape_of(icon: CursorIcon) -> Shape {
    match icon {
        CursorIcon::Pointer => Shape::Pointer,
        CursorIcon::Text => Shape::Text,
        CursorIcon::ColResize | CursorIcon::EwResize => Shape::ColResize,
        CursorIcon::RowResize | CursorIcon::NsResize => Shape::RowResize,
        CursorIcon::Grab => Shape::Grab,
        CursorIcon::Grabbing => Shape::Grabbing,
        CursorIcon::NotAllowed => Shape::NotAllowed,
        CursorIcon::Wait => Shape::Wait,
        CursorIcon::Crosshair => Shape::Crosshair,
        CursorIcon::Move => Shape::Move,
        _ => Shape::Default,
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for Wl {
    fn event(wl: &mut Self, _: &ZwlrLayerSurfaceV1, event: zwlr_layer_surface_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                let Some(shown) = &mut wl.shown else { return };
                if shown.probing {
                    // Still inside `show`, which acks it.
                    shown.configure = Some((serial, width, height));
                    return;
                }
                shown.layer.ack_configure(serial);
                let size = (width.max(1), if height == 0 { shown.size.1 } else { height });
                if size != shown.size {
                    shown.size = size;
                    if let Some(viewport) = &shown.viewport {
                        viewport.set_destination(size.0 as i32, size.1 as i32);
                    }
                    wl.resized();
                }
            }
            zwlr_layer_surface_v1::Event::Closed => wl.events.push(LayerEvent::Closed),
            _ => {}
        }
    }
}

impl Dispatch<WlSurface, ()> for Wl {
    fn event(wl: &mut Self, surface: &WlSurface, event: wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        // Integer scales, where there's no fractional scaling.
        if let wl_surface::Event::PreferredBufferScale { factor } = event
            && let Some(shown) = &mut wl.shown
            && shown.fractional.is_none()
            && factor > 0
        {
            shown.scale120 = factor as u32 * 120;
            surface.set_buffer_scale(factor);
            if shown.size != (0, 0) {
                wl.resized();
            }
        }
    }
}

impl Dispatch<WpFractionalScaleV1, ()> for Wl {
    fn event(wl: &mut Self, _: &WpFractionalScaleV1, event: wp_fractional_scale_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && let Some(shown) = &mut wl.shown
            && shown.scale120 != scale
        {
            shown.scale120 = scale;
            if shown.size != (0, 0) {
                wl.resized();
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for Wl {
    fn event(wl: &mut Self, seat: &WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event else { return };
        if caps.contains(Capability::Keyboard) {
            wl.keyboard.get_or_insert_with(|| seat.get_keyboard(qh, ()));
        } else if let Some(keyboard) = wl.keyboard.take() {
            keyboard.release();
        }
        if caps.contains(Capability::Pointer) {
            if wl.pointer.is_none() {
                let pointer = seat.get_pointer(qh, ());
                wl.cursor_device = wl.cursor_shape.as_ref().map(|manager| manager.get_pointer(&pointer, qh, ()));
                wl.pointer = Some(pointer);
            }
        } else if let Some(pointer) = wl.pointer.take() {
            if let Some(device) = wl.cursor_device.take() {
                device.destroy();
            }
            pointer.release();
        }
    }
}

impl Dispatch<WlKeyboard, ()> for Wl {
    fn event(wl: &mut Self, _: &WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Keymap { format: WEnum::Value(KeymapFormat::XkbV1), fd, size } => {
                wl.keys.set_keymap(fd, size);
            }
            // Only for the surface that's up: a leave for the one just
            // taken away would hide the new one right away.
            wl_keyboard::Event::Enter { surface, .. } if wl.is_current(&surface) => {
                log::debug!("drop-down window got the keyboard");
                wl.events.push(LayerEvent::Focused(true));
            }
            wl_keyboard::Event::Leave { surface, .. } if wl.is_current(&surface) => {
                log::debug!("drop-down window lost the keyboard");
                wl.repeat = None;
                wl.events.push(LayerEvent::Focused(false));
            }
            wl_keyboard::Event::Modifiers { mods_depressed, mods_latched, mods_locked, group, .. } => {
                wl.keys.set_modifiers(mods_depressed, mods_latched, mods_locked, group);
                wl.events.push(LayerEvent::Modifiers(wl.keys.modifiers));
            }
            wl_keyboard::Event::Key { key, state, .. } => {
                let pressed = matches!(state, WEnum::Value(KeyState::Pressed));
                let state = if pressed { ElementState::Pressed } else { ElementState::Released };
                let Some(input) = wl.keys.key(key, state) else { return };
                if pressed {
                    wl.repeat = match wl.keys.repeat {
                        Some((_, delay)) if wl.keys.repeats(key) => Some((key, input.clone(), Instant::now() + delay)),
                        _ => None,
                    };
                } else if wl.repeat.as_ref().is_some_and(|(held, ..)| *held == key) {
                    wl.repeat = None;
                }
                wl.events.push(LayerEvent::Key(input));
            }
            wl_keyboard::Event::RepeatInfo { rate, delay } => {
                wl.keys.repeat = (rate > 0).then(|| (rate as u32, Duration::from_millis(delay.max(0) as u64)));
            }
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for Wl {
    fn event(wl: &mut Self, _: &WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let scale = wl.shown.as_ref().map_or(1.0, Shown::scale);
        let position = |x: f64, y: f64| PhysicalPosition::new(x * scale, y * scale);
        match event {
            wl_pointer::Event::Enter { serial, surface_x, surface_y, .. } => {
                wl.pointer_serial = serial;
                wl.apply_cursor();
                wl.events.push(LayerEvent::PointerMoved(position(surface_x, surface_y)));
            }
            wl_pointer::Event::Leave { .. } => wl.events.push(LayerEvent::PointerLeft),
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                wl.events.push(LayerEvent::PointerMoved(position(surface_x, surface_y)));
            }
            wl_pointer::Event::Button { button, state, .. } => {
                // Linux input event codes.
                let button = match button {
                    0x110 => MouseButton::Left,
                    0x111 => MouseButton::Right,
                    0x112 => MouseButton::Middle,
                    0x113 => MouseButton::Back,
                    0x114 => MouseButton::Forward,
                    other => MouseButton::Other(other as u16),
                };
                let state = match state {
                    WEnum::Value(ButtonState::Pressed) => ElementState::Pressed,
                    _ => ElementState::Released,
                };
                wl.events.push(LayerEvent::Button(button, state));
            }
            wl_pointer::Event::AxisSource { axis_source: WEnum::Value(source) } => wl.axis.source = Some(source),
            wl_pointer::Event::AxisValue120 { axis, value120 } => match axis {
                WEnum::Value(Axis::VerticalScroll) => wl.axis.value120.1 += value120,
                WEnum::Value(Axis::HorizontalScroll) => wl.axis.value120.0 += value120,
                _ => {}
            },
            wl_pointer::Event::AxisDiscrete { axis, discrete } => match axis {
                WEnum::Value(Axis::VerticalScroll) => wl.axis.value120.1 += discrete * 120,
                WEnum::Value(Axis::HorizontalScroll) => wl.axis.value120.0 += discrete * 120,
                _ => {}
            },
            wl_pointer::Event::Axis { axis, value, .. } => match axis {
                WEnum::Value(Axis::VerticalScroll) => wl.axis.value.1 += value,
                WEnum::Value(Axis::HorizontalScroll) => wl.axis.value.0 += value,
                _ => {}
            },
            wl_pointer::Event::Frame => {
                let axis = std::mem::take(&mut wl.axis);
                // Like winit: a wheel scrolls lines, anything else pixels;
                // Wayland counts down as positive, winit up.
                let wheel = matches!(axis.source, None | Some(AxisSource::Wheel | AxisSource::WheelTilt));
                if wheel && axis.value120 != (0, 0) {
                    let (x, y) = axis.value120;
                    wl.events.push(LayerEvent::Wheel(MouseScrollDelta::LineDelta(-x as f32 / 120.0, -y as f32 / 120.0)));
                } else if axis.value != (0.0, 0.0) {
                    let (x, y) = axis.value;
                    wl.events.push(LayerEvent::Wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(-x * scale, -y * scale))));
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Wl {
    fn event(_: &mut Self, _: &WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_noop!(Wl: ignore WlCompositor);
delegate_noop!(Wl: ignore ZwlrLayerShellV1);
delegate_noop!(Wl: ignore WpFractionalScaleManagerV1);
delegate_noop!(Wl: ignore WpViewporter);
delegate_noop!(Wl: ignore WpViewport);
delegate_noop!(Wl: ignore WpCursorShapeManagerV1);
delegate_noop!(Wl: ignore WpCursorShapeDeviceV1);
