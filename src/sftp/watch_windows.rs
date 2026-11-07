//! The Windows counterpart of `edit.rs`'s inotify [`Watcher`]: a
//! `ReadDirectoryChangesW` per folder, all on one completion port that a
//! thread of its own waits on -- one thread however many folders, as
//! with inotify.
//!
//! What the folders are watched for (`mask`, inotify's) doesn't carry
//! over: Windows reports any name or content change. More events than
//! asked for do no harm -- an edit is only looked at again once it's
//! quiet, a sync just runs.
//!
//! A pending read writes into its watch's buffer and `OVERLAPPED` until
//! its completion comes, also after cancelling. So a watch is only freed
//! by the thread, on that completion; every watch in the map has a read
//! pending.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows_sys::Win32::System::IO::{
    CancelIoEx, CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED, PostQueuedCompletionStatus,
};

/// The completion key that ends the thread; watches count from 1.
const STOP: usize = 0;
/// How long the thread waits for cancelled reads to come back when it
/// ends; whatever doesn't is leaked rather than freed under the kernel.
const DRAIN_MS: u32 = 1000;
const INFINITE: u32 = u32::MAX;

pub(crate) struct Watcher {
    shared: Arc<Shared>,
}

struct Shared {
    port: HANDLE,
    watches: Mutex<Watches>,
}

// SAFETY: a completion port handle may be used from any thread.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Drop for Shared {
    fn drop(&mut self) {
        // SAFETY: our handle, closed once.
        unsafe { CloseHandle(self.port) };
    }
}

#[derive(Default)]
struct Watches {
    next: usize,
    by_key: HashMap<usize, Box<Watch>>,
}

/// The buffer `ReadDirectoryChangesW` fills: DWORD-aligned records.
#[repr(C, align(8))]
struct Buffer([u8; 16 * 1024]);

struct Watch {
    dir: PathBuf,
    handle: HANDLE,
    overlapped: OVERLAPPED,
    buffer: Buffer,
    /// Cancelled; freed when its read comes back.
    closing: bool,
}

// SAFETY: the handle is only used under the `Watches` lock.
unsafe impl Send for Watch {}

impl Watch {
    /// Start the next read; `false` if that failed (the folder is gone).
    fn read(&mut self) -> bool {
        let filter = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME
            | FILE_NOTIFY_CHANGE_LAST_WRITE
            | FILE_NOTIFY_CHANGE_SIZE;
        self.overlapped = OVERLAPPED::default();
        // SAFETY: buffer and OVERLAPPED live in the boxed watch, which is
        // only freed once this read has completed (module docs).
        unsafe {
            ReadDirectoryChangesW(
                self.handle,
                self.buffer.0.as_mut_ptr().cast(),
                self.buffer.0.len() as u32,
                0,
                filter,
                std::ptr::null_mut(),
                &mut self.overlapped,
                None,
            ) != 0
        }
    }

    /// The changed paths in the first `len` bytes of the buffer; a
    /// buffer that overflowed (`len` 0) names only the folder.
    fn changes(&self, len: usize) -> Vec<PathBuf> {
        if len == 0 {
            return vec![self.dir.clone()];
        }
        names(&self.buffer.0[..len.min(self.buffer.0.len())]).into_iter().map(|name| self.dir.join(name)).collect()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // SAFETY: our handle, closed once; no read is pending any more.
        unsafe { CloseHandle(self.handle) };
    }
}

impl Watcher {
    pub(crate) fn new(changed: impl Fn(PathBuf) -> bool + Send + 'static) -> io::Result<Self> {
        Self::with("edit-watch", 0, changed)
    }

    /// A watcher thread called `name`. `mask` is inotify's; see the
    /// module docs.
    pub(crate) fn with(name: &str, _mask: u32, changed: impl Fn(PathBuf) -> bool + Send + 'static) -> io::Result<Self> {
        // SAFETY: a new port, no file yet.
        let port = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1) };
        if port.is_null() {
            return Err(io::Error::last_os_error());
        }
        let shared = Arc::new(Shared { port, watches: Mutex::new(Watches { next: 1, ..Watches::default() }) });
        let thread = shared.clone();
        std::thread::Builder::new().name(name.into()).spawn(move || run(&thread, changed))?;
        Ok(Self { shared })
    }

    pub(crate) fn add(&self, dir: &Path) -> io::Result<()> {
        let mut watches = self.shared.lock();
        if watches.by_key.values().any(|watch| watch.dir == dir && !watch.closing) {
            return Ok(());
        }
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: a NUL-terminated path; the handle is owned by the watch.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let key = watches.next;
        let mut watch = Box::new(Watch {
            dir: dir.to_path_buf(),
            handle,
            overlapped: OVERLAPPED::default(),
            buffer: Buffer([0; 16 * 1024]),
            closing: false,
        });
        // SAFETY: both handles are valid; the port outlives the folder's.
        if unsafe { CreateIoCompletionPort(handle, self.shared.port, key, 0) }.is_null() || !watch.read() {
            return Err(io::Error::last_os_error());
        }
        watches.next += 1;
        watches.by_key.insert(key, watch);
        Ok(())
    }

    pub(crate) fn remove(&self, dir: &Path) {
        for watch in self.shared.lock().by_key.values_mut().filter(|watch| watch.dir == dir && !watch.closing) {
            watch.closing = true;
            // SAFETY: a valid handle; its read comes back aborted.
            unsafe { CancelIoEx(watch.handle, std::ptr::null()) };
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // SAFETY: a valid port; no OVERLAPPED.
        unsafe { PostQueuedCompletionStatus(self.shared.port, 0, STOP, std::ptr::null()) };
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Watches> {
        self.watches.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The next completion: its key, bytes, and whether the read went
    /// through. `None` when nothing came within `ms`.
    fn next(&self, ms: u32) -> Option<(usize, usize, bool)> {
        let (mut bytes, mut key, mut overlapped) = (0u32, 0usize, std::ptr::null_mut());
        // SAFETY: valid out-pointers for the call.
        let ok = unsafe { GetQueuedCompletionStatus(self.port, &mut bytes, &mut key, &mut overlapped, ms) } != 0;
        // No OVERLAPPED and failed: nothing was dequeued.
        if !ok && overlapped.is_null() && key != STOP {
            return None;
        }
        Some((key, bytes as usize, ok))
    }
}

fn run(shared: &Shared, changed: impl Fn(PathBuf) -> bool) {
    while let Some((key, bytes, ok)) = shared.next(INFINITE) {
        if key == STOP {
            break;
        }
        let changes = {
            let mut watches = shared.lock();
            let Some(watch) = watches.by_key.get_mut(&key) else { continue };
            // Cancelled, or the folder went away.
            if watch.closing || !ok {
                watches.by_key.remove(&key);
                continue;
            }
            let changes = watch.changes(bytes);
            if !watch.read() {
                watches.by_key.remove(&key);
            }
            changes
        };
        if !changes.into_iter().all(&changed) {
            break;
        }
    }
    shut_down(shared);
}

/// Cancel every read and wait for them to come back before the watches
/// are freed.
fn shut_down(shared: &Shared) {
    for watch in shared.lock().by_key.values_mut() {
        watch.closing = true;
        // SAFETY: a valid handle.
        unsafe { CancelIoEx(watch.handle, std::ptr::null()) };
    }
    while !shared.lock().by_key.is_empty() {
        match shared.next(DRAIN_MS) {
            Some((STOP, ..)) => {}
            Some((key, ..)) => drop(shared.lock().by_key.remove(&key)),
            None => {
                log::warn!("folder watches didn't stop; leaving them");
                shared.lock().by_key.drain().for_each(|(_, watch)| std::mem::forget(watch));
            }
        }
    }
}

/// The file names in `FILE_NOTIFY_INFORMATION` records: offset of the
/// next record, action, name length in bytes, then the UTF-16 name.
fn names(buffer: &[u8]) -> Vec<OsString> {
    let field = |at: usize| buffer.get(at..at + 4).map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize);
    let mut names = Vec::new();
    let mut offset = 0;
    while let (Some(next), Some(len)) = (field(offset), field(offset + 8)) {
        let Some(name) = buffer.get(offset + 12..offset + 12 + len) else { break };
        let wide: Vec<u16> = name.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
        names.push(OsString::from_wide(&wide));
        if next == 0 {
            break;
        }
        offset += next;
    }
    names
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn records_are_read_with_their_names() {
        let record = |next: u32, name: &str| {
            let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let mut bytes = [next.to_le_bytes(), 1u32.to_le_bytes(), (wide.len() as u32).to_le_bytes()].concat();
            bytes.extend(wide);
            bytes.resize(next.max(bytes.len() as u32) as usize, 0);
            bytes
        };
        let buffer = [record(32, "ä.txt"), record(0, "neu")].concat();
        assert_eq!(names(&buffer), [OsString::from("ä.txt"), OsString::from("neu")]);
        assert_eq!(names(&buffer[..10]), Vec::<OsString>::new(), "a cut record is dropped");
    }

    #[test]
    fn writes_and_new_files_are_reported() {
        let dir = std::env::temp_dir().join(format!("terminaal-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = mpsc::channel();
        let watcher = Watcher::new(move |path| tx.send(path).is_ok()).unwrap();
        watcher.add(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        let path = rx.recv_timeout(Duration::from_secs(5)).expect("a change");
        assert_eq!(path, dir.join("a.txt"));
        watcher.remove(&dir);
        drop(watcher);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
