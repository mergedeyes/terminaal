//! One SFTP session: a thread that owns the [`Client`] and does what the
//! files tab asks -- list a folder, create, rename, delete, transfer files
//! and folders, edit files locally ([`super::edit`]). The tab sends
//! [`Command`]s and draws the shared [`State`]; `wake` asks for a redraw
//! whenever that changed.
//!
//! The session outlives its connection. It gets SFTP streams from
//! [`Channels`] -- the terminal's SSH connection -- and checks every second
//! whether the stream is still there. Once it's gone, transfers and edits
//! wait; when the terminal is connected again (it reconnects by itself, or
//! the host is opened anew, see [`Command::Rebind`]), the session attaches
//! again and carries on: a download from its `.part` file, an upload from
//! the part already on the server, as long as the source didn't change.
//!
//! Transfers run one after another, each with many requests in flight
//! ([`protocol::PIPELINE`]), a few dozen milliseconds per round so that
//! commands in between -- opening a folder mid-download -- stay quick.
//! Neither direction overwrites anything: a name that's taken gets a
//! number (`notes (1).txt`).

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use super::edit::{EditAction, EditStatus, Edits};
use super::protocol::{self, Attrs, Client, Entry, open};
use crate::i18n::t;
use crate::ssh::connection::Opener;

/// Longest a transfer round takes before commands get a look in.
const ROUND: Duration = Duration::from_millis(40);
/// How often progress is shown while a transfer runs.
const PUBLISH_EVERY: Duration = Duration::from_millis(150);
/// How often an idle session looks whether its connection is still there.
const CHECK_EVERY: Duration = Duration::from_secs(1);
/// Longest a command on the server (a hash of a big file) may take.
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);

/// Asks the UI for a redraw.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// A byte stream speaking SFTP, and the descriptor to watch for its end.
pub struct Stream {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub fd: RawFd,
}

/// Where a session's streams come from: a terminal's SSH connection
/// ([`Opener`]), or in tests a local `sftp-server`.
pub trait Channels: Send + Sync {
    /// Which connection is up: changes with each reconnect, 0 while none.
    fn connection(&self) -> u64;
    fn open_sftp(&self) -> io::Result<Stream>;
    /// Run `command` with `input` as its stdin; its output.
    fn exec(&self, command: &str, input: &[u8]) -> io::Result<Vec<u8>>;
}

impl Channels for Opener {
    fn connection(&self) -> u64 {
        Opener::connection(self)
    }

    fn open_sftp(&self) -> io::Result<Stream> {
        let stream = Opener::open_sftp(self)?;
        let fd = stream.as_raw_fd();
        let writer = stream.try_clone()?;
        Ok(Stream { reader: Box::new(stream), writer: Box::new(writer), fd })
    }

    fn exec(&self, command: &str, input: &[u8]) -> io::Result<Vec<u8>> {
        let mut stream = Opener::exec(self, command)?;
        stream.write_all(input)?;
        stream.shutdown(std::net::Shutdown::Write)?;
        stream.set_read_timeout(Some(EXEC_TIMEOUT))?;
        let mut output = Vec::new();
        stream.read_to_end(&mut output)?;
        Ok(output)
    }
}

#[derive(Debug)]
pub enum Command {
    /// Show this folder; `None`: the current one again.
    List(Option<String>),
    /// A new folder of this name in the current one.
    Mkdir(String),
    Rename { from: String, to: String },
    /// A file, or an empty folder.
    Remove { path: String, dir: bool },
    /// This file or folder into the local folder.
    Download { remote: String, local_dir: PathBuf },
    /// This local file or folder into the remote folder.
    Upload { local: PathBuf, remote_dir: String },
    Cancel(u64),
    /// Forget the transfers that are over.
    ClearTransfers,
    /// Edit this file locally. `editor`: the command to open it with,
    /// `None` for the desktop's default application.
    Edit { path: String, editor: Option<String> },
    /// A folder is listed, anything else edited -- for a symlink, whatever
    /// it leads to.
    Open { path: String, editor: Option<String> },
    EditAction { id: u64, action: EditAction },
    /// A file changed in a folder of local copies (from the watcher).
    LocalChanged(PathBuf),
    /// Get streams from here from now on (another terminal to the same
    /// host), and try attaching right away.
    Rebind(Arc<dyn Channels>),
    /// The tab is gone: end the session. Sent on drop -- the watcher holds
    /// a sender too, so the channel alone never says so.
    Close,
}

impl std::fmt::Debug for dyn Channels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Channels")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    /// No connection to attach to (yet, or right now): transfers and edits
    /// wait for it.
    Waiting,
    Ready,
    /// There's a connection, but SFTP didn't start on it, and why.
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Download,
    Upload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferState {
    Queued,
    Running,
    Done,
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferStatus {
    pub id: u64,
    pub direction: Direction,
    /// Where it goes, as shown.
    pub name: String,
    pub done: u64,
    pub total: u64,
    pub state: TransferState,
}

impl TransferStatus {
    pub fn is_over(&self) -> bool {
        matches!(self.state, TransferState::Done | TransferState::Cancelled | TransferState::Failed(_))
    }
}

/// What the files tab shows.
#[derive(Clone, Debug)]
pub struct State {
    pub connection: Connection,
    /// Attached once at least: the folders below are from the server.
    pub attached: bool,
    /// The login's home folder.
    pub home: String,
    /// The folder on show and its entries: folders first, then by name.
    pub dir: String,
    pub entries: Vec<Entry>,
    /// Why `dir` couldn't be listed; `entries` are then the old folder's.
    pub listing_error: Option<String>,
    pub transfers: Vec<TransferStatus>,
    pub edits: Vec<EditStatus>,
    /// The outcome of the last thing asked for.
    pub report: Option<Result<String, String>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            connection: Connection::Waiting,
            attached: false,
            home: String::new(),
            dir: String::new(),
            entries: Vec::new(),
            listing_error: None,
            transfers: Vec::new(),
            edits: Vec::new(),
            report: None,
        }
    }
}

/// The app's end of a session.
pub struct Remote {
    tx: Sender<Command>,
    state: Arc<Mutex<State>>,
}

impl Remote {
    /// A session on `channels`; `label` names the host in local folder
    /// names. Local copies of edited files go under the default place.
    pub fn spawn(channels: Arc<dyn Channels>, label: &str, wake: Wake) -> io::Result<Self> {
        Self::spawn_with(channels, label, Edits::default_root(label), wake)
    }

    /// Local copies of edited files go under `edit_root`.
    pub fn spawn_with(channels: Arc<dyn Channels>, label: &str, edit_root: PathBuf, wake: Wake) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let edits = Edits::new(edit_root, tx.clone(), channels.clone());
        std::thread::Builder::new().name(format!("sftp {label}")).spawn(move || {
            let mut worker = Worker {
                channels,
                client: None,
                fd: -1,
                attached_to: 0,
                refused: 0,
                rx,
                shared,
                wake,
                state: State::default(),
                queue: VecDeque::new(),
                next_id: 1,
                last_publish: Instant::now(),
                edits,
            };
            worker.run();
        })?;
        Ok(Self { tx, state })
    }

    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Close);
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

pub type SftpClient = Client<Box<dyn Read + Send>, Box<dyn Write + Send>>;

/// A transfer waiting or running.
struct Transfer {
    status: TransferStatus,
    job: Job,
    /// Everything before this offset arrived: where to carry on after the
    /// connection dropped.
    resume: u64,
}

enum Job {
    Download {
        remote: String,
        local: PathBuf,
        /// The server file's size and time when it started: a resume only
        /// continues the same file.
        source: Option<(Option<u64>, Option<u32>)>,
        run: Option<Down>,
    },
    Upload {
        local: PathBuf,
        remote: String,
        /// The local file's size and time when it started.
        source: Option<(u64, Option<SystemTime>)>,
        /// The file on the server is ours (created by this transfer).
        created: bool,
        run: Option<Up>,
    },
}

struct Down {
    handle: Vec<u8>,
    file: File,
    /// Request id, offset, length.
    in_flight: Vec<(u32, u64, u32)>,
    next: u64,
    eof: Option<u64>,
}

struct Up {
    handle: Vec<u8>,
    file: File,
    /// Request id, offset, length.
    in_flight: Vec<(u32, u64, u64)>,
    next: u64,
}

impl Transfer {
    /// The connection went: note where to carry on, forget the handles.
    fn interrupt(&mut self) {
        let offset = match &mut self.job {
            Job::Download { run: Some(run), .. } => {
                Some(run.in_flight.iter().map(|&(_, offset, _)| offset).min().unwrap_or(run.next))
            }
            Job::Upload { run: Some(run), .. } => {
                Some(run.in_flight.iter().map(|&(_, offset, _)| offset).min().unwrap_or(run.next))
            }
            _ => None,
        };
        if let Some(offset) = offset {
            self.resume = offset;
        }
        match &mut self.job {
            Job::Download { run, .. } => *run = None,
            Job::Upload { run, .. } => *run = None,
        }
        if self.status.state == TransferState::Running {
            self.status.state = TransferState::Queued;
        }
    }
}

struct Worker {
    channels: Arc<dyn Channels>,
    client: Option<SftpClient>,
    /// The client's stream, watched for its end.
    fd: RawFd,
    /// The connection the client runs on.
    attached_to: u64,
    /// The connection SFTP last failed to start on: not tried again until
    /// there's another (or a rebind).
    refused: u64,
    rx: Receiver<Command>,
    shared: Arc<Mutex<State>>,
    wake: Wake,
    /// The thread's copy; published to `shared`.
    state: State,
    queue: VecDeque<Transfer>,
    next_id: u64,
    last_publish: Instant,
    edits: Edits,
}

/// The connection broke in the middle of something.
pub struct Fatal(pub String);

impl From<protocol::Error> for Fatal {
    fn from(err: protocol::Error) -> Self {
        Fatal(err.to_string())
    }
}

impl Worker {
    fn run(&mut self) {
        self.publish();
        loop {
            if self.client.is_none() {
                self.attach();
            } else {
                self.check_connection();
            }
            let connected = self.client.is_some();
            let busy = connected && self.queue.front().is_some();
            let wait = if busy {
                Duration::ZERO
            } else {
                let edits = if connected { self.edits.next_deadline() } else { None };
                let edits = edits.map(|at| at.saturating_duration_since(Instant::now()));
                edits.map_or(CHECK_EVERY, |edits| edits.min(CHECK_EVERY))
            };
            let command = if wait.is_zero() {
                self.rx.try_recv().map_err(|err| match err {
                    mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
                    mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
                })
            } else {
                self.rx.recv_timeout(wait)
            };
            let result = match command {
                Ok(Command::Close) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(command) => self.command(command),
                Err(RecvTimeoutError::Timeout) => Ok(()),
            };
            let result = result.and_then(|()| self.tick());
            let result = result.and_then(|()| if self.client.is_some() { self.transfer_round() } else { Ok(()) });
            if let Err(Fatal(err)) = result {
                self.lose(&err);
            }
        }
        self.edits.close_all();
        log::debug!("sftp session closed");
    }

    /// Start SFTP on the connection, if there is one now.
    fn attach(&mut self) {
        let connection = self.channels.connection();
        if connection == 0 || connection == self.refused {
            return;
        }
        let started = self.channels.open_sftp().and_then(|stream| {
            let fd = stream.fd;
            Client::new(stream.reader, stream.writer).map(|client| (client, fd)).map_err(|err| io::Error::other(err.to_string()))
        });
        match started {
            Ok((client, fd)) => {
                log::debug!("sftp attached to connection {connection}");
                self.client = Some(client);
                self.fd = fd;
                self.attached_to = connection;
                if let Err(Fatal(err)) = self.start() {
                    return self.lose(&err);
                }
                self.state.connection = Connection::Ready;
                self.state.attached = true;
                self.edits.connected();
                self.publish();
            }
            Err(err) => {
                log::info!("sftp didn't start on connection {connection}: {err}");
                self.refused = connection;
                self.state.connection = Connection::Failed(t!("files-connect-failed", err = err.to_string()));
                self.publish();
            }
        }
    }

    /// The home folder, and the folder on show again (else home).
    fn start(&mut self) -> Result<(), Fatal> {
        self.state.home = self.client()?.realpath(".")?;
        let dir = if self.state.dir.is_empty() { self.state.home.clone() } else { self.state.dir.clone() };
        self.list(&dir)?;
        if self.state.listing_error.is_some() && dir != self.state.home {
            let home = self.state.home.clone();
            self.list(&home)?;
        }
        Ok(())
    }

    /// Notice a connection that's gone although nothing was asked of it:
    /// its stream closed, or the terminal is on another connection now.
    fn check_connection(&mut self) {
        if self.channels.connection() != self.attached_to {
            return self.lose(&t!("files-reconnected"));
        }
        let mut poll = libc::pollfd { fd: self.fd, events: libc::POLLIN | libc::POLLRDHUP, revents: 0 };
        // SAFETY: one valid pollfd for the duration of the call; no wait.
        let ready = unsafe { libc::poll(&mut poll, 1, 0) };
        if ready > 0 && poll.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLRDHUP | libc::POLLNVAL) != 0 {
            self.lose(&t!("files-stream-closed"));
        }
    }

    /// The connection is gone: everything waits for the next one.
    fn lose(&mut self, reason: &str) {
        if self.client.take().is_none() {
            return;
        }
        log::info!("sftp connection lost: {reason}");
        for transfer in &mut self.queue {
            transfer.interrupt();
            if let Some(shown) = self.state.transfers.iter_mut().find(|t| t.id == transfer.status.id) {
                *shown = transfer.status.clone();
            }
        }
        self.edits.disconnected();
        self.state.edits = self.edits.statuses();
        self.state.connection = Connection::Waiting;
        self.publish();
    }

    fn client(&mut self) -> Result<&mut SftpClient, Fatal> {
        self.client.as_mut().ok_or_else(|| Fatal(t!("files-offline")))
    }

    fn tick(&mut self) -> Result<(), Fatal> {
        let Some(client) = self.client.as_mut() else { return Ok(()) };
        if self.edits.tick(client)? {
            if let Some(report) = self.edits.take_report() {
                self.state.report = Some(report);
            }
            self.state.edits = self.edits.statuses();
            self.publish();
        }
        Ok(())
    }

    fn command(&mut self, command: Command) -> Result<(), Fatal> {
        let needs_connection = !matches!(
            command,
            Command::LocalChanged(_)
                | Command::ClearTransfers
                | Command::Cancel(_)
                | Command::Rebind(_)
                | Command::Close
                | Command::EditAction { action: EditAction::Reopen | EditAction::Close, .. }
        );
        if needs_connection && self.client.is_none() {
            self.state.report = Some(Err(t!("files-offline")));
            self.publish();
            return Ok(());
        }
        let result = self.run_command(command);
        if let Some(report) = self.edits.take_report() {
            self.state.report = Some(report);
        }
        self.state.edits = self.edits.statuses();
        self.publish();
        result
    }

    fn run_command(&mut self, command: Command) -> Result<(), Fatal> {
        match command {
            Command::List(dir) => {
                let dir = dir.unwrap_or_else(|| self.state.dir.clone());
                self.list(&dir)?;
            }
            Command::Mkdir(name) => {
                let path = join(&self.state.dir, &name);
                let result = self.client()?.mkdir(&path, Attrs::mode(0o755));
                self.report(result.map(|()| t!("files-created", name = &name)))?;
                self.relist()?;
            }
            Command::Rename { from, to } => {
                let result = self.client()?.rename(&from, &to);
                self.report(result.map(|()| t!("files-renamed", name = file_name(&to))))?;
                self.relist()?;
            }
            Command::Remove { path, dir } => {
                let client = self.client()?;
                let result = if dir { client.rmdir(&path) } else { client.remove(&path) };
                self.report(result.map(|()| t!("files-removed", name = file_name(&path))))?;
                self.relist()?;
            }
            Command::Download { remote, local_dir } => self.queue_download(&remote, &local_dir)?,
            Command::Upload { local, remote_dir } => self.queue_upload(&local, &remote_dir)?,
            Command::Cancel(id) => self.cancel(id)?,
            Command::ClearTransfers => self.state.transfers.retain(|t| !t.is_over()),
            Command::Edit { path, editor } => {
                let client = self.client.as_mut().ok_or_else(|| Fatal(t!("files-offline")))?;
                self.edits.open(client, &path, editor)?;
            }
            Command::Open { path, editor } => match self.client()?.stat(&path) {
                Ok(attrs) if attrs.is_dir() => self.list(&path)?,
                Ok(_) => {
                    let client = self.client.as_mut().ok_or_else(|| Fatal(t!("files-offline")))?;
                    self.edits.open(client, &path, editor)?;
                }
                Err(err) => self.report(Err(err))?,
            },
            Command::EditAction { id, action } => match self.client.as_mut() {
                Some(client) => self.edits.action(client, id, action)?,
                None => self.edits.offline_action(id, action),
            },
            Command::LocalChanged(path) => self.edits.local_changed(&path),
            Command::Rebind(channels) => {
                self.edits.set_channels(channels.clone());
                self.channels = channels;
                self.refused = 0;
                if self.client.is_some() && self.channels.connection() != self.attached_to {
                    self.lose(&t!("files-reconnected"));
                }
            }
            Command::Close => {}
        }
        Ok(())
    }

    /// Show the outcome; only a broken connection goes further.
    fn report(&mut self, result: protocol::Result<String>) -> Result<(), Fatal> {
        self.state.report = Some(match result {
            Ok(text) => Ok(text),
            Err(err) if err.is_fatal() => return Err(err.into()),
            Err(err) => Err(err.to_string()),
        });
        Ok(())
    }

    fn list(&mut self, dir: &str) -> Result<(), Fatal> {
        let client = self.client()?;
        let listed = client.realpath(dir).and_then(|dir| Ok((client.list(&dir)?, dir)));
        match listed {
            Ok((mut entries, dir)) => {
                sort_entries(&mut entries);
                self.state.entries = entries;
                self.state.dir = dir;
                self.state.listing_error = None;
            }
            Err(err) if err.is_fatal() => return Err(err.into()),
            Err(err) => self.state.listing_error = Some(t!("files-list-failed", dir = dir, err = err.to_string())),
        }
        Ok(())
    }

    fn relist(&mut self) -> Result<(), Fatal> {
        let dir = self.state.dir.clone();
        self.list(&dir)
    }

    fn publish(&mut self) {
        *lock(&self.shared) = self.state.clone();
        self.last_publish = Instant::now();
        (self.wake)();
    }

    fn push_transfer(&mut self, direction: Direction, name: String, total: u64, job: Job) {
        let id = self.next_id;
        self.next_id += 1;
        let status = TransferStatus { id, direction, name, done: 0, total, state: TransferState::Queued };
        self.state.transfers.push(status.clone());
        self.queue.push_back(Transfer { status, job, resume: 0 });
    }

    fn download_job(remote: String, local: PathBuf) -> Job {
        Job::Download { remote, local, source: None, run: None }
    }

    fn upload_job(local: PathBuf, remote: String) -> Job {
        Job::Upload { local, remote, source: None, created: false, run: None }
    }

    /// A file, or a folder with everything in it (symlinks inside are
    /// skipped), into `local_dir` under a name that's free there.
    fn queue_download(&mut self, remote: &str, local_dir: &Path) -> Result<(), Fatal> {
        let attrs = match self.client()?.stat(remote) {
            Ok(attrs) => attrs,
            Err(err) => return self.report(Err(err)),
        };
        let local = free_local_name(local_dir, file_name(remote));
        if !attrs.is_dir() {
            let job = Self::download_job(remote.to_string(), local.clone());
            self.push_transfer(Direction::Download, display(&local), attrs.size.unwrap_or(0), job);
            return Ok(());
        }
        let mut dirs = vec![(remote.to_string(), local)];
        while let Some((remote_dir, local_dir)) = dirs.pop() {
            if let Err(err) = std::fs::create_dir_all(&local_dir) {
                self.state.report = Some(Err(t!("files-local-failed", path = display(&local_dir), err = err.to_string())));
                return Ok(());
            }
            let entries = match self.client()?.list(&remote_dir) {
                Ok(entries) => entries,
                Err(err) => return self.report(Err(err)),
            };
            for entry in entries {
                let (remote, local) = (join(&remote_dir, &entry.name), local_dir.join(&entry.name));
                if entry.attrs.is_dir() {
                    dirs.push((remote, local));
                } else if entry.attrs.is_file() {
                    let size = entry.attrs.size.unwrap_or(0);
                    self.push_transfer(Direction::Download, display(&local), size, Self::download_job(remote, local));
                }
            }
        }
        Ok(())
    }

    /// A file, or a folder with everything in it, into `remote_dir` under a
    /// name that's free there.
    fn queue_upload(&mut self, local: &Path, remote_dir: &str) -> Result<(), Fatal> {
        let meta = match std::fs::metadata(local) {
            Ok(meta) => meta,
            Err(err) => {
                self.state.report = Some(Err(t!("files-local-failed", path = display(local), err = err.to_string())));
                return Ok(());
            }
        };
        let name = local.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let remote = match self.free_remote_name(remote_dir, &name)? {
            Ok(remote) => remote,
            Err(err) => return self.report(Err(err)),
        };
        if !meta.is_dir() {
            let job = Self::upload_job(local.to_path_buf(), remote.clone());
            self.push_transfer(Direction::Upload, remote, meta.len(), job);
            return Ok(());
        }
        let mut dirs = vec![(local.to_path_buf(), remote)];
        while let Some((local_dir, remote_dir)) = dirs.pop() {
            let mode = std::fs::metadata(&local_dir).map_or(0o755, |meta| meta.permissions().mode());
            if let Err(err) = self.client()?.mkdir(&remote_dir, Attrs::mode(mode)) {
                return self.report(Err(err));
            }
            let read = match std::fs::read_dir(&local_dir) {
                Ok(read) => read,
                Err(err) => {
                    self.state.report =
                        Some(Err(t!("files-local-failed", path = display(&local_dir), err = err.to_string())));
                    return Ok(());
                }
            };
            for entry in read.flatten() {
                let Ok(kind) = entry.file_type() else { continue };
                let name = entry.file_name().to_string_lossy().into_owned();
                let (local, remote) = (entry.path(), join(&remote_dir, &name));
                if kind.is_dir() {
                    dirs.push((local, remote));
                } else if kind.is_file() {
                    let size = entry.metadata().map_or(0, |meta| meta.len());
                    self.push_transfer(Direction::Upload, remote.clone(), size, Self::upload_job(local, remote));
                }
            }
        }
        Ok(())
    }

    fn free_remote_name(&mut self, dir: &str, name: &str) -> Result<protocol::Result<String>, Fatal> {
        let client = self.client()?;
        for candidate in numbered(name) {
            let path = join(dir, &candidate);
            match client.lstat(&path) {
                Err(err) if err.is_fatal() => return Err(err.into()),
                Err(err) if err.not_found() => return Ok(Ok(path)),
                Err(err) => return Ok(Err(err)),
                Ok(_) => {}
            }
        }
        unreachable!("numbered names never run out")
    }

    fn cancel(&mut self, id: u64) -> Result<(), Fatal> {
        let Some(at) = self.queue.iter().position(|t| t.status.id == id) else { return Ok(()) };
        let mut transfer = self.queue.remove(at).expect("position exists");
        self.abort(&mut transfer)?;
        transfer.status.state = TransferState::Cancelled;
        self.update_status(&transfer.status);
        Ok(())
    }

    /// Wait out the requests in flight, close and remove what was begun --
    /// on the server only while connected.
    fn abort(&mut self, transfer: &mut Transfer) -> Result<(), Fatal> {
        match &mut transfer.job {
            Job::Download { local, run, .. } => {
                if let (Some(run), Some(client)) = (run.take(), self.client.as_mut()) {
                    for (id, ..) in run.in_flight {
                        client.reply(id)?;
                    }
                    let _ = client.close(&run.handle);
                }
                let _ = std::fs::remove_file(part_name(local));
            }
            Job::Upload { remote, run, created, .. } => {
                let Some(client) = self.client.as_mut() else { return Ok(()) };
                if let Some(run) = run.take() {
                    for (id, ..) in run.in_flight {
                        client.reply(id)?;
                    }
                    let _ = client.close(&run.handle);
                }
                if *created {
                    let _ = client.remove(remote);
                }
            }
        }
        Ok(())
    }

    fn update_status(&mut self, status: &TransferStatus) {
        if let Some(shown) = self.state.transfers.iter_mut().find(|t| t.id == status.id) {
            *shown = status.clone();
        }
    }

    /// Move the first transfer along for a round.
    fn transfer_round(&mut self) -> Result<(), Fatal> {
        let Some(mut transfer) = self.queue.pop_front() else { return Ok(()) };
        let started = Instant::now();
        let outcome = loop {
            match self.step(&mut transfer) {
                Ok(true) => break Ok(true),
                Ok(false) if started.elapsed() < ROUND => {}
                Ok(false) => break Ok(false),
                Err(err) => break Err(err),
            }
        };
        match outcome {
            Ok(false) => {
                let started = transfer.status.state == TransferState::Queued;
                transfer.status.state = TransferState::Running;
                self.update_status(&transfer.status);
                self.queue.push_front(transfer);
                if started || self.last_publish.elapsed() >= PUBLISH_EVERY {
                    self.publish();
                }
            }
            Ok(true) => {
                transfer.status.state = TransferState::Done;
                transfer.status.done = transfer.status.done.max(transfer.status.total);
                self.update_status(&transfer.status);
                if transfer.status.direction == Direction::Upload {
                    self.relist()?;
                }
                self.publish();
            }
            Err(err) if err.is_fatal() => {
                // Back in line, to carry on with the next connection.
                self.queue.push_front(transfer);
                return Err(err.into());
            }
            Err(err) => {
                self.abort(&mut transfer)?;
                transfer.status.state = TransferState::Failed(err.to_string());
                self.update_status(&transfer.status);
                self.publish();
            }
        }
        Ok(())
    }

    /// One reply's worth of progress; `true` once the transfer is done.
    fn step(&mut self, transfer: &mut Transfer) -> protocol::Result<bool> {
        let Some(client) = self.client.as_mut() else {
            return Err(protocol::Error::Io(io::Error::other(t!("files-offline"))));
        };
        let chunk = client.chunk;
        let status = &mut transfer.status;
        let resume = transfer.resume;
        match &mut transfer.job {
            Job::Download { remote, local, source, run } => {
                if run.is_none() {
                    let attrs = client.stat(remote)?;
                    let part = part_name(local);
                    let fresh = (attrs.size, attrs.mtime());
                    let have = std::fs::metadata(&part).map_or(0, |meta| meta.len());
                    // The same file as before, and the part still there.
                    let offset = if *source == Some(fresh) && have >= resume { resume } else { 0 };
                    *source = Some(fresh);
                    let handle = client.open(remote, open::READ, Attrs::default())?;
                    let file = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(&part)?;
                    file.set_len(offset)?;
                    status.total = attrs.size.unwrap_or(status.total);
                    status.done = offset;
                    *run = Some(Down { handle, file, in_flight: Vec::new(), next: offset, eof: None });
                }
                let run = run.as_mut().expect("just started");
                while run.in_flight.len() < protocol::PIPELINE && run.eof.is_none() && run.next <= status.total {
                    let id = client.send_read(&run.handle, run.next, chunk)?;
                    run.in_flight.push((id, run.next, chunk));
                    run.next += u64::from(chunk);
                }
                if run.in_flight.is_empty() {
                    client.close(&run.handle)?;
                    std::fs::rename(part_name(local), &*local)?;
                    return Ok(true);
                }
                let ids: Vec<u32> = run.in_flight.iter().map(|&(id, ..)| id).collect();
                let (id, reply) = client.reply_any(&ids)?;
                let at = run.in_flight.iter().position(|&(other, ..)| other == id).expect("asked for it");
                let (_, offset, len) = run.in_flight.swap_remove(at);
                match reply.data()? {
                    Some(data) => {
                        run.file.write_all_at(&data, offset)?;
                        status.done += data.len() as u64;
                        let end = offset + data.len() as u64;
                        // The file grew meanwhile: keep reading to its end.
                        status.total = status.total.max(end);
                        if data.len() < len as usize && !data.is_empty() {
                            let rest = len - data.len() as u32;
                            let id = client.send_read(&run.handle, end, rest)?;
                            run.in_flight.push((id, end, rest));
                        }
                    }
                    None => run.eof = Some(run.eof.map_or(offset, |eof| eof.min(offset))),
                }
                Ok(false)
            }
            Job::Upload { local, remote, source, created, run } => {
                if run.is_none() {
                    let file = File::open(&*local)?;
                    let meta = file.metadata()?;
                    let fresh = (meta.len(), meta.modified().ok());
                    let same = *source == Some(fresh) && *created;
                    *source = Some(fresh);
                    let existing = if same { client.lstat(remote).ok().and_then(|attrs| attrs.size) } else { None };
                    let (handle, offset) = match existing {
                        Some(size) if size >= resume => (client.open(remote, open::WRITE, Attrs::default())?, resume),
                        _ => {
                            if *created {
                                let _ = client.remove(remote);
                            }
                            let mode = meta.permissions().mode();
                            let flags = open::WRITE | open::CREATE | open::EXCLUSIVE;
                            (client.open(remote, flags, Attrs::mode(mode))?, 0)
                        }
                    };
                    *created = true;
                    status.total = meta.len();
                    status.done = offset;
                    *run = Some(Up { handle, file, in_flight: Vec::new(), next: offset });
                }
                let run = run.as_mut().expect("just started");
                let mut buf = vec![0u8; chunk as usize];
                while run.in_flight.len() < protocol::PIPELINE && run.next < status.total {
                    let n = run.file.read_at(&mut buf, run.next)?;
                    if n == 0 {
                        // Shrank meanwhile.
                        status.total = run.next;
                        break;
                    }
                    let id = client.send_write(&run.handle, run.next, &buf[..n])?;
                    run.in_flight.push((id, run.next, n as u64));
                    run.next += n as u64;
                }
                if run.in_flight.is_empty() {
                    client.close(&run.handle)?;
                    return Ok(true);
                }
                let ids: Vec<u32> = run.in_flight.iter().map(|&(id, ..)| id).collect();
                let (id, reply) = client.reply_any(&ids)?;
                let at = run.in_flight.iter().position(|&(other, ..)| other == id).expect("asked for it");
                let (_, _, len) = run.in_flight.swap_remove(at);
                reply.ok()?;
                status.done += len;
                Ok(false)
            }
        }
    }
}

/// Folders first, then by name, ignoring case.
pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.attrs.is_dir().cmp(&a.attrs.is_dir()).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// `dir/name` on the server (always `/`).
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

/// The last part of a server path.
pub fn file_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

/// The folder above a server path; `/` stays `/`.
pub fn parent(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/".to_string(),
        Some((parent, _)) => parent.to_string(),
    }
}

/// `name`, `name (1)`, `name (2)`, ... -- the number before the extension.
fn numbered(name: &str) -> impl Iterator<Item = String> + '_ {
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    std::iter::once(name.to_string()).chain((1..).map(move |n| format!("{stem} ({n}){ext}")))
}

fn free_local_name(dir: &Path, name: &str) -> PathBuf {
    numbered(name)
        .map(|candidate| dir.join(candidate))
        .find(|path| std::fs::symlink_metadata(path).is_err() && std::fs::symlink_metadata(part_name(path)).is_err())
        .expect("numbered names never run out")
}

/// Hidden next to the real name while it's being written.
fn part_name(path: &Path) -> PathBuf {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.part"))
}

fn display(path: &Path) -> String {
    crate::ssh::display_path(path)
}

#[cfg(test)]
pub mod tests {
    use std::process::{Child, Command as Process, Stdio};
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use super::super::protocol::tests::{scratch, sftp_server};
    use super::*;

    #[test]
    fn remote_paths() {
        assert_eq!(join("/home/u", "a"), "/home/u/a");
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(file_name("/etc/nginx/nginx.conf"), "nginx.conf");
        assert_eq!(parent("/etc/nginx"), "/etc");
        assert_eq!(parent("/etc"), "/");
        assert_eq!(parent("/"), "/");
    }

    #[test]
    fn taken_names_get_a_number() {
        let names: Vec<String> = numbered("notes.txt").take(3).collect();
        assert_eq!(names, ["notes.txt", "notes (1).txt", "notes (2).txt"]);
        assert_eq!(numbered(".bashrc").nth(1).unwrap(), ".bashrc (1)");
        assert_eq!(numbered("dir").nth(2).unwrap(), "dir (2)");
    }

    #[test]
    fn folders_come_first() {
        let entry = |name: &str, dir: bool| Entry {
            name: name.into(),
            attrs: Attrs { permissions: Some(if dir { 0o40755 } else { 0o100644 }), ..Attrs::default() },
        };
        let mut entries = vec![entry("b", false), entry("Z", true), entry("a", true), entry("A.txt", false)];
        sort_entries(&mut entries);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a", "Z", "A.txt", "b"]);
    }

    /// Reads with a pause: slow enough to cut a transfer mid-way.
    struct Slow<R>(R);

    impl<R: Read> Read for Slow<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            std::thread::sleep(Duration::from_millis(2));
            self.0.read(buf)
        }
    }

    /// "The server": a fresh `sftp-server` per connection, working in
    /// `home`; commands run with `sh` here. The connection can be dropped
    /// and brought back.
    pub struct TestServer {
        path: PathBuf,
        pub home: PathBuf,
        connection: AtomicU64,
        children: Mutex<Vec<Child>>,
        slow: bool,
    }

    impl TestServer {
        pub fn drop_connection(&self) {
            self.connection.store(0, Ordering::SeqCst);
            for mut child in self.children.lock().unwrap().drain(..) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        /// `sftp-server`s still running.
        pub fn running(&self) -> usize {
            self.children.lock().unwrap().iter_mut().filter(|child| child.try_wait().is_ok_and(|done| done.is_none())).count()
        }

        pub fn reconnect(&self) {
            static NEXT: AtomicU64 = AtomicU64::new(100);
            self.connection.store(NEXT.fetch_add(1, Ordering::SeqCst), Ordering::SeqCst);
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.drop_connection();
        }
    }

    impl Channels for TestServer {
        fn connection(&self) -> u64 {
            self.connection.load(Ordering::SeqCst)
        }

        fn open_sftp(&self) -> io::Result<Stream> {
            if self.connection() == 0 {
                return Err(io::Error::other("not connected"));
            }
            let mut child =
                Process::new(&self.path).current_dir(&self.home).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
            let (stdout, stdin) = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
            let fd = stdout.as_raw_fd();
            self.children.lock().unwrap().push(child);
            let reader: Box<dyn Read + Send> = if self.slow { Box::new(Slow(stdout)) } else { Box::new(stdout) };
            Ok(Stream { reader, writer: Box::new(stdin), fd })
        }

        fn exec(&self, command: &str, input: &[u8]) -> io::Result<Vec<u8>> {
            let mut child = Process::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .current_dir(&self.home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            child.stdin.take().unwrap().write_all(input)?;
            let output = child.wait_with_output()?;
            Ok([output.stdout, output.stderr].concat())
        }
    }

    pub struct Session {
        pub remote: Remote,
        pub server: Arc<TestServer>,
        pub wakes: Arc<AtomicUsize>,
    }

    /// A connected session against `sftp-server`; `None` if it isn't
    /// installed.
    pub fn session(name: &str) -> Option<Session> {
        session_with(name, false)
    }

    pub fn session_with(name: &str, slow: bool) -> Option<Session> {
        let path = sftp_server()?;
        let server = Arc::new(TestServer {
            path,
            home: scratch(&format!("{name}-home")),
            connection: AtomicU64::new(1),
            children: Mutex::new(Vec::new()),
            slow,
        });
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let wake: Wake = Arc::new(move || drop(counter.fetch_add(1, Ordering::SeqCst)));
        let root = scratch(&format!("{name}-edits"));
        let channels: Arc<dyn Channels> = server.clone();
        let remote = Remote::spawn_with(channels, "test", root.join("local"), wake).unwrap();
        Some(Session { remote, server, wakes })
    }

    /// Wait until `done` holds for the published state.
    pub fn wait_for(remote: &Remote, what: &str, done: impl Fn(&State) -> bool) -> State {
        let start = Instant::now();
        loop {
            let state = remote.state().clone();
            if done(&state) {
                return state;
            }
            assert!(start.elapsed() < Duration::from_secs(20), "timed out waiting for {what}: {state:#?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn browses_and_changes_folders() {
        let Some(Session { remote, wakes, .. }) = session("browse") else { return };
        let state = wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        assert!(!state.home.is_empty() && state.dir == state.home);
        assert!(wakes.load(Ordering::SeqCst) > 0);

        let dir = scratch("browse");
        let dir_s = dir.to_str().unwrap().to_string();
        std::fs::write(dir.join("file.txt"), "x").unwrap();
        remote.send(Command::List(Some(dir_s.clone())));
        wait_for(&remote, "listing", |s| s.dir == dir_s && s.entries.len() == 1);

        remote.send(Command::Mkdir("sub".into()));
        let state = wait_for(&remote, "mkdir", |s| s.entries.len() == 2);
        assert_eq!(state.entries[0].name, "sub");
        remote.send(Command::Rename { from: join(&dir_s, "file.txt"), to: join(&dir_s, "renamed.txt") });
        wait_for(&remote, "rename", |s| s.entries.iter().any(|e| e.name == "renamed.txt"));
        remote.send(Command::Remove { path: join(&dir_s, "sub"), dir: true });
        wait_for(&remote, "rmdir", |s| s.entries.len() == 1);

        remote.send(Command::List(Some(join(&dir_s, "missing"))));
        let state = wait_for(&remote, "failed listing", |s| s.listing_error.is_some());
        assert_eq!(state.dir, dir_s, "stays in the old folder");

        remote.send(Command::Remove { path: join(&dir_s, "nope"), dir: false });
        wait_for(&remote, "error report", |s| matches!(s.report, Some(Err(_))));

        drop(remote);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn transfers_files_and_folders_both_ways_without_overwriting() {
        let Some(Session { remote, .. }) = session("transfer") else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("transfer");
        let (server, local) = (root.join("server"), root.join("local"));
        std::fs::create_dir_all(server.join("tree/deeper")).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        let big: Vec<u8> = (0..3_000_000).map(|i| (i % 253) as u8).collect();
        std::fs::write(server.join("big.bin"), &big).unwrap();
        std::fs::write(server.join("tree/a.txt"), "a").unwrap();
        std::fs::write(server.join("tree/deeper/b.txt"), "b").unwrap();
        std::fs::write(local.join("big.bin"), "already here").unwrap();
        let s = |path: &Path| path.to_str().unwrap().to_string();

        remote.send(Command::Download { remote: s(&server.join("big.bin")), local_dir: local.clone() });
        remote.send(Command::Download { remote: s(&server.join("tree")), local_dir: local.clone() });
        let state = wait_for(&remote, "downloads", |s| s.transfers.len() == 3 && s.transfers.iter().all(|t| t.is_over()));
        assert!(state.transfers.iter().all(|t| t.state == TransferState::Done), "{:?}", state.transfers);
        assert_eq!(std::fs::read(local.join("big (1).bin")).unwrap(), big);
        assert_eq!(std::fs::read_to_string(local.join("big.bin")).unwrap(), "already here");
        assert_eq!(std::fs::read_to_string(local.join("tree/deeper/b.txt")).unwrap(), "b");
        assert!(!local.join(".big (1).bin.part").exists());

        remote.send(Command::ClearTransfers);
        wait_for(&remote, "cleared", |s| s.transfers.is_empty());
        remote.send(Command::Upload { local: local.join("tree"), remote_dir: s(&server) });
        remote.send(Command::Upload { local: local.join("big (1).bin"), remote_dir: s(&server.join("tree")) });
        let state = wait_for(&remote, "uploads", |s| s.transfers.len() == 3 && s.transfers.iter().all(|t| t.is_over()));
        assert!(state.transfers.iter().all(|t| t.state == TransferState::Done), "{:?}", state.transfers);
        assert_eq!(std::fs::read_to_string(server.join("tree (1)/deeper/b.txt")).unwrap(), "b");
        assert_eq!(std::fs::read(server.join("tree/big (1).bin")).unwrap(), big);

        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_cancelled_transfer_leaves_nothing_behind() {
        let Some(Session { remote, .. }) = session("cancel") else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("cancel");
        // Sparse: far too big to be done before the cancel comes in.
        File::create(root.join("huge")).unwrap().set_len(8 << 30).unwrap();
        std::fs::create_dir_all(root.join("into")).unwrap();
        remote.send(Command::Download { remote: root.join("huge").to_str().unwrap().into(), local_dir: root.join("into") });
        // The first transfer gets id 1; it's cancelled mid-way.
        remote.send(Command::Cancel(1));
        let state = wait_for(&remote, "cancelled", |s| s.transfers.first().is_some_and(TransferStatus::is_over));
        assert_eq!(state.transfers[0].state, TransferState::Cancelled);
        assert_eq!(std::fs::read_dir(root.join("into")).unwrap().count(), 0);
        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn notices_a_dropped_connection_while_idle_and_attaches_again() {
        let Some(Session { remote, server, .. }) = session("idle-drop") else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let started = Instant::now();
        server.drop_connection();
        wait_for(&remote, "noticed", |s| s.connection == Connection::Waiting);
        assert!(started.elapsed() < Duration::from_secs(3), "took {:?}", started.elapsed());
        remote.send(Command::List(None));
        wait_for(&remote, "offline report", |s| matches!(&s.report, Some(Err(_))));
        server.reconnect();
        let state = wait_for(&remote, "attached again", |s| s.connection == Connection::Ready);
        assert!(state.attached);
    }

    #[test]
    fn transfers_carry_on_after_the_connection_comes_back() {
        let Some(Session { remote, server, .. }) = session_with("resume", true) else { return };
        wait_for(&remote, "connected", |s| s.connection == Connection::Ready);
        let root = scratch("resume");
        let (serverside, local) = (root.join("server"), root.join("local"));
        std::fs::create_dir_all(&serverside).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        let data: Vec<u8> = (0..24_000_000u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
        std::fs::write(serverside.join("down.bin"), &data).unwrap();
        std::fs::write(local.join("up.bin"), &data).unwrap();
        let s = |path: &Path| path.to_str().unwrap().to_string();

        for direction in [Direction::Download, Direction::Upload] {
            remote.send(match direction {
                Direction::Download => Command::Download { remote: s(&serverside.join("down.bin")), local_dir: local.clone() },
                Direction::Upload => Command::Upload { local: local.join("up.bin"), remote_dir: s(&serverside) },
            });
            let under_way = wait_for(&remote, "under way", |s| {
                s.transfers.last().is_some_and(|t| t.direction == direction && t.done > 2_000_000 && !t.is_over())
            });
            let id = under_way.transfers.last().unwrap().id;
            server.drop_connection();
            let waiting = wait_for(&remote, "waiting", |s| s.connection == Connection::Waiting);
            let before = waiting.transfers.iter().find(|t| t.id == id).unwrap().clone();
            assert!(!before.is_over(), "{before:?}");
            server.reconnect();
            let resumed = wait_for(&remote, "resumed", |s| {
                s.connection == Connection::Ready && s.transfers.iter().any(|t| t.id == id && t.state == TransferState::Running)
            });
            let carried_on = resumed.transfers.iter().find(|t| t.id == id).unwrap();
            assert!(carried_on.done >= 1_000_000, "started over: {carried_on:?}");
            let done = wait_for(&remote, "done", |s| s.transfers.iter().any(|t| t.id == id && t.is_over()));
            assert_eq!(done.transfers.iter().find(|t| t.id == id).unwrap().state, TransferState::Done);
        }
        assert_eq!(std::fs::read(local.join("down.bin")).unwrap(), data);
        assert_eq!(std::fs::read(serverside.join("up.bin")).unwrap(), data);
        drop(remote);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
