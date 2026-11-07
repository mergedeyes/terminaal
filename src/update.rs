//! Updates from the GitHub releases. [`check`] asks GitHub for the latest
//! release and says whether it's newer than the running Terminaal;
//! [`install`] downloads its binary for this machine, checks it against
//! the SHA-256 published with it and puts it in place of the running one
//! (a rename, so a half-written file never sits there). The new version
//! runs from the next start; [`restart`] starts it once this one is gone.
//!
//! A release (`.github/workflows/release.yml`) carries the bare binary as
//! `terminaal-<arch>-linux` (`terminaal-<arch>-windows.exe`) and its
//! checksum as `….sha256`. Without them -- another architecture, a release
//! made by hand -- there's only the release page to point to. Downloads go
//! through [`Fetch`], so the tests don't need the network.
//!
//! On Windows a copy that Terminaal's setup installed ([`Installation`])
//! updates through the release's setup instead
//! (`terminaal-<arch>-windows-setup.exe`): [`install`] downloads and checks
//! it, and once Terminaal ends ([`restart`], [`apply_at_exit`]) it runs
//! silently -- the setup keeps its own entry under Settings → Apps, the
//! version shown there and the Start menu shortcut in step, and only it can
//! write to an install for all users (with a UAC prompt).

use std::fs::{File, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Where the releases are.
pub const REPO: &str = "mergedeyes/terminaal";
/// A binary bigger than this isn't ours.
const MAX_BINARY: u64 = 512 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(60);

/// The running version.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo.toml has a plain x.y.z version")
}

/// The setup's app ID (Inno Setup's `AppId` in `installer/terminaal.iss`):
/// its entry under Settings → Apps is `…\Uninstall\<this>_is1`.
#[cfg_attr(not(windows), allow(dead_code))]
pub const SETUP_APP_ID: &str = "{B9A7C3DB-EA83-4F63-A499-1A6BA7735663}";

/// The AppUserModelID on the setup's Start menu shortcut, which an
/// installed Terminaal takes on (`platform::use_app_id`).
#[cfg_attr(not(windows), allow(dead_code))]
pub const APP_USER_MODEL_ID: &str = "Terminaal.Terminaal";

/// How this copy of Terminaal got onto the machine, which decides what an
/// update downloads and how it goes in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Installation {
    /// A bare program file (always on Linux; on Windows the exe on its
    /// own): an update replaces it.
    Standalone,
    /// Installed by Terminaal's setup: an update runs the new setup.
    #[cfg(windows)]
    Setup { all_users: bool },
}

impl Installation {
    /// How `exe` was installed: by the setup if its entry under Settings →
    /// Apps names `exe`'s folder -- a copy elsewhere is on its own, even
    /// with the setup's install on the same machine.
    pub fn of(exe: Option<&Path>) -> Self {
        #[cfg(windows)]
        if let Some(here) = exe.and_then(Path::parent).and_then(|dir| std::fs::canonicalize(dir).ok()) {
            let installs = crate::platform::setup_folders(SETUP_APP_ID);
            if let Some((_, all_users)) = installs.into_iter().find(|(dir, _)| std::fs::canonicalize(dir).ok().as_ref() == Some(&here)) {
                return Self::Setup { all_users };
            }
        }
        #[cfg(unix)]
        let _ = exe;
        Self::Standalone
    }

    /// The release asset an update of this copy downloads -- only ever one
    /// for this system.
    pub fn asset(self) -> String {
        match self {
            Self::Standalone => asset_name(),
            #[cfg(windows)]
            Self::Setup { .. } => format!("terminaal-{}-windows-setup.exe", std::env::consts::ARCH),
        }
    }
}

/// The release asset with the binary for this machine.
#[cfg(unix)]
pub fn asset_name() -> String {
    format!("terminaal-{}-linux", std::env::consts::ARCH)
}

#[cfg(windows)]
pub fn asset_name() -> String {
    format!("terminaal-{}-windows.exe", std::env::consts::ARCH)
}

/// A release version, `x.y.z`; a leading `v` (the tag) is fine.
/// Pre-releases (`1.0.0-rc1`) don't parse: they're never offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut parts = text.split('.').map(|part| part.parse::<u64>().ok());
        let version = Self(parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// A release on GitHub.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    /// Its page, with the notes.
    pub page: String,
    /// The notes as written (Markdown).
    pub notes: String,
    /// The binary for this machine, if the release has one.
    pub binary: Option<Binary>,
}

/// A downloadable binary and where its checksum is.
#[derive(Clone, Debug, PartialEq)]
pub struct Binary {
    pub url: String,
    pub checksum_url: String,
    pub size: u64,
}

/// GitHub's view of a release, as far as it matters here.
#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// The release in GitHub's JSON (`/releases/latest`), with the binary
/// named `asset` and its `.sha256` if both are there. `None` for a
/// draft, a pre-release or a tag that isn't a plain version.
fn parse_release(json: &str, asset: &str) -> Result<Option<Release>, String> {
    let release: ApiRelease = serde_json::from_str(json).map_err(|err| err.to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some(version) = Version::parse(&release.tag_name) else { return Ok(None) };
    let find = |name: &str| release.assets.iter().find(|a| a.name == name);
    let binary = match (find(asset), find(&format!("{asset}.sha256"))) {
        (Some(binary), Some(checksum)) => Some(Binary {
            url: binary.browser_download_url.clone(),
            checksum_url: checksum.browser_download_url.clone(),
            size: binary.size,
        }),
        _ => None,
    };
    Ok(Some(Release { version, page: release.html_url, notes: release.body.unwrap_or_default(), binary }))
}

/// Fetches a URL's body, at most `limit` bytes; `None` if there's
/// nothing there (404 -- a repository without releases yet, say).
pub trait Fetch {
    fn get(&self, url: &str, limit: u64) -> Result<Option<Vec<u8>>, String>;
}

/// HTTPS through the system's OpenSSL (already linked for libssh2).
pub struct Http(ureq::Agent);

impl Http {
    pub fn new() -> Self {
        let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(TIMEOUT))
            .user_agent(format!("terminaal/{}", env!("CARGO_PKG_VERSION")))
            .build();
        Self(config.into())
    }
}

impl Fetch for Http {
    fn get(&self, url: &str, limit: u64) -> Result<Option<Vec<u8>>, String> {
        // Only through `TERMINAAL_UPDATE_URL` (a URL from GitHub is always
        // https): a release in a folder, to try updates where no test
        // server can be reached -- a VM with a read-only shared folder.
        if let Some(path) = local_path(url) {
            return read_local(&path, limit);
        }
        let mut response = match self.0.get(url).header("Accept", "application/vnd.github+json").call() {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(404)) => return Ok(None),
            Err(err) => return Err(err.to_string()),
        };
        response.body_mut().with_config().limit(limit).read_to_vec().map(Some).map_err(|err| err.to_string())
    }
}

/// The file a `file:///` URL names: `file:///tmp/rel/x` is `/tmp/rel/x`,
/// `file:///Z:/rel/x` is `Z:/rel/x`.
fn local_path(url: &str) -> Option<PathBuf> {
    let path = url.strip_prefix("file://")?;
    let drive = path.strip_prefix('/').filter(|rest| rest.as_bytes().get(1) == Some(&b':'));
    Some(PathBuf::from(drive.unwrap_or(path)))
}

/// A local file like a download: `None` if it isn't there.
fn read_local(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    let mut data = Vec::new();
    file.take(limit + 1).read_to_end(&mut data).map_err(|err| format!("{}: {err}", path.display()))?;
    if data.len() as u64 > limit {
        return Err(format!("{} is bigger than {limit} bytes", path.display()));
    }
    Ok(Some(data))
}

/// Where to ask instead of GitHub (a local server with a release in
/// GitHub's format), to try updating end to end. Also makes development
/// builds check at start. See RELEASING.md.
pub const URL_OVERRIDE: &str = "TERMINAAL_UPDATE_URL";

/// The latest release, if it's newer than `current`, with the download
/// for `installation`.
pub fn check(fetch: &impl Fetch, current: Version, installation: Installation) -> Result<Option<Release>, String> {
    let url = std::env::var(URL_OVERRIDE)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    // No release at all yet: nothing newer either.
    let Some(body) = fetch.get(&url, 1024 * 1024)? else { return Ok(None) };
    let json = String::from_utf8(body).map_err(|err| err.to_string())?;
    Ok(parse_release(&json, &installation.asset())?.filter(|release| release.version > current))
}

/// Why an update couldn't be put in place.
#[derive(Clone, Debug, PartialEq)]
pub enum InstallError {
    /// No writing next to the binary (installed by the system's package
    /// manager, say): that's who updates it.
    NotWritable(PathBuf),
    /// Download, checksum or file trouble; what went wrong.
    Failed(String),
}

/// The SHA-256 at the start of a checksum file (`sha256sum` format).
fn parse_checksum(text: &str) -> Option<[u8; 32]> {
    let hex = text.split_whitespace().next()?;
    if hex.len() != 64 {
        return None;
    }
    let mut sum = [0; 32];
    for (i, byte) in sum.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(sum)
}

/// What a program for this system starts with, and what it's called.
#[cfg(unix)]
const PROGRAM: (&[u8], &str) = (b"\x7fELF", "a Linux program");
#[cfg(windows)]
const PROGRAM: (&[u8], &str) = (b"MZ", "a Windows program");

/// What [`install`] left for the restart.
#[derive(Clone, Debug, PartialEq)]
pub enum Installed {
    /// The program file is the new version already.
    Replaced,
    /// The new setup, checked, waits in `path` to run once Terminaal has
    /// ended ([`restart`], [`apply_at_exit`]).
    #[cfg(windows)]
    Setup { path: PathBuf, all_users: bool },
}

/// Download the update for `installation` and check it: a bare binary
/// goes in place of `exe` right away, a setup waits for the restart.
pub fn install(fetch: &impl Fetch, binary: &Binary, exe: &Path, installation: Installation) -> Result<Installed, InstallError> {
    match installation {
        Installation::Standalone => replace_binary(fetch, binary, exe).map(|()| Installed::Replaced),
        #[cfg(windows)]
        Installation::Setup { all_users } => stage_setup(fetch, binary, all_users),
    }
}

/// Download `binary`, check it and put it in place of `exe`. The new file
/// is written next to `exe` first and then takes its place
/// ([`replace`]), so `exe` is either the old or the new binary, never
/// half of one; the running process keeps the old one open.
fn replace_binary(fetch: &impl Fetch, binary: &Binary, exe: &Path) -> Result<(), InstallError> {
    let failed = |err: String| InstallError::Failed(err);
    let dir = exe.parent().ok_or_else(|| failed(format!("{} has no folder", exe.display())))?;
    let staged = dir.join(format!(".terminaal-update-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o755);
    let mut file = match options.open(&staged) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return Err(InstallError::NotWritable(dir.to_path_buf())),
        Err(err) => return Err(failed(format!("{}: {err}", staged.display()))),
    };
    let result = stage(fetch, binary, exe, &mut file, &staged).and_then(|()| {
        drop(file);
        replace(&staged, exe).map_err(|err| format!("{}: {err}", exe.display()))
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result.map_err(failed)
}

/// `binary`, downloaded and checked: its checksum matches, and it's a
/// program for this system.
fn download(fetch: &impl Fetch, binary: &Binary) -> Result<Vec<u8>, String> {
    let missing = |url: &str| format!("{url} is gone");
    let checksum = fetch.get(&binary.checksum_url, 4096)?.ok_or_else(|| missing(&binary.checksum_url))?;
    let expected = parse_checksum(&String::from_utf8_lossy(&checksum)).ok_or("the release's checksum file is unreadable")?;
    let data = fetch.get(&binary.url, MAX_BINARY)?.ok_or_else(|| missing(&binary.url))?;
    if <[u8; 32]>::from(Sha256::digest(&data)) != expected {
        return Err("the download doesn't match the release's checksum".into());
    }
    let (magic, kind) = PROGRAM;
    if !data.starts_with(magic) {
        return Err(format!("the download isn't {kind}"));
    }
    Ok(data)
}

/// Fill `file` (at `staged`) with the checked download, executable like
/// `exe`.
fn stage(fetch: &impl Fetch, binary: &Binary, exe: &Path, file: &mut File, staged: &Path) -> Result<(), String> {
    let data = download(fetch, binary)?;
    file.write_all(&data).and_then(|()| file.sync_all()).map_err(|err| format!("{}: {err}", staged.display()))?;
    // As executable as the binary it replaces.
    #[cfg(unix)]
    {
        let mode = std::fs::metadata(exe).map(|meta| meta.permissions().mode() & 0o7777).unwrap_or(0o755);
        std::fs::set_permissions(staged, std::fs::Permissions::from_mode(mode)).map_err(|err| err.to_string())?;
    }
    #[cfg(windows)]
    let _ = exe;
    Ok(())
}

/// Renamed over `exe`.
#[cfg(unix)]
fn replace(staged: &Path, exe: &Path) -> std::io::Result<()> {
    std::fs::rename(staged, exe)
}

/// A running program can't be replaced on Windows, only renamed: `exe`
/// moves aside to [`old`] (gone with the next start, [`clean_up`]), the
/// new one takes its name -- or the old one comes back.
#[cfg(windows)]
fn replace(staged: &Path, exe: &Path) -> std::io::Result<()> {
    let old = old(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old)?;
    std::fs::rename(staged, exe).inspect_err(|_| {
        let _ = std::fs::rename(&old, exe);
    })
}

/// The checked setup in Terminaal's temp folder
/// (`…\Temp\terminaal\update\terminaal-setup-<pid>.exe`), new for this
/// process.
#[cfg(windows)]
fn stage_setup(fetch: &impl Fetch, binary: &Binary, all_users: bool) -> Result<Installed, InstallError> {
    let failed = |err: String| InstallError::Failed(err);
    let data = download(fetch, binary).map_err(failed)?;
    let dir = crate::platform::user_temp_dir().join("update");
    crate::platform::create_private_dir(&dir).map_err(|err| failed(format!("{}: {err}", dir.display())))?;
    let path = dir.join(format!("terminaal-setup-{}.exe", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let written = crate::platform::create_private_file(&path).and_then(|mut file| {
        file.write_all(&data)?;
        file.sync_all()
    });
    if let Err(err) = written {
        let _ = std::fs::remove_file(&path);
        return Err(failed(format!("{}: {err}", path.display())));
    }
    Ok(Installed::Setup { path, all_users })
}

/// What the setup is run with: no window, no questions, never a reboot;
/// for the same users as before; a log next to it.
#[cfg(windows)]
fn setup_arguments(path: &Path, all_users: bool) -> String {
    let scope = if all_users { "/ALLUSERS" } else { "/CURRENTUSER" };
    format!(r#"/VERYSILENT /SUPPRESSMSGBOXES /NORESTART {scope} /LOG="{}""#, path.with_extension("log").display())
}

/// Where the replaced program waits on Windows: `terminaal.exe.old`.
#[cfg(windows)]
fn old(exe: &Path) -> PathBuf {
    let mut name = exe.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}

/// The program an update replaced, once nothing runs it any more.
#[cfg(windows)]
pub fn clean_up(exe: &Path) {
    let old = old(exe);
    if old.exists()
        && let Err(err) = std::fs::remove_file(&old)
    {
        log::info!("can't remove {} yet: {err}", old.display());
    }
}

/// Start `exe` once this process has ended (within ten seconds): it then
/// gets the session, whose lock this one still holds until it's gone.
#[cfg(unix)]
pub fn restart(exe: &Path, _installed: &Installed) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    let script = r#"i=0; while kill -0 "$1" 2>/dev/null && [ "$i" -lt 100 ]; do sleep 0.1; i=$((i + 1)); done; exec "$0""#;
    std::process::Command::new("sh")
        .args(["-c", script])
        .arg(exe)
        .arg(std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        // Not in this process's group: nothing sent to it reaches the new one.
        .process_group(0)
        .spawn()
        .map(drop)
}

/// Nothing left to do on Linux: the binary is the new one already.
#[cfg(unix)]
pub fn apply_at_exit(_exe: &Path, _installed: &Installed) {}

/// Run by [`finish`] in a hidden Windows PowerShell once this process has
/// ended (or after 30 s): the setup, if one waits, then Terminaal again
/// if asked to. The setup closes a drop-down Terminaal of the same install
/// (Restart Manager); it's started again afterwards, by this script, as
/// the user -- the setup may run elevated. Everything comes in by
/// environment, never as code.
#[cfg(windows)]
const FINISH: &str = r#"Wait-Process -Id $env:TERMINAAL_PID -Timeout 30 -ErrorAction SilentlyContinue
if ($env:TERMINAAL_SETUP) {
    $quake = @(Get-CimInstance Win32_Process -Filter "Name = 'terminaal.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.ExecutablePath -eq $env:TERMINAAL_EXE -and $_.CommandLine -match '--quake' })
    $setup = Start-Process -FilePath $env:TERMINAAL_SETUP -ArgumentList $env:TERMINAAL_SETUP_ARGS -PassThru -ErrorAction SilentlyContinue
    if ($setup) { $setup.WaitForExit() }
    Remove-Item -LiteralPath $env:TERMINAAL_SETUP -ErrorAction SilentlyContinue
    if ($quake.Count -gt 0) { Start-Process -FilePath $env:TERMINAAL_EXE -ArgumentList '--quake' }
}
if ($env:TERMINAAL_START) { Start-Process -FilePath $env:TERMINAAL_EXE }
"#;

/// The same on Windows: a waiting setup runs first, then `exe` starts.
#[cfg(windows)]
pub fn restart(exe: &Path, installed: &Installed) -> std::io::Result<()> {
    finish(exe, installed, true)
}

/// Terminaal ends without "Restart now": a waiting setup still runs
/// (then the next start is the new version, as with a replaced binary).
#[cfg(windows)]
pub fn apply_at_exit(exe: &Path, installed: &Installed) {
    if matches!(installed, Installed::Setup { .. })
        && let Err(err) = finish(exe, installed, false)
    {
        log::warn!("can't run the update's setup: {err}");
    }
}

/// [`FINISH`] in a hidden Windows PowerShell, detached from this process.
#[cfg(windows)]
fn finish(exe: &Path, installed: &Installed, start: bool) -> std::io::Result<()> {
    use base64::Engine;
    use std::os::windows::process::CommandExt;
    let utf16: Vec<u8> = FINISH.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut command = std::process::Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
        .arg(base64::engine::general_purpose::STANDARD.encode(utf16))
        .env("TERMINAAL_PID", std::process::id().to_string())
        .env("TERMINAAL_EXE", exe)
        .env_remove("TERMINAAL_SETUP")
        .env_remove("TERMINAAL_SETUP_ARGS")
        .env_remove("TERMINAAL_START");
    if let Installed::Setup { path, all_users } = installed {
        command.env("TERMINAAL_SETUP", path).env("TERMINAAL_SETUP_ARGS", setup_arguments(path, *all_users));
    }
    if start {
        command.env("TERMINAAL_START", "1");
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(crate::platform::CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    /// Serves fixed bodies; counts what was asked for.
    #[derive(Default)]
    struct Fake {
        bodies: HashMap<String, Vec<u8>>,
        asked: RefCell<Vec<String>>,
    }

    impl Fetch for Fake {
        fn get(&self, url: &str, limit: u64) -> Result<Option<Vec<u8>>, String> {
            self.asked.borrow_mut().push(url.to_string());
            let Some(body) = self.bodies.get(url).cloned() else { return Ok(None) };
            if body.len() as u64 > limit {
                return Err("too big".into());
            }
            Ok(Some(body))
        }
    }

    fn release_json(tag: &str, assets: &[&str]) -> String {
        let assets: Vec<String> = assets
            .iter()
            .map(|name| format!(r#"{{"name": "{name}", "browser_download_url": "https://dl/{name}", "size": 4}}"#))
            .collect();
        format!(
            r#"{{"tag_name": "{tag}", "html_url": "https://github.com/x/releases/tag/{tag}", "body": "- new stuff", "draft": false, "prerelease": false, "assets": [{}]}}"#,
            assets.join(",")
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("terminaal-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn versions() {
        assert_eq!(Version::parse("v0.2.0"), Some(Version(0, 2, 0)));
        assert_eq!(Version::parse("1.10.3"), Some(Version(1, 10, 3)));
        assert_eq!(Version::parse("1.0.0-rc1"), None);
        assert_eq!(Version::parse("1.0"), None);
        assert_eq!(Version::parse("1.0.0.0"), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 9), "numbers, not text");
        assert_eq!(Version(1, 2, 3).to_string(), "1.2.3");
        assert_eq!(current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn reads_the_latest_release() {
        let asset = Installation::Standalone.asset();
        assert_eq!(asset, asset_name());
        let json = release_json("v9.1.0", &[&asset, &format!("{asset}.sha256"), "terminaal-x86_64-linux.tar.gz"]);
        let release = parse_release(&json, &asset).unwrap().unwrap();
        assert_eq!(release.version, Version(9, 1, 0));
        assert_eq!(release.notes, "- new stuff");
        let binary = release.binary.unwrap();
        assert_eq!(binary.url, format!("https://dl/{asset}"));
        assert_eq!(binary.checksum_url, format!("https://dl/{asset}.sha256"));

        // No binary for this machine, or no checksum: only the page.
        let json = release_json("v9.1.0", &["terminaal-riscv64-linux"]);
        assert_eq!(parse_release(&json, &asset).unwrap().unwrap().binary, None);
        let json = release_json("v9.1.0", &[&asset]);
        assert_eq!(parse_release(&json, &asset).unwrap().unwrap().binary, None, "never without its checksum");

        assert_eq!(parse_release(&json.replace(r#""prerelease": false"#, r#""prerelease": true"#), &asset).unwrap(), None);
        assert_eq!(parse_release(&release_json("nightly", &[]), &asset).unwrap(), None);
        assert!(parse_release("<html>rate limited</html>", &asset).is_err());
    }

    #[test]
    fn only_newer_releases_are_offered() {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let mut fake = Fake::default();
        fake.bodies.insert(url.clone(), release_json("v1.2.0", &[]).into_bytes());
        let standalone = Installation::Standalone;
        assert!(check(&fake, Version(1, 1, 9), standalone).unwrap().is_some());
        assert!(check(&fake, Version(1, 2, 0), standalone).unwrap().is_none());
        assert!(check(&fake, Version(2, 0, 0), standalone).unwrap().is_none());
        assert_eq!(check(&Fake::default(), Version(1, 0, 0), standalone), Ok(None), "no releases yet");
    }

    /// A release carries downloads for every system, Linux and Windows,
    /// bare and as setup; each copy finds its own and nothing else.
    #[test]
    fn each_system_picks_only_its_own_download() {
        let all = [
            "terminaal-x86_64-linux",
            "terminaal-x86_64-linux.sha256",
            "terminaal-x86_64-linux.tar.gz",
            "terminaal-x86_64-linux.tar.gz.sha256",
            "terminaal-x86_64-windows.exe",
            "terminaal-x86_64-windows.exe.sha256",
            "terminaal-x86_64-windows-setup.exe",
            "terminaal-x86_64-windows-setup.exe.sha256",
        ];
        let json = release_json("v9.1.0", &all);
        let url = |installation: Installation| parse_release(&json, &installation.asset()).unwrap().unwrap().binary.unwrap().url;
        #[cfg(unix)]
        assert_eq!(url(Installation::Standalone), "https://dl/terminaal-x86_64-linux");
        #[cfg(windows)]
        {
            assert_eq!(url(Installation::Standalone), "https://dl/terminaal-x86_64-windows.exe");
            assert_eq!(url(Installation::Setup { all_users: false }), "https://dl/terminaal-x86_64-windows-setup.exe");
            assert_eq!(url(Installation::Setup { all_users: true }), "https://dl/terminaal-x86_64-windows-setup.exe");
        }
        // A Linux-only release is no update for Windows, and the other way round.
        let linux = release_json("v9.1.0", &all[..4]);
        let windows = release_json("v9.1.0", &all[4..]);
        let found = |json: &str| parse_release(json, &Installation::Standalone.asset()).unwrap().unwrap().binary.is_some();
        assert_eq!((found(&linux), found(&windows)), (cfg!(unix), cfg!(windows)));
    }

    /// The setup script, the release workflow and the program agree on the
    /// names that tie them together.
    #[test]
    fn the_setup_and_the_release_match_the_program() {
        let setup = include_str!("../installer/terminaal.iss");
        // Inno Setup doubles a leading brace to escape it.
        assert!(setup.contains(&format!("AppId={{{SETUP_APP_ID}")), "AppId");
        assert!(setup.contains(&format!(r#"AppUserModelID: "{APP_USER_MODEL_ID}""#)), "AppUserModelID");
        let release = include_str!("../.github/workflows/release.yml");
        assert!(release.contains("terminaal-x86_64-windows-setup.exe"), "setup asset");
        assert!(release.contains("terminaal-x86_64-windows.exe"), "bare exe asset");
    }

    /// Against the real GitHub: TLS through OpenSSL and the API's answer.
    /// `cargo test -- --ignored talks_to_github`.
    #[test]
    #[ignore = "needs the network"]
    fn talks_to_github() {
        let http = Http::new();
        let repo = http.get(&format!("https://api.github.com/repos/{REPO}"), 1024 * 1024).unwrap().unwrap();
        assert!(String::from_utf8_lossy(&repo).contains("\"full_name\""));
        check(&http, Version(0, 0, 0), Installation::Standalone).unwrap();
    }

    /// A release in a folder (`TERMINAAL_UPDATE_URL=file:///…`), read
    /// like downloads.
    #[test]
    fn reads_a_release_from_a_folder() {
        assert_eq!(local_path("file:///tmp/rel/latest.json"), Some(PathBuf::from("/tmp/rel/latest.json")));
        assert_eq!(local_path("file:///Z:/rel/latest.json"), Some(PathBuf::from("Z:/rel/latest.json")));
        assert_eq!(local_path("https://api.github.com/x"), None);
        let dir = temp_dir("folder");
        std::fs::write(dir.join("latest.json"), b"{}").unwrap();
        let url = |name: &str| format!("file:///{}", dir.join(name).display().to_string().replace('\\', "/").trim_start_matches('/'));
        let http = Http::new();
        assert_eq!(http.get(&url("latest.json"), 10), Ok(Some(b"{}".to_vec())));
        assert_eq!(http.get(&url("missing"), 10), Ok(None));
        assert!(http.get(&url("latest.json"), 1).is_err(), "over the limit");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn checksums() {
        let sum = Sha256::digest(b"x");
        let hex: String = sum.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(parse_checksum(&format!("{hex}  terminaal-x86_64-linux\n")), Some(sum.into()));
        assert_eq!(parse_checksum(&hex.to_uppercase()), Some(sum.into()));
        assert_eq!(parse_checksum("abc"), None);
        assert_eq!(parse_checksum(""), None);
        assert_eq!(parse_checksum(&"g".repeat(64)), None);
    }

    fn serving(data: &[u8], checksum_of: &[u8]) -> (Fake, Binary) {
        let hex: String = Sha256::digest(checksum_of).iter().map(|b| format!("{b:02x}")).collect();
        let mut fake = Fake::default();
        fake.bodies.insert("https://dl/bin".into(), data.to_vec());
        fake.bodies.insert("https://dl/bin.sha256".into(), format!("{hex}  bin\n").into_bytes());
        let binary = Binary { url: "https://dl/bin".into(), checksum_url: "https://dl/bin.sha256".into(), size: data.len() as u64 };
        (fake, binary)
    }

    /// A program for this system: its header, then `rest`.
    fn program(rest: &str) -> Vec<u8> {
        [PROGRAM.0, rest.as_bytes()].concat()
    }

    #[test]
    fn installs_a_checked_binary_in_place() {
        let dir = temp_dir("ok");
        let exe = dir.join("terminaal");
        std::fs::write(&exe, program(" old")).unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o750)).unwrap();
        let new = program(" new version");
        let (fake, binary) = serving(&new, &new);
        assert_eq!(install(&fake, &binary, &exe, Installation::Standalone), Ok(Installed::Replaced));
        assert_eq!(std::fs::read(&exe).unwrap(), new);
        #[cfg(unix)]
        assert_eq!(std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o750, "keeps the mode");
        #[cfg(windows)]
        {
            assert_eq!(std::fs::read(old(&exe)).unwrap(), program(" old"), "the old one moved aside");
            clean_up(&exe);
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "nothing left over");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_download_leaves_the_binary_alone() {
        let dir = temp_dir("bad");
        let exe = dir.join("terminaal");
        std::fs::write(&exe, program(" old")).unwrap();
        // Wrong checksum, not a program, nothing there.
        let (fake, binary) = serving(&program(" tampered"), &program(" original"));
        assert!(matches!(install(&fake, &binary, &exe, Installation::Standalone), Err(InstallError::Failed(err)) if err.contains("checksum")));
        let (fake, binary) = serving(b"<html>", b"<html>");
        assert!(matches!(install(&fake, &binary, &exe, Installation::Standalone), Err(InstallError::Failed(_))));
        let (_, binary) = serving(b"", b"");
        assert!(matches!(install(&Fake::default(), &binary, &exe, Installation::Standalone), Err(InstallError::Failed(_))));
        assert_eq!(std::fs::read(&exe).unwrap(), program(" old"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no staged file left behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_without_write_access_says_so() {
        let dir = temp_dir("ro");
        let exe = dir.join("terminaal");
        std::fs::write(&exe, b"\x7fELF old").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let (fake, binary) = serving(b"\x7fELF new", b"\x7fELF new");
        let result = install(&fake, &binary, &exe, Installation::Standalone);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Root writes anywhere; then it simply works.
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(result, Err(InstallError::NotWritable(dir.clone())));
            assert!(fake.asked.borrow().is_empty(), "nothing downloaded for nothing");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An installed copy downloads the setup into its temp folder, checked
    /// like a binary; the program file stays untouched until the setup runs.
    #[cfg(windows)]
    #[test]
    fn an_installed_copy_stages_the_checked_setup() {
        let dir = temp_dir("setup");
        let exe = dir.join("terminaal.exe");
        std::fs::write(&exe, program(" old")).unwrap();
        let new = program(" setup");
        let (fake, binary) = serving(&new, &new);
        let Ok(Installed::Setup { path, all_users }) = install(&fake, &binary, &exe, Installation::Setup { all_users: true }) else {
            panic!("no setup staged");
        };
        assert!(all_users);
        assert_eq!(std::fs::read(&path).unwrap(), new);
        assert_eq!(std::fs::read(&exe).unwrap(), program(" old"), "the program stays until the setup runs");
        let arguments = setup_arguments(&path, all_users);
        assert!(arguments.starts_with("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /ALLUSERS /LOG="), "{arguments}");
        assert!(setup_arguments(&path, false).contains("/CURRENTUSER"));
        std::fs::remove_file(&path).unwrap();

        let (fake, binary) = serving(&program(" tampered"), &new);
        assert!(matches!(install(&fake, &binary, &exe, Installation::Setup { all_users: false }), Err(InstallError::Failed(_))));
        assert!(!path.exists(), "nothing staged from a bad download");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A copy outside the setup's folder (or with no setup at all) is on
    /// its own.
    #[cfg(windows)]
    #[test]
    fn a_copy_elsewhere_is_standalone() {
        let dir = temp_dir("elsewhere");
        assert_eq!(Installation::of(Some(&dir.join("terminaal.exe"))), Installation::Standalone);
        assert_eq!(Installation::of(None), Installation::Standalone);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
