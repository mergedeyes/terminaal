use std::ffi::OsStr;
use std::fs::{DirBuilder, File, Metadata, OpenOptions, Permissions};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{DirBuilderExt, FileExt, OpenOptionsExt, PermissionsExt};
pub use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Duration;

/// One end of a connected local byte stream, from [`pair`].
pub type LocalStream = UnixStream;

/// What [`poll`] waits on.
pub type Handle = RawFd;
/// No socket at all; never ready, never hung up.
pub const NO_HANDLE: Handle = -1;

/// The `events`/`revents` bits of [`poll`].
pub type Events = libc::c_short;
pub const POLLIN: Events = libc::POLLIN;
pub const POLLOUT: Events = libc::POLLOUT;
pub const POLLHUP: Events = libc::POLLHUP;

/// Whether `path` is a Unix socket (not following a symlink).
pub fn is_socket_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket())
}

/// What tells this file apart from one that replaced it at the same
/// path: the inode (not following a symlink).
pub fn file_id(path: &Path) -> io::Result<u64> {
    std::fs::symlink_metadata(path).map(|meta| meta.ino())
}

/// For tests: Unix always has Unix sockets.
#[cfg(test)]
pub fn unix_sockets_supported() -> bool {
    true
}

/// A socketpair.
pub fn pair() -> io::Result<(LocalStream, LocalStream)> {
    UnixStream::pair()
}

/// Anything [`poll`] can wait on.
pub trait Pollable {
    fn handle(&self) -> Handle;
}

impl<T: AsRawFd> Pollable for T {
    fn handle(&self) -> Handle {
        self.as_raw_fd()
    }
}

/// Block until one of `fds` is ready or `timeout` passes (`None`: no
/// limit). Returns each fd's `revents`, all 0 on timeout or EINTR.
pub fn wait(fds: &[(Handle, Events)], timeout: Option<Duration>) -> io::Result<Vec<Events>> {
    let mut pollfds: Vec<libc::pollfd> =
        fds.iter().map(|&(fd, events)| libc::pollfd { fd, events, revents: 0 }).collect();
    // Round up, so a sub-millisecond deadline doesn't turn into a busy loop.
    let millis = timeout.map_or(-1, |timeout| {
        timeout.as_micros().div_ceil(1000).min(libc::c_int::MAX as u128) as libc::c_int
    });
    // SAFETY: `pollfds` is a valid, exclusively borrowed array of
    // `pollfds.len()` entries for the duration of the call.
    let ready = unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, millis) };
    if ready < 0 {
        let err = io::Error::last_os_error();
        return if err.kind() == io::ErrorKind::Interrupted { Ok(vec![0; fds.len()]) } else { Err(err) };
    }
    Ok(pollfds.iter().map(|p| p.revents).collect())
}

/// [`wait`] for at most `timeout`; an error counts as a timeout -- the
/// caller just loops.
pub fn poll(fds: &[(Handle, Events)], timeout: Duration) -> Vec<Events> {
    wait(fds, Some(timeout)).unwrap_or_else(|_| vec![0; fds.len()])
}

/// Set the permission bits of `path`.
pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    std::fs::set_permissions(path, Permissions::from_mode(mode))
}

/// Set the permission bits of an open file.
pub fn set_file_mode(file: &File, mode: u32) -> io::Result<()> {
    file.set_permissions(Permissions::from_mode(mode))
}

/// The permission bits of a file.
pub fn mode(meta: &Metadata) -> u32 {
    meta.permissions().mode()
}

/// How a process ended with exit code `code`.
pub fn exit_status(code: i32) -> ExitStatus {
    ExitStatus::from_raw(code << 8)
}

/// Whether the other end of a socket hung up; doesn't wait.
pub fn hung_up(handle: Handle) -> bool {
    let mut poll = libc::pollfd { fd: handle, events: libc::POLLIN | libc::POLLRDHUP, revents: 0 };
    // SAFETY: one valid pollfd for the duration of the call; no wait.
    let ready = unsafe { libc::poll(&mut poll, 1, 0) };
    ready > 0 && poll.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLRDHUP | libc::POLLNVAL) != 0
}

/// Read from `offset` without touching the file's position.
pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    file.read_at(buf, offset)
}

/// Write all of `data` at `offset` without touching the file's position.
pub fn write_all_at(file: &File, data: &[u8], offset: u64) -> io::Result<()> {
    file.write_all_at(data, offset)
}

/// Whether this process runs as root.
pub fn is_root() -> bool {
    // SAFETY: geteuid only reads the process's own id and can't fail.
    unsafe { libc::geteuid() == 0 }
}

/// This machine's host name.
pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is valid for `buf.len()` bytes; gethostname writes at
    // most that many, NUL-terminated unless truncated.
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return String::new();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

/// The user's home folder: `$HOME`.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Terminaal's settings, hosts, keys, snippets, themes:
/// `~/.config/terminaal`.
pub fn config_dir() -> Option<PathBuf> {
    Some(home_dir()?.join(".config/terminaal"))
}

/// What Terminaal keeps between runs (session, sync records):
/// `$XDG_STATE_HOME/terminaal`, else `~/.local/state/terminaal`.
pub fn state_dir() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|dir| dir.is_absolute());
    let state = state.or_else(|| Some(home_dir()?.join(".local/state")))?;
    Some(state.join("terminaal"))
}

/// Nothing to do: stdout and stderr are the terminal's anyway.
pub fn attach_console() {}

/// Whether there's an SSH agent to ask: `$SSH_AUTH_SOCK` is set.
pub fn agent_available() -> bool {
    std::env::var_os("SSH_AUTH_SOCK").is_some()
}

/// A folder of this user's own in the temp dir (when there's no
/// `$XDG_RUNTIME_DIR`).
pub fn user_temp_dir() -> PathBuf {
    // SAFETY: getuid can't fail.
    std::env::temp_dir().join(format!("terminaal-{}", unsafe { libc::getuid() }))
}

/// `dir` and its missing parents, new ones 0700.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

/// A new file only this user can read (0600); fails if it exists.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}

/// Whether opening the file could run it.
pub fn is_executable(_path: &Path, meta: &Metadata) -> bool {
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

/// The desktop's default application for a file or URL.
pub fn open_command(target: &OsStr) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(target);
    command
}

/// `editor` (a shell command line) with `path` appended.
pub fn editor_command(editor: &str, path: &Path) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(format!("exec {editor} \"$1\"")).arg("sh").arg(path);
    command
}

/// Local time of a Unix timestamp: year, month, day, hour, minute.
pub fn local_time(secs: u64) -> Option<(i32, u32, u32, u32, u32)> {
    let time = secs as libc::time_t;
    // SAFETY: a plain C struct; all zeros is a valid value.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the call.
    if unsafe { libc::localtime_r(&time, &mut tm) }.is_null() {
        return None;
    }
    Some((tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32, tm.tm_hour as u32, tm.tm_min as u32))
}

/// Why [`trash`] didn't.
pub enum TrashError {
    /// There's nothing to trash with (`gio`).
    NoTool,
    Failed(String),
}

/// Into the desktop's trash, with `gio trash` (GLib's, on nearly every
/// desktop) -- it knows the trash of other disks too.
pub fn trash(paths: &[PathBuf]) -> Result<(), TrashError> {
    match Command::new("gio").arg("trash").arg("--").args(paths).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(TrashError::Failed(String::from_utf8_lossy(&output.stderr).trim().to_string())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(TrashError::NoTool),
        Err(err) => Err(TrashError::Failed(err.to_string())),
    }
}

/// A desktop notification, through `notify-send`.
pub fn notify_command(title: String, body: String) -> Option<Command> {
    let mut command = Command::new("notify-send");
    command.args(["--app-name=Terminaal", "--icon=terminaal", "--"]).args([title, body]);
    Some(command)
}
