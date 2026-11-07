//! Windows' Desktop Window Manager: the blurred backdrop behind a
//! see-through window ([`Blur`], same interface as `blur.rs`) and the
//! title bar following the theme's brightness ([`set_dark_title_bar`]).
//!
//! The blur is DWM's own Acrylic (`DWMWA_SYSTEMBACKDROP_TYPE`, Windows 11
//! 22H2 and newer) behind the whole window: the frame extended into the
//! client area, so DWM draws the backdrop under it, and the frames come
//! premultiplied through DirectComposition (`gpu::composition`) on top.
//! Like every Acrylic window, it turns solid while the window is inactive.

use std::ffi::c_void;
use std::mem::size_of;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DWMSBT_AUTO, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmExtendFrameIntoClientArea,
    DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use windows_sys::Win32::UI::Controls::MARGINS;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// The first build with `DWMWA_SYSTEMBACKDROP_TYPE` that's documented:
/// Windows 11 22H2. (Insider builds from 22523 had it too.)
const BACKDROP_BUILD: u32 = 22621;

/// The window's Acrylic backdrop, where Windows has one.
pub struct Blur {
    hwnd: HWND,
    on: bool,
}

impl Blur {
    pub fn new(window: &Window) -> Option<Self> {
        if windows_build() < BACKDROP_BUILD {
            log::info!("no system backdrop before Windows build {BACKDROP_BUILD}");
            return None;
        }
        // Only a window drawn through DirectComposition can show it: the
        // swapchain of a plain one covers it, see-through or not.
        if !crate::gpu::composition() {
            return None;
        }
        Some(Self { hwnd: hwnd(window)?, on: false })
    }

    /// # Safety
    /// None needed; the signature is `blur.rs`'s. Windows has no Wayland
    /// surfaces.
    pub unsafe fn for_surface(_display: *mut c_void, _surface: *mut c_void) -> Option<Self> {
        None
    }

    /// Acrylic behind the whole window, or back to Windows' default. The
    /// size is the Wayland region's; DWM covers the window by itself.
    pub fn set(&mut self, on: bool, _size: (i32, i32)) {
        if on == self.on {
            return;
        }
        self.on = on;
        // -1 on every side: "sheet of glass", the whole client area.
        let inset = if on { -1 } else { 0 };
        let margins = MARGINS { cxLeftWidth: inset, cxRightWidth: inset, cyTopHeight: inset, cyBottomHeight: inset };
        let backdrop = if on { DWMSBT_TRANSIENTWINDOW } else { DWMSBT_AUTO };
        // SAFETY: a live window of ours; the attribute's value is the i32
        // it's documented to take.
        unsafe {
            let hr = DwmExtendFrameIntoClientArea(self.hwnd, &margins);
            if hr < 0 {
                log::warn!("DwmExtendFrameIntoClientArea failed: 0x{hr:08X}");
            }
            let hr = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE as u32,
                (&backdrop as *const i32).cast(),
                size_of::<i32>() as u32,
            );
            if hr < 0 {
                log::warn!("setting the system backdrop failed: 0x{hr:08X}");
            }
        }
    }
}

/// Draw the window's title bar and frame dark (`DWMWA_USE_IMMERSIVE_DARK_MODE`)
/// or light, to go with the theme instead of with Windows' own mode. Only
/// on a change: the frame is redrawn for it.
pub fn set_dark_title_bar(window: &Window, dark: bool) {
    let Some(hwnd) = hwnd(window) else { return };
    let attribute = DWMWA_USE_IMMERSIVE_DARK_MODE as u32;
    let mut current: i32 = -1;
    // SAFETY: a live window of ours; the attribute is a BOOL (i32) both ways.
    unsafe {
        let read = DwmGetWindowAttribute(hwnd, attribute, (&mut current as *mut i32).cast(), size_of::<i32>() as u32);
        if read >= 0 && (current != 0) == dark {
            return;
        }
        let value = i32::from(dark);
        let hr = DwmSetWindowAttribute(hwnd, attribute, (&value as *const i32).cast(), size_of::<i32>() as u32);
        if hr < 0 {
            // Windows 10 before 20H1 (build 19041) doesn't know it.
            log::info!("no dark title bar: 0x{hr:08X}");
            return;
        }
        // Windows 10 repaints the title bar only when it has to; have it.
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
        );
    }
}

fn hwnd(window: &Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}

/// Windows' build number, as `winver` shows it (22631 for 23H2). Asked of
/// `RtlGetVersion`, which -- unlike `GetVersionEx` -- doesn't depend on
/// what the exe's manifest claims to be compatible with.
fn windows_build() -> u32 {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    // SAFETY: zeroes are a valid OSVERSIONINFOW; its size is set as the
    // function wants it.
    unsafe {
        let mut info: OSVERSIONINFOW = std::mem::zeroed();
        info.dwOSVersionInfoSize = size_of::<OSVERSIONINFOW>() as u32;
        if RtlGetVersion(&mut info) < 0 {
            return 0;
        }
        info.dwBuildNumber
    }
}
