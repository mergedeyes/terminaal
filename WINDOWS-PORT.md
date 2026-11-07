# Windows port — roadmap

Branch `feature/windows-port`. **Merge rule:** this branch goes into `main` only
once Windows has the full working feature list of the Linux build — no early
"it builds" merge (a release would offer a half-working Windows exe as an
update). This file lives only on the branch; before the merge its content moves
into README/docs and the file is deleted.

Order follows the project's own: functionality first, then install/UX, then
visual polish last.

Status as of 2026-11-07 (night): W1 closed in code, first manual tests in a
Windows 11 VM under way. **W2 done in the VM** for the setup (Program Files
default and per user), updates through the setup and uninstall; left in W2:
a few small checks (exe icon/details, taskbar grouping, toast sender, bare-exe
update, setup options; folded into the next VM round) and the first real
release run. **W3 (look) in code:** dark title bar, see-through window
through DirectComposition, Acrylic blur, theme "Windows"; next is checking
them in the VM. Branch up to date with `terminaal/main` at `25ddcdb`.

## Progress log

| Date | What happened |
|---|---|
| 2026-11-06 | Port started: builds for Windows, platform layer, ConPTY, PowerShell integration, SSH agent + Ed25519, SFTP watching, notifications, updates, release exe in CI |
| 2026-11-07 | Roadmap written. Drop-down window with global shortcut, default font. **W1 closed in code:** agent forwarding to named pipes, Unix-socket forwards, program in the tab title, cmd integration, Git Bash, more shells, Windows SSH servers recognized (WSL moved to follow-ups) |
| 2026-11-07 | CI on the branch green on real Windows (313 tests); dependency cache added to the Windows workflow |
| 2026-11-07 | First Windows 11 VM session (Insider build): found and fixed — exit codes shown as negative numbers (now hex), tab/window title stuck at `powershell.exe`'s path after winget, window didn't start maximized (new option `maximized`). All three confirmed in the VM |
| 2026-11-07 | CI with the cache: under 3 minutes. Decided for W2: a real installer (see W2) |
| 2026-11-07 | **W2 in code:** Inno Setup script (per user by default, all users optional, Start menu shortcut with AppUserModelID, Apps entry, drop-down window at sign-in, settings kept on uninstall unless asked), exe icon + version info via `build.rs` (also under cargo xwin), installed copies update by running the new setup silently, CI artifact + release assets. `TERMINAAL_UPDATE_URL` takes `file:///` for update tests in the VM. Setup compiled only in CI so far (running Inno Setup under Wine locally was blocked) |
| 2026-11-07 | VM: the Program Files setup (CI run 42386830255) works: install for all users with UAC, update through the silent setup with UAC, uninstall. **W2 closed in the VM** apart from the small checks in the W2 table |
| 2026-11-07 | VM: the first CI setup (per user) installed and updated itself through the setup from the `file:///` test release without a hitch. No SmartScreen prompt — expected, files from the `Z:` share carry no Mark of the Web; a browser download from GitHub will still get the warning |
| 2026-11-07 | First VM look at the setup: it defaulted to `%LOCALAPPDATA%\Programs`. Changed: **Program Files for all users by default** (UAC), "only for me" stays offered on the first page |
| 2026-11-07 | W2's first CI run green in 4:20 min: 319 tests on real Windows, the setup compiles without warnings in 8 s (10 MB, LZMA2). Setup, exe and an update test release (`~/Downloads/terminaal-update/`, tag v99.0.0) ready for the VM |
| 2026-11-07 | **W3 in code** (#13–#16): wgpu 30 can present through DirectComposition (`Dx12SwapchainKind::DxgiFromVisual`, surface offers `PreMultiplied`), now the default on Windows, with a window without redirection bitmap; DXGI can't change a swapchain's alpha mode, so the surface is premultiplied from the start and opaque frames draw alpha 1. Title bar dark/light with the theme (`DWMWA_USE_IMMERSIVE_DARK_MODE`), Acrylic backdrop for `blur` (Windows 11 22H2+), theme "Windows" decided as parity and built (registry + notification thread). Cross-checked with clippy, 323 tests under Wine (one known font failure); not yet seen on real Windows |
| 2026-11-07 | VM (software rendering: `Microsoft Basic Render Driver`, DX12, surface offers `PreMultiplied`): **transparency works** (opacity 30 %, panels 50 %, desktop shows through). Blur on turned the window solid light grey: Acrylic's fallback colour while Windows' transparency effects are off. Fixed: the backdrop is only set while `EnableTransparency` is on (read live through the theme watcher), else blur on = plain see-through, with the note `settings-blur-effects-off` |
| 2026-11-07 | VM: transparency effects were on after all, yet Windows' own Acrylic (desktop context menu) and Mica (Explorer) are solid there too: DWM doesn't blur on software rendering. Blur stays unverified until a machine with a GPU; the effects-off fix stays (right for that case) |
| 2026-11-07 | Confirmed: Windows' own Acrylic needs graphics hardware acceleration. So with a software adapter (`DeviceType::Cpu`, WARP) Terminaal offers no blur either — blur on would only turn the window solid — and the settings say why (`settings-blur-software-windows`) |
| 2026-11-07 | VM: the drop-down window typed a `t` when shown with Ctrl+Alt+T — winit's synthetic presses for keys held on focus. Fixed: only real key events reach the terminal (also helps X11 on Linux) |
| 2026-11-07 | VM checklist: typing, shells (not WSL: the tab closes at once, likely no nested virtualization), splits, links, 5250, drop-down window, session restore work. W4 #18 in code: SFTP/edit/sync tests on Windows against Git for Windows' `sftp-server` in CI |
| 2026-11-07 | CI run 42394445057 green: 352 tests, then the 33 SFTP/edit/sync tests against Git for Windows' `sftp-server` on the first try (transfers both ways, resume after a dropped connection, edits with conflicts, hash conflicts, live sync, busy lock). **W4 #18 done** |
| 2026-11-07 | A tab whose shell fails within 5 s of starting stays with its status (`kept_after_exit`), so WSL's error in the VM can be read: built on `feature/keep-failed-tab` from `main` (PR to `main` when you say so), merged here |
| 2026-11-07 | **W5 docs written**: `docs/guides/windows.md` and Windows parts in README and nine pages; CLAUDE.md section on the port |

### How builds get into the VM

- Full build (with OpenSSL, so Ed25519 works): CI artifact `terminaal-x86_64-windows` → `~/Downloads/terminaal.exe`. Only CI can build it (OpenSSL needs Perl + nmake)
- Setup: CI artifact `terminaal-x86_64-windows-setup` → `~/Downloads/terminaal-x86_64-windows-setup.exe`
- Quick local build (no OpenSSL, no Ed25519): `cargo xwin build --release --target x86_64-pc-windows-msvc --no-default-features` → `~/Downloads/terminaal-no-openssl.exe`
- Update test release: `~/Downloads/terminaal-update/` with `latest.json` (assets as `file:///Z:/terminaal-update/…`), started with `TERMINAAL_UPDATE_URL=file:///Z:/terminaal-update/latest.json`
- The VM sees `~/Downloads` read-only through virtiofs (WinFsp), usually as `Z:`; copy the exe or setup out before starting it

## Done

| Area | What works on Windows |
|---|---|
| Build & CI | `src/platform/` holds the OS differences (stream pairs: loopback TCP + `WSAPoll`; locks; permissions; links via `explorer.exe`; trash = Recycle Bin; local time). CI on `windows-latest` for the branch (and PRs, not `main` pushes until the merge): clippy, tests, release exe and setup as artifacts; dependencies cached (`Swatinem/rust-cache`): a run went from ~29 min to under 3 (clippy 35 s, tests 28 s, release build 52 s); a new `Cargo.lock` or Rust release fills the cache once more. A newer push cancels a running one. Release workflow builds the Windows exe and setup on `v*` tags only |
| Startup | Window program without a console; `--help`/`--version`/errors/`RUST_LOG` go to the parent console (`AttachConsole`). Settings in `%APPDATA%\terminaal`, state in `%LOCALAPPDATA%\terminaal`. German UI from the Windows locale |
| Local terminal | ConPTY; shell-integration filter inside ConPTY reads; shells found: pwsh 7 (and preview), Windows PowerShell, cmd, Git Bash (machine or per-user), Nushell, WSL, plus `pwsh`/`nu` anywhere on the `PATH` (default: first non-cmd) |
| Shell integration | PowerShell: OSC 7 + OSC 133 (prompt marks, exit codes, durations, notifications). Git Bash: the bash integration with `/` paths for `--rcfile` and the managed file; MSYS (`/c/…`) and Cygwin (`/cygdrive/c/…`) folders from OSC 7 become `C:\…`; the launcher's `usr\bin\bash` child counts as the shell. Untested in a real Git Bash. cmd: `PROMPT` set at launch wraps the user's own prompt (else `$P$G`) in OSC 133 A/B and sends the folder as OSC 9;9, which the filter now reads (all platforms); no C/D (cmd has no pre-exec hook): no durations, exit codes, notifications, output marks. Only through tests so far, not in a real cmd |
| Sidebar | Aliases, functions and lines for PowerShell (profile-loaded file of Terminaal's own) |
| Built-in commands | Systems `windows` (PowerShell) and `windows-cmd`: winget updates, disks, folder sizes, memory, processes, uptime, failed services, log errors, ports, addresses; usable for SSH hosts via `system`, and recognized by themselves: when the Unix probe finds nothing, `echo %OS% $PSVersionTable` tells cmd (`windows-cmd`) from PowerShell (`windows`) — Linux builds get this too. Not yet against a real Windows OpenSSH server |
| SSH | Agent auth (Pageant, Windows OpenSSH agent pipe), Ed25519 keys, ProxyCommand via `cmd /C`. Agent forwarding to named pipes (`ssh::agent_pipe`): Windows' OpenSSH agent when `SSH_AUTH_SOCK` is unset (also 1Password, Bitwarden, KeePassXC), any pipe in `IdentityAgent`/`ForwardAgent` (Pageant: the path `pageant --openssh-config` writes); a thread relays message by message between the pipe and a socket pair. Tested against a fake agent on a real pipe (Wine and CI); not yet against a real server |
| SFTP | Edit-and-sync-back and live folder sync with `ReadDirectoryChangesW`; excludes with `\` paths |
| Unix-socket forwards | `AF_UNIX` (Windows 10 1803+) through `platform::UnixListener`/`UnixStream` (own WinSock wrapper, polled with `WSAPoll`): local forwards listening on a socket, targets that are sockets, stale sockets replaced (reparse tag `IO_REPARSE_TAG_AF_UNIX`), only our own file removed (NTFS file index); `C:\…` paths in forwards. Wine has no `AF_UNIX` (tests skip there); passed in CI on real Windows (a skip would also show as passed, but the runners have `AF_UNIX`). Not yet with a real forward in the VM |
| Program in the tab title | `platform::running_program`: the shell's newest child process by creation time (Toolhelp32 snapshot), console hosts and `wslhost` skipped, children older than their parent ignored (reused PIDs), a child named like the shell taken as the shell behind a launcher (Git Bash). Same 250 ms poll as on Linux. Programs inside WSL stay invisible (they're Linux processes); PowerShell background jobs would show as `pwsh` |
| Exit codes | Negative codes (Windows' HRESULT/NTSTATUS, which PowerShell's `$LASTEXITCODE` holds signed) shown in hex at the prompt, in `--hold` and in notifications: `✘ 0x8A15002C`, not `✘ -1978335188`. 0–255 stay decimal. Seen in the VM with `winget upgrade --all` |
| Window and tab title | A console's default title (the shell's path, maybe after `Administrator: `) and an empty title count as none, so the folder shows again after programs like winget restore it; cmd's `…\cmd.exe - <command>` shows as the command |
| Window | `maximized` in `config.toml` (Windows only): starts maximized, noted whenever the window is maximized or restored; checkbox under Settings → General → Window size |
| Links & hints | File paths with drive letters (`C:\…`), runnable extensions open their folder instead |
| Notifications, updates | Toast notifications, under Terminaal's own AppUserModelID when installed by the setup (else PowerShell's). Updates: a copy the setup installed (`update::Installation::of`: the setup's Apps entry names the exe's folder, HKCU then HKLM) downloads `terminaal-x86_64-windows-setup.exe`, checks it and runs it with `/VERYSILENT … /CURRENTUSER` or `/ALLUSERS` (UAC) once Terminaal ends — "Restart now" or simply closing it — then starts Terminaal (and a drop-down window of that install that the setup closed) again; a bare copy elsewhere keeps the in-place swap (running exe renamed to `.old`, cleaned up next start). A hand copy in Program Files gets a Windows-specific "can't write" note |
| Setup | `installer/terminaal.iss` (Inno Setup 6, preinstalled on `windows-latest`): for all users by default in Program Files (UAC), "only for me" offered on the first page (`%LOCALAPPDATA%\Programs\Terminaal`, no admin rights); Start menu shortcut with AppUserModelID `Terminaal.Terminaal` and start folder `%USERPROFILE%`; optional desktop shortcut and drop-down window at sign-in (`{userstartup}`, `--quake`); entry under Settings → Apps with version and uninstall; closes a running Terminaal of the install (Restart Manager); English/German. Uninstall removes program and shortcuts and asks (default No, never when silent) whether to delete `%APPDATA%\terminaal` and `%LOCALAPPDATA%\terminaal`. `AppId` is fixed (`update::SETUP_APP_ID`, a test keeps script, workflow and program in step) |
| Exe resources | `build.rs` writes a resource script (icon from `assets/terminaal.ico`, `VERSIONINFO` from Cargo.toml) and compiles it with `embed-resource`: `rc.exe` in CI, `llvm-rc` under cargo xwin |
| Command line | `-c` with cmd, `-s` with a Windows path |
| Drop-down window | `terminaal --quake`: lock + loopback port in the temp folder, second start toggles and hands over the foreground; global shortcut `quake_hotkey` (default Ctrl+Alt+T, `RegisterHotKey`); frameless, on top, not in the taskbar, height slider live, hides on focus loss |
| Fonts | Default console font Cascadia Mono → Consolas → Lucida Console → Courier New (cosmic-text's Noto Sans Mono isn't on Windows) |
| Look (W3, code only so far) | Frames through DirectComposition (`gpu::composition`: wgpu's `DxgiFromVisual`; the window is made without redirection bitmap, which would cover the visual); the surface is `PreMultiplied` from the start (DXGI keeps a swapchain's alpha mode, wgpu's `configure` only resizes) and `GpuState` tracks "see-through" on its own, so `opacity`/`ui_opacity` work through the unchanged `apply_transparency`. `WGPU_DX12_PRESENTATION_SYSTEM=hwnd` falls back to the plain swapchain (opaque, settings greyed out with the existing note). Title bar: `blur::set_dark_title_bar` in `apply_theme` (`DWMWA_USE_IMMERSIVE_DARK_MODE` by the theme's brightness, only on a change, `SWP_FRAMECHANGED` for Windows 10's repaint); the window gets a winit theme at creation, else winit 0.30 resets the title bar to the system mode on every `WM_SETTINGCHANGE`. Blur: `blur_windows.rs` extends the frame into the whole client area and sets `DWMWA_SYSTEMBACKDROP_TYPE` = Acrylic (`DWMSBT_TRANSIENTWINDOW`), off again = margins 0 + `DWMSBT_AUTO`; only from build 22621 (`RtlGetVersion`), else the checkbox greys out with "Blur needs Windows 11 22H2 or newer". Theme "Windows" (`theme::windows`): `AppsUseLightTheme`, `AccentPalette` (light 2 on dark, dark 1 on light, as WinUI's accent fill), DWM's `AccentColor` as fallback; Campbell / One Half Light (Windows Terminal's defaults) for the console, Windows 11 greys for the chrome; `watch` = thread on `RegNotifyChangeKeyValue` for three keys, 200 ms settle → `UserEvent::DesktopThemeChanged` (was `CosmicThemeChanged`) |

## W1 — Functional gaps (blocking the merge)

All closed in code (2026-11-07); what's left is checking them on real Windows (W4). WSL moved to the follow-ups.

| # | Gap | State |
|---|---|---|
| 1 | Agent forwarding | Code + tests (fake agent on a real pipe, CI). VM: open |
| 2 | Unix-socket forwards | Code + tests (CI). VM: open |
| 3 | Program in the tab title | Code + tests (real child process, CI). VM: open |
| 4 | cmd integration | Code + tests. VM: open |
| 5 | Git Bash | Code + tests. VM: open (needs Git for Windows installed) |
| 7 | More shells | Code. VM: open |
| 8 | Windows SSH servers recognized | Code + tests. Needs a Windows OpenSSH server to try |

## W2 — Install, release, updates

**Decided 2026-11-07: a real installer**, like other Windows programs — a setup
that puts Terminaal in the Start menu and under Settings → Apps (with
uninstall), not a script or `terminaal --install`. Settled the same day:
**Inno Setup** (a `setup.exe`; both Inno Setup 6.7.1 and WiX 3.14 are on
`windows-latest`, Inno makes per-user installs, the AppUserModelID and custom
questions simple; its compiler would run under Wine, but installing it there
was blocked, so only CI compiles the setup), **Program Files for all users by
default, "only for me" optional** (first decided the other way round; changed
after the first look in the VM: a real program goes to Program Files), and **updates of installed copies run the new setup
silently** (the common practice, VS Code's user setup does the same: the
setup keeps its own Apps entry, files and uninstall log in step, and only an
elevated setup can update a Program Files install). Bare exes keep the
in-place swap.

| # | Item | State |
|---|---|---|
| 9 | Exe icon + version info | Code: `build.rs` + `embed-resource`, checked in the cross build (`llvm-readobj`: icon group, `VERSIONINFO` 0.3.1). VM: open (Explorer icon, Properties → Details) |
| 10 | Installer (counterpart of `install.sh`) | Done: `installer/terminaal.iss`, built by CI (artifact `terminaal-x86_64-windows-setup`). VM: install for all users (default) and per user, Start menu, Apps entry, uninstall confirmed. VM, still open: taskbar grouping with a pinned entry, toast sender "Terminaal", desktop shortcut and drop-down window at sign-in options |
| 11 | Release assets | Code (open: first real run with a `v*` tag): `release.yml` publishes setup + `.sha256` next to the bare exe + `.sha256`; `update::Installation::asset` picks the setup for installed copies, the bare exe otherwise, only ever for its own OS (test `each_system_picks_only_its_own_download`). First real run with the next `v*` tag |
| 12 | Updater location | Done (VM: per user without prompt, Program Files with UAC; bare-exe swap outside the install still to try): installed copies update through the setup (all users — the default — with a UAC prompt at each update, also when it runs on closing Terminaal; per user without prompt); bare copies swap in place; a hand copy in Program Files gets `update-not-writable-windows`. Tests: staging the checked setup, setup arguments (Wine). VM: open (update from a `file:///` release on `Z:`, Apps entry afterwards, decline UAC) |

Not done: code signing (see Open decisions) — SmartScreen warns about the
setup as about the bare exe. RM closing a running main window doesn't save
the session on the spot (winit has no `WM_ENDSESSION`); it's saved 2 s after
every change anyway. A downloaded setup that never ran (Terminaal killed)
stays in `%TEMP%\terminaal\update` until Windows cleans the temp folder.

## W3 — Look (last, as on Linux)

| # | Item | State |
|---|---|---|
| 13 | Dark title bar | Code (see Done → Look): `DWMWA_USE_IMMERSIVE_DARK_MODE` by the theme's brightness, switched live in `apply_theme`. VM: open (dark and light theme, switching while the window is active and inactive, Windows' own mode changed meanwhile must not flip it) |
| 14 | **Transparency** (`opacity`, `ui_opacity`) | Code: DirectComposition presentation (`DxgiFromVisual`) by default, premultiplied surface from the start; `apply_transparency` unchanged. **VM: works** (opacity and panel opacity, software rendering). Still to see: opaque at 100 % with no see-through gaps, resizing, maximized, DPI change, the drop-down window |
| 15 | **Blur** (`blur`) | Code: Acrylic system backdrop behind the extended frame (Windows 11 22H2+ with a hardware GPU, else not offered, with a note), only while Windows' transparency effects are on (else plain see-through + note, live). VM: can't be checked there: with transparency effects on, Windows' own Acrylic and Mica (context menu, Explorer) are solid too — DWM doesn't blur on the VM's software rendering. Needs a machine with a real GPU: on/off live, together with opacity, inactive window, effects switched off live (note + plain see-through) |
| 16 | System theme | **Decided 2026-11-07: part of parity.** Code: theme "Windows" from mode + accent colour, live through registry notifications, tests (accent shades, fallbacks, registry read under Wine). VM: open (switch Windows' mode and accent colour under Settings → Personalization → Colors while Terminaal shows the theme) |

## W4 — Tests and verification

| # | Item | Plan |
|---|---|---|
| 17 | Wine runs | All but one test pass under Wine (318 of 319 after W2); `glyphs_stay_on_their_cells` fails only because a bare prefix has no fonts — install a font into the throwaway prefix (or skip there). CI on real Windows: all 319 pass (2026-11-07, with W2) |
| 18 | SFTP / sync / edit end-to-end tests | **Done** (2026-11-07): 33 SFTP tests pass on real Windows in CI (run 42394445057, step "SFTP end-to-end tests", against the server). How: the test modules run on Windows (pure tests under Wine too: 351 of 352 pass, found three test-only Unix assumptions). The server is Git for Windows' MSYS `sftp-server.exe` with its `sh`/`sha256sum` (`TERMINAAL_TEST_SFTP_SERVER`/`TERMINAAL_TEST_SH`, set only by CI's last step "SFTP end-to-end tests"); `remote_path` maps `C:\x` → `/c/x`; the server's pipe is relayed into a socket (Windows polls only sockets). Unix-only still: sudo (fake `sudo` takes away Unix modes), permission denied, a symlink in a removed tree, Unix modes. Not against Win32-OpenSSH (paths `/C:/…`, no `sh` for the commands); running Git's server under Wine locally was blocked, so only CI can run them |
| 19 | Windows unit tests | Done: agent pipe, process tree, Unix sockets, OSC 9;9, MSYS/Cygwin folders, console titles, exit code labels, update through the setup (staging the checked setup, its arguments, a copy outside the install is standalone, download per OS and install type, setup script/workflow/program in step). Open: hotkey key mapping, default font choice; finding an install in the registry (only in the VM — a test would write to the real HKCU) |
| 20 | Manual pass in the VM | Checklist below, for every release candidate of the branch. Started 2026-11-07 (Windows 11 Insider): ticked items below |

### Manual checklist (Windows VM)

- [x] Starts on a fresh Windows 11, German UI
- [x] PowerShell 5.1 tab: prompt marks, durations (`5,2 s`), `✘` exit codes
- [x] Sidebar: shells (powershell, cmd, wsl), PowerShell aliases/functions/lines, built-in commands for Windows (winget)
- [x] Title after winget, hex exit codes, start maximized (fixed and confirmed)
- [ ] Splash, settings saved/kept, English
- [x] Typing incl. AltGr characters (`@ { [ ] } \ | ~`) on a German layout, dead keys, Ctrl/Alt shortcuts, Kitty keyboard in Neovim (2026-11-07: "typing works")
- [x] PowerShell, pwsh, cmd, Git Bash tabs; prompt marks, exit codes, new tab in the same folder, `-c`, `-s`, `--hold` (2026-11-07: "shells work")
- [ ] WSL tab: after installing WSL the tab opens and closes at once (2026-11-07) — most likely WSL 2 can't start without nested virtualization in the QEMU VM; to see its message: `terminaal -s wsl --hold`
- [x] Splits, links (2026-11-07)
- Tabs (drag), search, vi mode, hints, selection/copy/paste, paste warning, OSC 52, images (kitty `icat` over SSH)
- Scrollbar, mouse wheel in less/htop over SSH, DPI 100/125/150 %, monitor change
- SSH: password, key, agent (Windows OpenSSH agent, Pageant, 1Password), jump host, ProxyCommand, forwards incl. SOCKS, reconnect after network loss, host-key dialog/removal
- SFTP tab, transfers, edit with sync-back, folder sync (live), Recycle Bin delete, drag & drop upload
- [x] 5250 against PUB400 (telnet + TLS through schannel) (2026-11-07)
- [x] Session restore, drop-down window (2026-11-07; the hotkey typed a `t` in the window, fixed in 645a32c, to confirm)
- Second instance, drop-down window details (focus loss, height)
- Exe: icon in Explorer and taskbar, Properties → Details (version 0.3.1, product name)
- Look (W3): title bar dark with a dark theme and light with a light one, switching live (theme picker, also with the window inactive); opacity slider and the panels' opacity live, 100 % fully opaque; blur on/off live (Acrylic, solid while inactive); theme "Windows" follows Windows' mode and accent colour live; all of it after a DPI change (100 → 150 %), maximized, and in the drop-down window
- Setup default (all users): first page offers "for all users" preselected, UAC, `C:\Program Files\Terminaal`, Start menu entry with icon, first tab in the home folder, taskbar groups with a pinned Start menu entry, toast says "Terminaal" (not Windows PowerShell), Settings → Apps shows Terminaal 0.3.1 with icon; German setup on a German Windows
- Setup options: desktop shortcut, drop-down window at sign-in (sign out and in); setup over a running Terminaal (closes it, "Launch Terminaal" afterwards)
- [x] Setup "only for me" (first CI setup, then still the default): no UAC prompt, `%LOCALAPPDATA%\Programs\Terminaal`, Apps entry for this user only
- [x] Setup default (all users) and update of it with UAC (2026-11-07, reported working)
- [x] Update of the per-user install with `TERMINAAL_UPDATE_URL=file:///Z:/terminaal-update/latest.json`: offered, setup downloaded, installed silently, Terminaal back
- Update details with the same test release: installed copy offers it, downloads the setup, "Restart now" asks UAC, installs silently and starts again (drop-down window comes back); "Later" + close installs too (UAC after closing); declining UAC keeps the old version; a per-user install updates without a prompt; bare exe outside still swaps in place
- Uninstall from Settings → Apps: program and shortcuts gone; settings question (No keeps `%APPDATA%\terminaal`, Yes removes both folders)
- Notifications for long commands and silence, bell flashes the taskbar

## W5 — Docs and merge

| # | Item |
|---|---|
| 21 | **Done (2026-11-07)**: README – status line, Windows install paragraph (setup / plain exe, SmartScreen), Windows limitations under Known limitations, Windows in Roadmap → Done, link in Documentation |
| 22 | **Done (2026-11-07)**: new `docs/guides/windows.md` (install, updates, uninstall, folders, shells and integration, SSH agents, drop-down shortcut, look, notifications, troubleshooting), index entry; Windows parts in getting started, configuration (`maximized`, `quake_hotkey`, `%APPDATA%`), updates (assets, setup updates), drop-down window, appearance, shell integration, SSH hosts (pipes as `IdentityAgent`), troubleshooting, architecture (module map). To reread against the code before the merge |
| 23 | Partly (2026-11-07): CLAUDE.md has a marked section on the port (modules, rendering, where the details are). Before the merge: Windows into "Aktueller Stand", module list, known problems |
| 24 | Keep merging `terminaal/main` into the branch; **every new Linux feature adds a row here** — parity is a moving target |
| 25 | Final: tests + clippy on both targets, full VM pass, PR, then a release |

## Follow-ups (not blocking)

- ~~A shell that ends with an error within 5 s of starting keeps its tab open~~ — done on its own branch `feature/keep-failed-tab` (from `main`, for Linux too) and merged into this one; to check with `wsl` in the VM (the kept tab should show WSL's message, status in hex)
- **WSL integration** (moved out of W1 on 2026-11-07: Linux has no counterpart, so it isn't parity; WSL tabs already work as plain terminals, and fish ≥ 4 or Nushell inside send OSC 7/133 themselves). Would be: one entry per distro (`wsl -l -q`, UTF-16) — needs shells with arguments in config/session/`-s`; the distro's login shell with Terminaal's startup file (path via `wslpath`); folders `/mnt/c/…` ↔ `C:\…` and `\\wsl.localhost\<distro>\…`. Can't be tested under Wine or in a VM without nested virtualization
- MSYS2/Cygwin shells (their bash needs the login environment, which `--rcfile` skips); `pwsh` installed from the Store (an app execution alias that `is_file` doesn't see)
- Save the session when the Restart Manager closes a running Terminaal (setup over a running window): needs `WM_QUERYENDSESSION`/`WM_ENDSESSION`, which winit doesn't pass on (a window subclass would)
- winget manifest pointing at the setup (also a way around SmartScreen for winget users)
- Pageant found by itself for forwarding (its pipe name is derived from the user name and `CryptProtectMemory`), so `ForwardAgent yes` works with it without the `--openssh-config` path

## Not ported, by design

| Linux piece | Windows counterpart |
|---|---|
| Wayland layer surface for the drop-down window | Frameless top-most winit window + `RegisterHotKey` |
| `ext_background_effect_v1` / KWin blur | DWM backdrop (W3 #15) |
| COSMIC theme | Theme "Windows" (W3 #16, `theme::windows`) |
| `app_id`/`WM_CLASS`, `.desktop` file, `install.sh` | AppUserModelID, Start-menu shortcut, Inno Setup installer (W2) |
| Man page | `terminaal --help` in the calling console |
| inotify, `gio trash`, `flock`, Unix sockets for `--quake` | `ReadDirectoryChangesW`, Recycle Bin, `LockFileEx`, loopback port |

## Open decisions

- **Code signing:** unsigned exes trigger SmartScreen ("Windows protected your PC") on download — the setup as much as the bare exe (not seen in the VM only because files from the shared folder have no Mark of the Web; to see what users see, download the setup with Edge from a release); silent updates run without it (downloaded by Terminaal, no Mark of the Web). Signing costs money/yearly (an OV certificate no longer skips SmartScreen by itself; Azure Trusted Signing is the cheap route if the account qualifies); alternative is documenting "More info → Run anyway" and a winget manifest later. Inno Setup can sign setup and uninstaller (`SignTool=`) once there is a certificate.
