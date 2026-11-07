//! The drop-down ("Quake") window: `terminaal --quake` puts a Terminaal
//! along the top edge of the screen, or takes it away again. There's no
//! global shortcut a Wayland client may grab, so the desktop's own
//! shortcut settings run that command; on Windows the drop-down
//! Terminaal registers one itself as well ([`hotkey`]).
//!
//! The drop-down Terminaal is an instance of its own, with its own tabs
//! and session. The first `--quake` starts it; later ones find it by its
//! socket in `$XDG_RUNTIME_DIR` (Windows: a loopback port, written to a
//! file in the user's temp folder) and tell it to toggle ([`Toggle`]).
//!
//! On Wayland it's a layer surface ([`layer`]); elsewhere a normal window
//! that shows and hides.

#[cfg(windows)]
pub mod hotkey;
#[cfg(unix)]
pub mod keys;
#[cfg_attr(windows, path = "layer_windows.rs")]
pub mod layer;

use std::io::{self, Read, Write};
#[cfg(windows)]
use std::net::{Ipv4Addr, TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::ModifiersState;

use crate::input::KeyInput;

/// What happened to the drop-down window, for `app.rs`. Never on
/// Windows, which has no layer surfaces.
#[derive(Debug)]
#[cfg_attr(windows, allow(dead_code))]
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

/// What goes over the socket.
const TOGGLE: &[u8] = b"toggle\n";

/// `$XDG_RUNTIME_DIR/terminaal`, else a folder of this user's in the temp
/// dir (Windows: always that).
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|dir| dir.is_absolute()) {
        Some(dir) => dir.join("terminaal"),
        None => crate::platform::user_temp_dir(),
    }
}

/// How `--quake` went.
pub enum Start {
    /// A drop-down Terminaal runs already and was told to toggle.
    Toggled,
    /// This process is the drop-down Terminaal now; it gets toggles here.
    Owner(Toggle),
}

/// Toggle the drop-down Terminaal in `dir`, or become it.
pub fn start(dir: &Path) -> io::Result<Start> {
    std::fs::create_dir_all(dir)?;
    let lock = std::fs::File::options().create(true).truncate(false).write(true).open(dir.join("quake.lock"))?;
    if lock.try_lock().is_ok() {
        return Ok(Start::Owner(Toggle { listener: bind(dir)?, _lock: lock }));
    }
    // It may still be starting and not listen yet.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match connect(dir) {
            Ok(mut stream) => {
                // Windows gives the foreground only to the process the
                // user just started (this one): pass that on, or the
                // drop-down window would only flash in the taskbar.
                #[cfg(windows)]
                // SAFETY: no pointers; only widens who may take the foreground.
                unsafe {
                    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
                    AllowSetForegroundWindow(ASFW_ANY);
                }
                stream.write_all(TOGGLE)?;
                return Ok(Start::Toggled);
            }
            Err(err) if Instant::now() < deadline => {
                log::debug!("drop-down Terminaal not listening yet: {err}");
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(unix)]
type Listener = UnixListener;
#[cfg(windows)]
type Listener = TcpListener;

/// The socket in `dir`; whatever socket is left there belongs to one
/// that's gone.
#[cfg(unix)]
fn bind(dir: &Path) -> io::Result<Listener> {
    let socket = dir.join("quake.sock");
    let _ = std::fs::remove_file(&socket);
    UnixListener::bind(&socket)
}

#[cfg(unix)]
fn connect(dir: &Path) -> io::Result<UnixStream> {
    UnixStream::connect(dir.join("quake.sock"))
}

/// No Unix sockets in std on Windows: a loopback port, named in
/// `quake.port`. Anyone on this machine could toggle the window -- and
/// only that.
#[cfg(windows)]
fn bind(dir: &Path) -> io::Result<Listener> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    std::fs::write(dir.join("quake.port"), listener.local_addr()?.port().to_string())?;
    Ok(listener)
}

#[cfg(windows)]
fn connect(dir: &Path) -> io::Result<TcpStream> {
    let port = std::fs::read_to_string(dir.join("quake.port"))?;
    let port: u16 = port.trim().parse().map_err(io::Error::other)?;
    TcpStream::connect((Ipv4Addr::LOCALHOST, port))
}

/// The drop-down Terminaal's end: holds the lock and the socket.
pub struct Toggle {
    listener: Listener,
    _lock: std::fs::File,
}

impl Toggle {
    /// Call `toggled` for every `--quake` from now on, from a thread.
    pub fn listen(self, toggled: impl Fn() + Send + 'static) {
        let spawned = std::thread::Builder::new().name("quake toggle".into()).spawn(move || {
            let _lock = self._lock;
            for stream in self.listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let mut buf = [0; TOGGLE.len()];
                if stream.read_exact(&mut buf).is_ok() && buf == TOGGLE {
                    toggled();
                }
            }
        });
        if let Err(err) = spawned {
            log::error!("failed to listen for --quake: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn the_second_start_toggles_the_first() {
        let dir = std::env::temp_dir().join(format!("terminaal-quake-{}", std::process::id()));
        let Start::Owner(toggle) = start(&dir).unwrap() else { panic!("first start owns it") };
        let (tx, rx) = mpsc::channel();
        toggle.listen(move || tx.send(()).unwrap());
        for _ in 0..2 {
            assert!(matches!(start(&dir).unwrap(), Start::Toggled));
            rx.recv_timeout(Duration::from_secs(2)).expect("toggled");
        }
        // Garbage on the socket toggles nothing.
        connect(&dir).unwrap().write_all(b"rm -rf\n").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        // Windows keeps the lock file while the listening thread holds it.
        let _ = std::fs::remove_dir_all(&dir);
    }
}
