//! Blur behind the translucent window (`blur` in the config) on Wayland
//! compositors with `ext_background_effect_v1` -- COSMIC among them.
//! winit doesn't speak that protocol (its `set_blur` is KWin's, which
//! `app.rs` calls as well), so this binds it on winit's own connection:
//! libwayland lets a second event queue share the display, and the
//! window's `wl_surface` is borrowed from winit by pointer. The blur region
//! is double-buffered surface state, applied by the next commit -- the
//! next frame's present.

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window;

pub struct Blur {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    surface: WlSurface,
    compositor: WlCompositor,
    manager: ExtBackgroundEffectManagerV1,
    /// There while the window is blurred.
    effect: Option<ExtBackgroundEffectSurfaceV1>,
}

#[derive(Default)]
struct State {
    /// The compositor can blur (its latest `capabilities`).
    blur: bool,
}

impl Blur {
    /// `None` off Wayland, or when the compositor can't blur.
    pub fn new(window: &Window) -> Option<Self> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else { return None };
        let RawWindowHandle::Wayland(handle) = window.window_handle().ok()?.as_raw() else { return None };
        // SAFETY: both pointers come from winit's live connection and
        // window; `AppState` drops this before the window.
        let conn = Connection::from_backend(unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) });
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()) }.ok()?;
        let surface = WlSurface::from_id(&conn, id).ok()?;

        let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();
        let manager: ExtBackgroundEffectManagerV1 = globals.bind(&qh, 1..=1, ()).ok()?;
        let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).ok()?;
        let mut state = State::default();
        // Brings the manager's capabilities.
        queue.roundtrip(&mut state).ok()?;
        if !state.blur {
            log::info!("the compositor offers ext_background_effect, but no blur");
            return None;
        }
        log::info!("the compositor can blur behind the window (ext_background_effect)");
        Some(Self { conn, queue, state, surface, compositor, manager, effect: None })
    }

    /// Blur behind the whole window -- `size` in logical pixels -- or stop.
    pub fn set(&mut self, on: bool, (width, height): (i32, i32)) {
        let qh = self.queue.handle();
        if on {
            let effect = self.effect.get_or_insert_with(|| self.manager.get_background_effect(&self.surface, &qh, ()));
            let region = self.compositor.create_region(&qh, ());
            region.add(0, 0, width, height);
            effect.set_blur_region(Some(&region));
            region.destroy();
        } else if let Some(effect) = self.effect.take() {
            effect.set_blur_region(None);
            effect.destroy();
        }
        if let Err(err) = self.conn.flush() {
            log::warn!("failed to send the blur region: {err}");
        }
        // Nothing to act on, but it keeps the queue from growing.
        if let Err(err) = self.queue.dispatch_pending(&mut self.state) {
            log::warn!("wayland dispatch failed: {err}");
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            state.blur =
                matches!(flags, WEnum::Value(flags) if flags.contains(ext_background_effect_manager_v1::Capability::Blur));
        }
    }
}

delegate_noop!(State: ignore WlCompositor);
delegate_noop!(State: ignore WlRegion);
delegate_noop!(State: ignore ExtBackgroundEffectSurfaceV1);
