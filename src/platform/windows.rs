use std::ffi::OsStr;
use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::FileExt;
use std::os::windows::io::{AsRawSocket, RawSocket};
use std::os::windows::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Duration;

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Networking::WinSock::{self, WSAPOLLFD, WSAPoll};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW, SHFileOperationW,
};

#[path = "process_windows.rs"]
mod process;
pub use process::running_program;
#[path = "unix_socket_windows.rs"]
mod unix_socket;
pub use unix_socket::{UnixListener, UnixStream};
#[cfg(test)]
pub use unix_socket::supported as unix_sockets_supported;

/// For helper programs (`cmd`, `powershell`) started from a window
/// program: without it each one opens a console window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One end of a connected local byte stream, from [`pair`]. Windows has
/// no socketpair; a loopback TCP connection is what `WSAPoll` and
/// libssh2 (`set_tcp_stream` wants a socket) both accept.
pub type LocalStream = TcpStream;

/// What [`poll`] waits on.
pub type Handle = RawSocket;
/// No socket at all (`INVALID_SOCKET`); never hung up.
pub const NO_HANDLE: Handle = RawSocket::MAX;

/// The `events`/`revents` bits of [`poll`].
pub type Events = i16;
pub const POLLIN: Events = WinSock::POLLRDNORM;
pub const POLLOUT: Events = WinSock::POLLWRNORM;
pub const POLLHUP: Events = WinSock::POLLHUP;

/// Whether `path` is a Unix socket: a reparse point tagged as one
/// (`IO_REPARSE_TAG_AF_UNIX`), which only the directory listing tells.
pub fn is_socket_file(path: &Path) -> bool {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FindClose, FindFirstFileW, WIN32_FIND_DATAW};
    const IO_REPARSE_TAG_AF_UNIX: u32 = 0x8000_0023;
    let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a NUL-terminated name and a struct to fill; the handle is closed.
    let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let find = unsafe { FindFirstFileW(name.as_ptr(), &mut data) };
    if find == INVALID_HANDLE_VALUE {
        return false;
    }
    unsafe { FindClose(find) };
    data.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 && data.dwReserved0 == IO_REPARSE_TAG_AF_UNIX
}

/// What tells the file at `path` (not following a link or socket) apart
/// from one that replaced it: its file index, the inode of NTFS.
pub fn file_id(path: &Path) -> io::Result<u64> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
    };
    let file = OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    // SAFETY: an open handle and a struct to fill.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow))
}

/// Two connected loopback sockets. Another local program could connect
/// to the listener in the moment it's open; only our own end is taken.
pub fn pair() -> io::Result<(LocalStream, LocalStream)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let ours = TcpStream::connect(listener.local_addr()?)?;
    let expected = ours.local_addr()?;
    loop {
        let (theirs, peer) = listener.accept()?;
        if peer == expected {
            // Both carry small interactive writes (keys, wakeups).
            ours.set_nodelay(true)?;
            theirs.set_nodelay(true)?;
            return Ok((ours, theirs));
        }
    }
}

/// Anything [`poll`] can wait on.
pub trait Pollable {
    fn handle(&self) -> Handle;
}

impl<T: AsRawSocket> Pollable for T {
    fn handle(&self) -> Handle {
        self.as_raw_socket()
    }
}

/// Block until one of `fds` is ready or `timeout` passes (`None`: no
/// limit). Returns each socket's `revents`, all 0 on timeout.
pub fn wait(fds: &[(Handle, Events)], timeout: Option<Duration>) -> io::Result<Vec<Events>> {
    let mut pollfds: Vec<WSAPOLLFD> = fds
        .iter()
        // WSAPoll rejects the whole call (WSAEINVAL) for any requested
        // bit besides reading and writing; hangups are reported anyway.
        .map(|&(fd, events)| WSAPOLLFD { fd: fd as usize, events: events & (POLLIN | POLLOUT), revents: 0 })
        .collect();
    // Round up, so a sub-millisecond deadline doesn't turn into a busy loop.
    let millis = timeout.map_or(-1, |timeout| timeout.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32);
    // SAFETY: `pollfds` is a valid, exclusively borrowed array of
    // `pollfds.len()` entries for the duration of the call.
    let ready = unsafe { WSAPoll(pollfds.as_mut_ptr(), pollfds.len() as u32, millis) };
    if ready < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pollfds.iter().map(|p| p.revents).collect())
}

/// [`wait`] for at most `timeout`; an error counts as a timeout -- the
/// caller just loops.
pub fn poll(fds: &[(Handle, Events)], timeout: Duration) -> Vec<Events> {
    wait(fds, Some(timeout)).unwrap_or_else(|_| vec![0; fds.len()])
}

/// Windows has no permission bits; what's in the user's profile is the
/// user's alone by its ACL already. Only "read-only" maps over: a mode
/// without any write bit.
pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(mode & 0o222 == 0);
    std::fs::set_permissions(path, permissions)
}

/// [`set_mode`] for an open file.
pub fn set_file_mode(file: &File, mode: u32) -> io::Result<()> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_readonly(mode & 0o222 == 0);
    file.set_permissions(permissions)
}

/// Permission bits as a Unix server would expect them: read-only or not,
/// executable for folders.
pub fn mode(meta: &Metadata) -> u32 {
    let write = if meta.permissions().readonly() { 0 } else { 0o200 };
    if meta.is_dir() { 0o555 | write } else { 0o444 | write }
}

/// How a process ended with exit code `code`.
pub fn exit_status(code: i32) -> ExitStatus {
    ExitStatus::from_raw(code as u32)
}

/// Whether the other end of a socket hung up; doesn't wait.
pub fn hung_up(handle: Handle) -> bool {
    let mut poll = WSAPOLLFD { fd: handle as usize, events: POLLIN, revents: 0 };
    // SAFETY: one valid WSAPOLLFD for the duration of the call; no wait.
    let ready = unsafe { WSAPoll(&mut poll, 1, 0) };
    ready > 0 && poll.revents & (WinSock::POLLHUP | WinSock::POLLERR | WinSock::POLLNVAL) != 0
}

/// Read from `offset`. Moves the file's position on Windows; callers
/// only ever read by offset.
pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    file.seek_read(buf, offset)
}

/// Write all of `data` at `offset`. Moves the file's position, like
/// [`read_at`].
pub fn write_all_at(file: &File, mut data: &[u8], mut offset: u64) -> io::Result<()> {
    while !data.is_empty() {
        match file.seek_write(data, offset) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => {
                data = &data[n..];
                offset += n as u64;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// The user's Documents folder, wherever it is (often OneDrive's).
pub fn documents_dir() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath};
    let mut path = std::ptr::null_mut();
    // SAFETY: a valid GUID and out-pointer; the string is ours to free.
    let result = unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, 0, std::ptr::null_mut(), &mut path) };
    let dir = (result == 0 && !path.is_null()).then(|| {
        // SAFETY: a NUL-terminated wide string from the call above.
        let len = (0..).take_while(|&i| unsafe { *path.add(i) } != 0).count();
        PathBuf::from(std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(path, len) }))
    });
    // SAFETY: allocated by SHGetKnownFolderPath, even when it failed.
    unsafe { CoTaskMemFree(path.cast()) };
    dir
}

/// Never root: an elevated process isn't what `sudo`-less commands mean.
pub fn is_root() -> bool {
    false
}

/// This machine's host name.
pub fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

/// The user's profile folder (`%USERPROFILE%`); `HOME` is rarely set on
/// Windows. Windows' OpenSSH keeps `.ssh` there too.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").filter(|dir| !dir.is_empty()).map(PathBuf::from)
}

/// Terminaal's settings, hosts, keys, snippets, themes:
/// `%APPDATA%\terminaal`, roaming with the profile.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").filter(|dir| !dir.is_empty()).map(|dir| PathBuf::from(dir).join("terminaal"))
}

/// What Terminaal keeps between runs on this machine (session, sync
/// records): `%LOCALAPPDATA%\terminaal`.
pub fn state_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").filter(|dir| !dir.is_empty()).map(|dir| PathBuf::from(dir).join("terminaal"))
}

/// Terminaal is a window program on Windows (no console of its own);
/// started from one, `--help`, `--version`, errors and `RUST_LOG` output
/// go to that console.
pub fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: no pointers; fails harmlessly without a parent console.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Always worth asking: without `SSH_AUTH_SOCK` libssh2 looks for
/// Pageant and Windows' OpenSSH agent (`\\.\pipe\openssh-ssh-agent`)
/// itself.
pub fn agent_available() -> bool {
    true
}

/// A folder of Terminaal's in the temp dir, which is the user's own on
/// Windows (`%LOCALAPPDATA%\Temp`).
pub fn user_temp_dir() -> PathBuf {
    std::env::temp_dir().join("terminaal")
}

/// `dir` and its missing parents. Private by the profile's ACL.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// A new file; fails if it exists. Private by the profile's ACL.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// Extensions Windows runs (or hands to an interpreter) when a file is
/// opened, rather than showing it.
const RUNNABLE: &[&str] = &[
    "appref-ms", "bat", "cmd", "com", "cpl", "exe", "hta", "inf", "jar", "js", "jse", "lnk", "msc", "msi", "msp",
    "pif", "ps1", "psm1", "reg", "scr", "url", "vb", "vbe", "vbs", "ws", "wsc", "wsf", "wsh",
];

/// Whether opening the file could run it: by its extension.
pub fn is_executable(path: &Path, meta: &Metadata) -> bool {
    meta.is_file()
        && path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| RUNNABLE.iter().any(|runnable| ext.eq_ignore_ascii_case(runnable)))
}

/// The default application for a file or URL, through Explorer -- not
/// `cmd /C start`, which would read `&` and `^` in the target.
pub fn open_command(target: &OsStr) -> Command {
    let mut command = Command::new("explorer.exe");
    command.arg(target);
    command
}

/// `editor` (a command line) with `path` appended, through `cmd` so that
/// `code` finds `code.cmd`. Paths can't hold `"`; `%` would still expand.
pub fn editor_command(editor: &str, path: &Path) -> Command {
    let mut command = Command::new("cmd");
    command.arg("/C").raw_arg(format!("{editor} \"{}\"", path.display())).creation_flags(CREATE_NO_WINDOW);
    command
}

/// Local time of a Unix timestamp: year, month, day, hour, minute.
pub fn local_time(secs: u64) -> Option<(i32, u32, u32, u32, u32)> {
    // FILETIME counts 100 ns from 1601.
    let ticks = secs.checked_add(11_644_473_600)?.checked_mul(10_000_000)?;
    let file_time = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let (mut utc, mut local): (SYSTEMTIME, SYSTEMTIME) = Default::default();
    // SAFETY: valid pointers for the calls; a null zone is the current one.
    let ok = unsafe {
        FileTimeToSystemTime(&file_time, &mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0
    };
    ok.then(|| {
        (local.wYear.into(), local.wMonth.into(), local.wDay.into(), local.wHour.into(), local.wMinute.into())
    })
}

/// Why [`trash`] didn't.
pub enum TrashError {
    /// Never on Windows; there for `unix.rs`'s `gio`.
    #[allow(dead_code)]
    NoTool,
    Failed(String),
}

/// Into the Recycle Bin, with the shell's own file operation.
pub fn trash(paths: &[PathBuf]) -> Result<(), TrashError> {
    // A list of NUL-terminated paths, ended by another NUL.
    let mut from: Vec<u16> = Vec::new();
    for path in paths {
        from.extend(path.as_os_str().encode_wide());
        from.push(0);
    }
    from.push(0);
    let mut operation = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    // SAFETY: `operation` and the list it points to live through the call.
    match unsafe { SHFileOperationW(&mut operation) } {
        0 if operation.fAnyOperationsAborted == 0 => Ok(()),
        // Not a Win32 error code but SHFileOperation's own (0x7C: a path
        // that doesn't exist, say).
        code => Err(TrashError::Failed(format!("error 0x{code:X}"))),
    }
}

/// Windows PowerShell's app ID, which toasts go out under when Terminaal
/// has none of its own.
const POWERSHELL_APP_ID: &str = r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// Set once [`use_app_id`] gave this process Terminaal's own app ID.
static OWN_APP_ID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Gives this process `id` as its AppUserModelID, before any window is
/// made: the taskbar then groups its windows with the Start menu shortcut
/// that carries the same ID (the setup's), and toasts go out under it --
/// named and drawn as Terminaal rather than Windows PowerShell. Only for a
/// copy the setup installed: without that shortcut Windows doesn't know
/// the ID, and toasts under it wouldn't show at all.
pub fn use_app_id(id: &str) {
    let wide: Vec<u16> = OsStr::new(id).encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and lives through the call.
    let result = unsafe { windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide.as_ptr()) };
    if result < 0 {
        log::warn!("can't set the AppUserModelID: 0x{result:08X}");
        return;
    }
    OWN_APP_ID.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Where Terminaal's setup installed it, from its entry under Settings →
/// Apps (`…\Uninstall\<id>_is1`, written by Inno Setup): per user first,
/// then for all users. Each with whether it's the all-users one.
pub fn setup_folders(app_id: &str) -> Vec<(PathBuf, bool)> {
    use windows_sys::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    let wide = |text: &str| -> Vec<u16> { OsStr::new(text).encode_wide().chain(Some(0)).collect() };
    let key = wide(&format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_id}_is1"));
    let value = wide("InstallLocation");
    let read = |root: HKEY| -> Option<PathBuf> {
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: key and value names are NUL-terminated, `buf` holds
        // `size` bytes; REG_SZ comes back NUL-terminated.
        let status = unsafe {
            RegGetValueW(root, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
        };
        if status != 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        use std::os::windows::ffi::OsStringExt;
        let path = std::ffi::OsString::from_wide(&buf[..len]);
        (!path.is_empty()).then(|| PathBuf::from(path))
    };
    [(HKEY_CURRENT_USER, false), (HKEY_LOCAL_MACHINE, true)]
        .into_iter()
        .filter_map(|(root, all_users)| Some((read(root)?, all_users)))
        .collect()
}

/// Shows the toast in [`notify_command`]: Windows PowerShell's WinRT
/// projection, under Terminaal's app ID ([`use_app_id`]) or else
/// PowerShell's own. Title, text and app ID come in by environment, never
/// as code; `Escape` makes them XML.
const TOAST: &str = r#"$notifications = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
$xml = [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime]::new()
$title = [Security.SecurityElement]::Escape($env:TERMINAAL_TITLE)
$body = [Security.SecurityElement]::Escape($env:TERMINAAL_BODY)
$xml.LoadXml("<toast><visual><binding template='ToastGeneric'><text>$title</text><text>$body</text></binding></visual></toast>")
$notifications::CreateToastNotifier($env:TERMINAAL_APP_ID).Show([Windows.UI.Notifications.ToastNotification]::new($xml))
"#;

/// A desktop notification: a toast, through a hidden Windows PowerShell
/// (pwsh has no WinRT types).
pub fn notify_command(title: String, body: String) -> Option<Command> {
    use base64::Engine;
    let utf16: Vec<u8> = TOAST.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
        .arg(base64::engine::general_purpose::STANDARD.encode(utf16))
        .env("TERMINAAL_TITLE", title)
        .env("TERMINAAL_BODY", body)
        .env("TERMINAAL_APP_ID", if OWN_APP_ID.load(std::sync::atomic::Ordering::Relaxed) { crate::update::APP_USER_MODEL_ID } else { POWERSHELL_APP_ID })
        .creation_flags(CREATE_NO_WINDOW);
    Some(command)
}
