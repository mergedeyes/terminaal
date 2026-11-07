# Windows port — roadmap

Branch `feature/windows-port`. **Merge rule:** this branch goes into `main` only
once Windows has the full working feature list of the Linux build — no early
"it builds" merge (a release would offer a half-working Windows exe as an
update). This file lives only on the branch; before the merge its content moves
into README/docs and the file is deleted.

Order follows the project's own: functionality first, then install/UX, then
visual polish last.

Status as of 2026-11-07, branch up to date with `terminaal/main` at `25ddcdb`.

## Done

| Area | What works on Windows |
|---|---|
| Build & CI | `src/platform/` holds the OS differences (stream pairs: loopback TCP + `WSAPoll`; locks; permissions; links via `explorer.exe`; trash = Recycle Bin; local time). CI on `windows-latest`: clippy, tests, release exe as artifact. Release workflow builds a Windows exe |
| Startup | Window program without a console; `--help`/`--version`/errors/`RUST_LOG` go to the parent console (`AttachConsole`). Settings in `%APPDATA%\terminaal`, state in `%LOCALAPPDATA%\terminaal`. German UI from the Windows locale |
| Local terminal | ConPTY; shell-integration filter inside ConPTY reads; shells found: pwsh 7 (and preview), Windows PowerShell, cmd, Git Bash (machine or per-user), Nushell, WSL, plus `pwsh`/`nu` anywhere on the `PATH` (default: first non-cmd) |
| Shell integration | PowerShell: OSC 7 + OSC 133 (prompt marks, exit codes, durations, notifications). Git Bash: the bash integration with `/` paths for `--rcfile` and the managed file; MSYS (`/c/…`) and Cygwin (`/cygdrive/c/…`) folders from OSC 7 become `C:\…`; the launcher's `usr\bin\bash` child counts as the shell. Untested in a real Git Bash. cmd: `PROMPT` set at launch wraps the user's own prompt (else `$P$G`) in OSC 133 A/B and sends the folder as OSC 9;9, which the filter now reads (all platforms); no C/D (cmd has no pre-exec hook): no durations, exit codes, notifications, output marks. Only through tests so far, not in a real cmd |
| Sidebar | Aliases, functions and lines for PowerShell (profile-loaded file of Terminaal's own) |
| Built-in commands | Systems `windows` (PowerShell) and `windows-cmd`: winget updates, disks, folder sizes, memory, processes, uptime, failed services, log errors, ports, addresses; usable for SSH hosts via `system`, and recognized by themselves: when the Unix probe finds nothing, `echo %OS% $PSVersionTable` tells cmd (`windows-cmd`) from PowerShell (`windows`) — Linux builds get this too. Not yet against a real Windows OpenSSH server |
| SSH | Agent auth (Pageant, Windows OpenSSH agent pipe), Ed25519 keys, ProxyCommand via `cmd /C`. Agent forwarding to named pipes (`ssh::agent_pipe`): Windows' OpenSSH agent when `SSH_AUTH_SOCK` is unset (also 1Password, Bitwarden, KeePassXC), any pipe in `IdentityAgent`/`ForwardAgent` (Pageant: the path `pageant --openssh-config` writes); a thread relays message by message between the pipe and a socket pair. Tested against a fake agent on a real pipe (Wine); not yet against a real server |
| SFTP | Edit-and-sync-back and live folder sync with `ReadDirectoryChangesW`; excludes with `\` paths |
| Unix-socket forwards | `AF_UNIX` (Windows 10 1803+) through `platform::UnixListener`/`UnixStream` (own WinSock wrapper, polled with `WSAPoll`): local forwards listening on a socket, targets that are sockets, stale sockets replaced (reparse tag `IO_REPARSE_TAG_AF_UNIX`), only our own file removed (NTFS file index); `C:\…` paths in forwards. Wine has no `AF_UNIX` (tests skip there); **needs a run on real Windows** (CI or VM) |
| Program in the tab title | `platform::running_program`: the shell's newest child process by creation time (Toolhelp32 snapshot), console hosts and `wslhost` skipped, children older than their parent ignored (reused PIDs), a child named like the shell taken as the shell behind a launcher (Git Bash). Same 250 ms poll as on Linux. Programs inside WSL stay invisible (they're Linux processes); PowerShell background jobs would show as `pwsh` |
| Links & hints | File paths with drive letters (`C:\…`), runnable extensions open their folder instead |
| Notifications, updates | Toast notifications; in-place update (running exe renamed to `.old`, cleaned up next start; restart via hidden PowerShell) |
| Command line | `-c` with cmd, `-s` with a Windows path |
| Drop-down window | `terminaal --quake`: lock + loopback port in the temp folder, second start toggles and hands over the foreground; global shortcut `quake_hotkey` (default Ctrl+Alt+T, `RegisterHotKey`); frameless, on top, not in the taskbar, height slider live, hides on focus loss |
| Fonts | Default console font Cascadia Mono → Consolas → Lucida Console → Courier New (cosmic-text's Noto Sans Mono isn't on Windows) |

## W1 — Functional gaps (blocking the merge)

All closed in code (2026-11-07); what's left is checking them on real Windows (W4). WSL moved to the follow-ups.

## W2 — Install, release, updates

| # | Item | Plan |
|---|---|---|
| 9 | Exe icon + version info | Resource via `build.rs` (`embed-resource`); must also build in the `cargo xwin` cross-check. Window icon is already set at runtime |
| 10 | Install / uninstall (counterpart of `install.sh`) | Per-user, no admin: `%LOCALAPPDATA%\Programs\Terminaal\terminaal.exe`, Start-menu shortcut with AppUserModelID (taskbar grouping, toast sender), optional Startup entry for the drop-down window; uninstall removes them. Form: `install.ps1` in a release zip, or `terminaal --install/--uninstall` |
| 11 | Release assets | zip (exe, install script, docs) + `.sha256`; the bare exe + `.sha256` for updates. Check the update check picks the asset for the running OS only |
| 12 | Updater location | Update into the install folder (writable per-user); `NotWritable` path tested for a Program Files install |

## W3 — Look (last, as on Linux)

| # | Item | Plan |
|---|---|---|
| 13 | Dark title bar | `DWMWA_USE_IMMERSIVE_DARK_MODE` following the theme's brightness, switched in `apply_theme` |
| 14 | **Transparency** (`opacity`, `ui_opacity`) | The DX12 HWND swapchain is opaque-only. wgpu's DirectComposition presentation gives `PreMultiplied`; then `apply_transparency` works unchanged. Until then the settings grey out with a note instead of doing nothing |
| 15 | **Blur** (`blur`) | DWM backdrop (`DWMWA_SYSTEMBACKDROP_TYPE`: Acrylic/Mica, Windows 11 22H2+) behind the transparent client area — `blur_windows.rs` becomes real |
| 16 | System theme | Counterpart of the COSMIC theme: theme "Windows" from the accent colour and light/dark mode, live on `WM_SETTINGCHANGE` |

## W4 — Tests and verification

| # | Item | Plan |
|---|---|---|
| 17 | Wine runs | All but one test pass under Wine; `glyphs_stay_on_their_cells` fails only because a bare prefix has no fonts — install a font into the throwaway prefix (or skip there). CI on real Windows runs everything |
| 18 | SFTP / sync / edit end-to-end tests | Today `cfg(unix)` (need `sftp-server`). Run them on Windows against Win32-OpenSSH's `sftp-server.exe` (downloaded in CI), plus the `ReadDirectoryChangesW` watcher tests |
| 19 | Windows unit tests | Hotkey key mapping, default font choice, process-tree title, OSC 9;9, cwd conversion (MSYS/WSL), agent pipe stream |
| 20 | Manual pass in the VM | Checklist below, for every release candidate of the branch |

### Manual checklist (Windows VM)

- Start, splash, settings saved/kept, German/English
- Typing incl. AltGr characters (`@ { [ ] } \ | ~`) on a German layout, dead keys, Ctrl/Alt shortcuts, Kitty keyboard in Neovim
- PowerShell, pwsh, cmd, Git Bash, WSL tabs; prompt marks, exit codes, new tab in the same folder, `-c`, `-s`, `--hold`
- Splits, tabs (drag), search, vi mode, hints, links (URL, `C:\` path), selection/copy/paste, paste warning, OSC 52, images (kitty `icat` over SSH)
- Scrollbar, mouse wheel in less/htop over SSH, DPI 100/125/150 %, monitor change
- SSH: password, key, agent (Windows OpenSSH agent, Pageant, 1Password), jump host, ProxyCommand, forwards incl. SOCKS, reconnect after network loss, host-key dialog/removal
- SFTP tab, transfers, edit with sync-back, folder sync (live), Recycle Bin delete, drag & drop upload
- 5250 against PUB400 (telnet + TLS through schannel)
- Session restore, second instance, update from a local server (`TERMINAAL_UPDATE_URL`), drop-down window (command, hotkey, focus loss, height)
- Notifications for long commands and silence, bell flashes the taskbar

## W5 — Docs and merge

| # | Item |
|---|---|
| 21 | README: Windows install, requirements (Windows 10 1809+ for ConPTY), what differs, known limitations |
| 22 | Guides: configuration (`quake_hotkey`, Windows paths), drop-down window (hotkey, Startup folder), shells/integration (PowerShell, cmd, Git Bash, WSL), SSH hosts (Windows agents), troubleshooting (SmartScreen, Defender, ConPTY), architecture (`src/platform/`) |
| 23 | CLAUDE.md: Windows parts in "Aktueller Stand", module list, known problems |
| 24 | Keep merging `terminaal/main` into the branch; **every new Linux feature adds a row here** — parity is a moving target |
| 25 | Final: tests + clippy on both targets, full VM pass, PR, then a release |

## Follow-ups (not blocking)

- **WSL integration** (moved out of W1 on 2026-11-07: Linux has no counterpart, so it isn't parity; WSL tabs already work as plain terminals, and fish ≥ 4 or Nushell inside send OSC 7/133 themselves). Would be: one entry per distro (`wsl -l -q`, UTF-16) — needs shells with arguments in config/session/`-s`; the distro's login shell with Terminaal's startup file (path via `wslpath`); folders `/mnt/c/…` ↔ `C:\…` and `\\wsl.localhost\<distro>\…`. Can't be tested under Wine or in a VM without nested virtualization
- MSYS2/Cygwin shells (their bash needs the login environment, which `--rcfile` skips); `pwsh` installed from the Store (an app execution alias that `is_file` doesn't see)

- Pageant found by itself for forwarding (its pipe name is derived from the user name and `CryptProtectMemory`), so `ForwardAgent yes` works with it without the `--openssh-config` path

## Not ported, by design

| Linux piece | Windows counterpart |
|---|---|
| Wayland layer surface for the drop-down window | Frameless top-most winit window + `RegisterHotKey` |
| `ext_background_effect_v1` / KWin blur | DWM backdrop (W3 #15) |
| COSMIC theme | Windows theme (W3 #16) |
| `app_id`/`WM_CLASS`, `.desktop` file, `install.sh` | AppUserModelID, Start-menu shortcut, installer (W2) |
| Man page | `terminaal --help` in the calling console |
| inotify, `gio trash`, `flock`, Unix sockets for `--quake` | `ReadDirectoryChangesW`, Recycle Bin, `LockFileEx`, loopback port |

## Open decisions

- **Code signing:** unsigned exes trigger SmartScreen ("Windows protected your PC") on download. Signing costs money/yearly; alternative is documenting "More info → Run anyway" and a winget manifest later.
- **System theme (W3 #16):** required for parity, or is COSMIC-sync Linux-only?
- **Installer form (W2 #10):** script in a zip vs. `terminaal --install` vs. MSI/winget.
