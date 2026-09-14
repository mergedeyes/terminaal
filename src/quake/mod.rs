//! The drop-down ("Quake") window: `terminaal --quake` puts a Terminaal
//! along the top edge of the screen, or takes it away again. There's no
//! global shortcut a Wayland client may grab, so the desktop's own
//! shortcut settings run that command.
//!
//! The drop-down Terminaal is an instance of its own, with its own tabs
//! and session. The first `--quake` starts it; later ones find it by its
//! socket in `$XDG_RUNTIME_DIR` and tell it to toggle ([`Toggle`]).
//!
//! On Wayland it's a layer surface ([`layer`]); elsewhere a normal window
//! that shows and hides.

pub mod keys;
pub mod layer;

use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What goes over the socket.
const TOGGLE: &[u8] = b"toggle\n";

/// `$XDG_RUNTIME_DIR/terminaal`, else a folder of this user's in the temp dir.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|dir| dir.is_absolute()) {
        Some(dir) => dir.join("terminaal"),
        // SAFETY: getuid can't fail.
        None => std::env::temp_dir().join(format!("terminaal-{}", unsafe { libc::getuid() })),
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
    let socket = dir.join("quake.sock");
    let lock = std::fs::File::options().create(true).truncate(false).write(true).open(dir.join("quake.lock"))?;
    // SAFETY: a valid descriptor, owned by `lock`.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        // Whatever socket is left there belongs to one that's gone.
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)?;
        return Ok(Start::Owner(Toggle { listener, _lock: lock }));
    }
    // It may still be starting and not listen yet.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match UnixStream::connect(&socket) {
            Ok(mut stream) => {
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

/// The drop-down Terminaal's end: holds the lock and the socket.
pub struct Toggle {
    listener: UnixListener,
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
        UnixStream::connect(dir.join("quake.sock")).unwrap().write_all(b"rm -rf\n").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
