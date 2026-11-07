# Terminaal on Windows

Terminaal runs on Windows 10 version 1809 (build 17763) or newer, 64-bit. It
does what it does on Linux; this page is about what's different: installing,
where things are kept, the shells, SSH agents, the drop-down window and the
look.

## Installing

Download `terminaal-x86_64-windows-setup.exe` from the
[latest release](https://github.com/mergedeyes/terminaal/releases/latest) and
run it. Its first page asks for whom:

- **For all users** (the default): into `C:\Program Files\Terminaal`. Windows
  asks for administrator rights.
- **Only for me**: into `%LOCALAPPDATA%\Programs\Terminaal`, no administrator
  rights needed.

Two options are offered and off by default: a desktop shortcut, and **Start the
drop-down window when I sign in** ([below](#the-drop-down-window)). Terminaal
then is in the Start menu and under **Settings → Apps**.

**"Windows protected your PC"**: Terminaal isn't signed with a certificate, so
SmartScreen warns about a setup downloaded with a browser. Click **More info**,
then **Run anyway**.

**Without installing**: `terminaal-x86_64-windows.exe` from the same release
runs from any folder of yours, as it is.

## Updates

Terminaal offers new releases at start, as on Linux ([Updates](updates.md)). How
it installs one depends on how it got there:

- **Installed by the setup**: it downloads the new setup, checks it, and runs it
  without its pages once Terminaal ends – after **Restart now**, or when you
  simply close it later. Then Terminaal starts again. An install for all users
  asks for administrator rights at each update.
- **A plain `terminaal.exe`** of yours is replaced in place.
- A plain exe copied into `C:\Program Files` by hand can't be: Terminaal says so
  and points to the setup.

## Uninstalling

**Settings → Apps → Terminaal → Uninstall** removes the program and its
shortcuts, then asks whether to delete your settings as well (no, unless you
say so). Your SSH keys in `%USERPROFILE%\.ssh` stay either way.

## Where things are

| What | Where |
| --- | --- |
| `config.toml`, `hosts.toml`, `keys.toml`, `snippets.toml`, `themes\` | `%APPDATA%\terminaal` |
| The saved session, folder-sync records | `%LOCALAPPDATA%\terminaal` |
| SSH keys, `known_hosts`, `config` | `%USERPROFILE%\.ssh` |

Wherever the other pages say `~/.config/terminaal`, read `%APPDATA%\terminaal`.
The [configuration reference](configuration.md) has two keys only Windows reads:
`maximized` and `quake_hotkey`.

## Shells

Terminaal finds PowerShell 7 (`pwsh`, also the preview), Windows PowerShell,
cmd, Git Bash, Nushell and WSL, plus `pwsh` and `nu` anywhere on the `PATH`.
The default is the first that isn't cmd; change it in the sidebar's **Shells**.

What the [shell integration](shell-integration.md) gives you, per shell:

| Shell | Prompt marks | Exit codes, run times, notifications | Folder for new tabs |
| --- | --- | --- | --- |
| PowerShell (both) | yes | yes | yes |
| Git Bash | yes | yes | yes |
| cmd | yes | no – cmd tells nothing before a command runs | yes |
| WSL | only from a shell that sends them itself (fish 4, Nushell) | the same | the same |

Aliases, functions and lines for PowerShell (both) go in a file of Terminaal's
own, `Documents\PowerShell\terminaal.ps1`, loaded only in Terminaal; your
profile isn't touched.

The tab title names the program running in it, as on Linux; programs inside WSL
are Linux processes and don't show.

The built-in command buttons (winget updates, disks, services, …) are there for
PowerShell and cmd tabs, and for SSH hosts running Windows' OpenSSH server – not
for WSL or Git Bash tabs.

Negative exit codes, which Windows programs use for their errors, show in hex:
`✘ 0x8A15002C`.

## SSH agents

Windows' own OpenSSH agent listens on the pipe `\\.\pipe\openssh-ssh-agent`, and
Terminaal uses it whenever `SSH_AUTH_SOCK` isn't set. 1Password, Bitwarden and
KeePassXC can offer their keys on that same pipe – switch on their "SSH agent"
option and they work without more.

**Pageant** has a pipe of its own: run `pageant --openssh-config <file>` once,
and put the pipe path written into that file as `IdentityAgent` for your hosts
(in `~/.ssh/config` or under the host's **Advanced**). Agent forwarding
(`ForwardAgent`) works with all of them.

`ProxyCommand` runs through `cmd /C`. Unix-socket forwards work with `C:\…`
paths.

## The drop-down window

`terminaal --quake` shows a terminal along the top of the screen and hides it
again ([The drop-down window](drop-down-window.md)). On Windows it registers a
global shortcut of its own once it runs: <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>T</kbd>
by default, changed under **Settings → General → Drop-down window → Shortcut**
(`quake_hotkey`, empty for none). To have it ready after signing in, tick the
setup's option, or put `terminaal --quake` in your Startup folder
(<kbd>Win</kbd>+<kbd>R</kbd>, `shell:startup`).

## The look

- **The title bar** is dark with a dark theme and light with a light one,
  whatever Windows' own mode is.
- **The theme "Windows"** follows Windows' light or dark app mode and its accent
  colour, also when you change them under **Settings → Personalization →
  Colors**. The console then has Windows Terminal's colors.
- **Translucency** (**Opacity**, **Interface opacity**) works as on Wayland.
- **Blur** is Windows' Acrylic. It needs Windows 11 22H2 or newer, a graphics
  card (Windows draws it as a plain colour without hardware acceleration, as in
  many virtual machines), and **Transparency effects** switched on under
  **Settings → Personalization → Colors**. Where one is missing, the settings
  say which, and the window is see-through without blur.

**Start maximized** under **Settings → General → Window size** (`maximized`)
opens the window maximized; Terminaal notes it whenever you maximize or restore
the window, so it opens the way you left it.

## Notifications

Long commands and silent tabs notify with a Windows toast, named Terminaal when
the setup installed it (a plain exe's come under Windows PowerShell's name). The
bell flashes the taskbar button.

## When something's wrong

- **The window stays empty or black.** Terminaal draws through DirectComposition
  so the window can be see-through. Should a graphics driver show nothing that
  way, start it with the plain path – opaque only:
  ```powershell
  $env:WGPU_DX12_PRESENTATION_SYSTEM = "hwnd"; terminaal
  ```
- **A tab opens and closes again.** A shell that fails within five seconds of
  starting keeps its tab with what it printed – read it there. WSL in a virtual
  machine usually needs nested virtualization.
- **Logs**: Terminaal is a window program, but started from a console it writes
  there:
  ```powershell
  $env:RUST_LOG = "terminaal=info"; terminaal
  ```
- **An update didn't install**: the setup's log is in `%TEMP%\terminaal\update`.

More in [Troubleshooting](../troubleshooting.md).
