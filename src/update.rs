//! Updates from the GitHub releases. [`check`] asks GitHub for the latest
//! release and says whether it's newer than the running Terminaal;
//! [`install`] downloads its binary for this machine, checks it against
//! the SHA-256 published with it and puts it in place of the running one
//! (a rename, so a half-written file never sits there). The new version
//! runs from the next start; [`restart`] starts it once this one is gone.
//!
//! A release (`.github/workflows/release.yml`) carries the bare binary as
//! `terminaal-<arch>-linux` and its checksum as `….sha256`. Without them
//! -- another architecture, a release made by hand -- there's only the
//! release page to point to. Downloads go through [`Fetch`], so the tests
//! don't need the network.

use std::fs::{File, OpenOptions};
use std::io::Write;
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

/// The release asset with the binary for this machine.
pub fn asset_name() -> String {
    format!("terminaal-{}-linux", std::env::consts::ARCH)
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
        let mut response = match self.0.get(url).header("Accept", "application/vnd.github+json").call() {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(404)) => return Ok(None),
            Err(err) => return Err(err.to_string()),
        };
        response.body_mut().with_config().limit(limit).read_to_vec().map(Some).map_err(|err| err.to_string())
    }
}

/// Where to ask instead of GitHub (a local server with a release in
/// GitHub's format), to try updating end to end. Also makes development
/// builds check at start. See RELEASING.md.
pub const URL_OVERRIDE: &str = "TERMINAAL_UPDATE_URL";

/// The latest release, if it's newer than `current`.
pub fn check(fetch: &impl Fetch, current: Version) -> Result<Option<Release>, String> {
    let url = std::env::var(URL_OVERRIDE)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    // No release at all yet: nothing newer either.
    let Some(body) = fetch.get(&url, 1024 * 1024)? else { return Ok(None) };
    let json = String::from_utf8(body).map_err(|err| err.to_string())?;
    Ok(parse_release(&json, &asset_name())?.filter(|release| release.version > current))
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

/// Download `binary`, check it and put it in place of `exe`. The new file
/// is written next to `exe` first and renamed over it, so `exe` is either
/// the old or the new binary, never half of one; the running process
/// keeps the old one open.
pub fn install(fetch: &impl Fetch, binary: &Binary, exe: &Path) -> Result<(), InstallError> {
    let failed = |err: String| InstallError::Failed(err);
    let dir = exe.parent().ok_or_else(|| failed(format!("{} has no folder", exe.display())))?;
    let staged = dir.join(format!(".terminaal-update-{}", std::process::id()));
    let mut file = match OpenOptions::new().write(true).create_new(true).mode(0o755).open(&staged) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return Err(InstallError::NotWritable(dir.to_path_buf())),
        Err(err) => return Err(failed(format!("{}: {err}", staged.display()))),
    };
    let result = stage(fetch, binary, exe, &mut file, &staged).and_then(|()| {
        std::fs::rename(&staged, exe).map_err(|err| format!("{}: {err}", exe.display()))
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result.map_err(failed)
}

/// Fill `file` (at `staged`) with the checked download, executable like
/// `exe`.
fn stage(fetch: &impl Fetch, binary: &Binary, exe: &Path, file: &mut File, staged: &Path) -> Result<(), String> {
    let missing = |url: &str| format!("{url} is gone");
    let checksum = fetch.get(&binary.checksum_url, 4096)?.ok_or_else(|| missing(&binary.checksum_url))?;
    let expected = parse_checksum(&String::from_utf8_lossy(&checksum)).ok_or("the release's checksum file is unreadable")?;
    let data = fetch.get(&binary.url, MAX_BINARY)?.ok_or_else(|| missing(&binary.url))?;
    if <[u8; 32]>::from(Sha256::digest(&data)) != expected {
        return Err("the download doesn't match the release's checksum".into());
    }
    if !data.starts_with(b"\x7fELF") {
        return Err("the download isn't a Linux program".into());
    }
    file.write_all(&data).and_then(|()| file.sync_all()).map_err(|err| format!("{}: {err}", staged.display()))?;
    // As executable as the binary it replaces.
    let mode = std::fs::metadata(exe).map(|meta| meta.permissions().mode() & 0o7777).unwrap_or(0o755);
    std::fs::set_permissions(staged, std::fs::Permissions::from_mode(mode)).map_err(|err| err.to_string())
}

/// Start `exe` once this process has ended (within ten seconds): it then
/// gets the session, whose lock this one still holds until it's gone.
pub fn restart(exe: &Path) -> std::io::Result<()> {
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
        let asset = asset_name();
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
        assert!(check(&fake, Version(1, 1, 9)).unwrap().is_some());
        assert!(check(&fake, Version(1, 2, 0)).unwrap().is_none());
        assert!(check(&fake, Version(2, 0, 0)).unwrap().is_none());
        assert_eq!(check(&Fake::default(), Version(1, 0, 0)), Ok(None), "no releases yet");
    }

    /// Against the real GitHub: TLS through OpenSSL and the API's answer.
    /// `cargo test -- --ignored talks_to_github`.
    #[test]
    #[ignore = "needs the network"]
    fn talks_to_github() {
        let http = Http::new();
        let repo = http.get(&format!("https://api.github.com/repos/{REPO}"), 1024 * 1024).unwrap().unwrap();
        assert!(String::from_utf8_lossy(&repo).contains("\"full_name\""));
        check(&http, Version(0, 0, 0)).unwrap();
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

    #[test]
    fn installs_a_checked_binary_in_place() {
        let dir = temp_dir("ok");
        let exe = dir.join("terminaal");
        std::fs::write(&exe, b"\x7fELF old").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o750)).unwrap();
        let new = b"\x7fELF new version";
        let (fake, binary) = serving(new, new);
        install(&fake, &binary, &exe).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), new);
        assert_eq!(std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o750, "keeps the mode");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "nothing left over");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_download_leaves_the_binary_alone() {
        let dir = temp_dir("bad");
        let exe = dir.join("terminaal");
        std::fs::write(&exe, b"\x7fELF old").unwrap();
        // Wrong checksum, not a program, nothing there.
        let (fake, binary) = serving(b"\x7fELF tampered", b"\x7fELF original");
        assert!(matches!(install(&fake, &binary, &exe), Err(InstallError::Failed(err)) if err.contains("checksum")));
        let (fake, binary) = serving(b"<html>", b"<html>");
        assert!(matches!(install(&fake, &binary, &exe), Err(InstallError::Failed(_))));
        let (_, binary) = serving(b"", b"");
        assert!(matches!(install(&Fake::default(), &binary, &exe), Err(InstallError::Failed(_))));
        assert_eq!(std::fs::read(&exe).unwrap(), b"\x7fELF old");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no staged file left behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_without_write_access_says_so() {
        let dir = temp_dir("ro");
        let exe = dir.join("terminaal");
        std::fs::write(&exe, b"\x7fELF old").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let (fake, binary) = serving(b"\x7fELF new", b"\x7fELF new");
        let result = install(&fake, &binary, &exe);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Root writes anywhere; then it simply works.
        if unsafe { libc::geteuid() } != 0 {
            assert_eq!(result, Err(InstallError::NotWritable(dir.clone())));
            assert!(fake.asked.borrow().is_empty(), "nothing downloaded for nothing");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
