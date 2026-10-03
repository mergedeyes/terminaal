<p align="center">
  <img src="assets/terminaal_logo.png" alt="Terminaal logo" width="160">
</p>

<h1 align="center">Terminaal</h1>

<p align="center">
  A GPU-rendered terminal emulator for COSMIC with a built-in SSH manager.<br>
  Written in Rust. English and German UI.
</p>

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#getting-around">Getting around</a> ·
  <a href="#configuration">Configuration</a> ·
  <a href="docs/README.md">Documentation</a> ·
  <a href="#roadmap">Roadmap</a>
</p>

---

Terminaal combines an everyday terminal with Termius-style host management, in
the spirit of Tabby/Terminus. Open local shells and SSH sessions side by side in
tabs and split panes, keep your hosts, logins and keys in a sidebar, work with a
server's files over the same connection – and never have a secret written to
disk.

> [!NOTE]
> **Status:** under active development and used as a daily driver on Linux
> (CachyOS, Wayland and X11). Expect rough edges, and expect the config format
> to change.

> [!IMPORTANT]
> **Disclaimer:** this project is coded mostly by Claude Code. I make all the
> decisions, though – every feature was planned by me.

<img width="2557" height="1383" alt="Terminaal with local and SSH tabs and the sidebar" src="https://github.com/user-attachments/assets/95a72250-0e38-4be0-86cd-d9cbce21ce06" />
<img width="2557" height="1383" alt="Terminaal with split panes" src="https://github.com/user-attachments/assets/5d820773-2745-474c-bb17-9327a6339164" />

## Contents

- [Highlights](#highlights)
- [Features](#features)
  - [Terminal](#terminal) · [Shell integration](#shell-integration) · [SSH](#ssh) · [Files on the server](#files-on-the-server-sftp) · [Shells and commands](#shells-and-commands) · [Keys](#keys) · [Look and feel](#look-and-feel)
- [Installation](#installation)
- [Getting around](#getting-around) – shortcuts, mouse, command line
- [Configuration](#configuration)
- [Documentation](#documentation)
- [Known limitations](#known-limitations)
- [Roadmap](#roadmap)
- [Contributing](#contributing)

## Highlights

- **Fast and quiet:** a custom wgpu renderer that redraws only when something changes
- **SSH manager built in:** hosts, several logins per host, keys, jump hosts, port forwards, automatic reconnect
- **Files over the same connection:** browse, copy, edit locally and **keep folders in sync** – no second login
- **Split panes, broadcast, command palette** for working on many terminals at once
- **Shell integration** without setup for fish, bash and zsh: prompt jumps, exit codes, run times, notifications
- **Nothing secret on disk**, and your own config files are never rewritten behind your back
- **Fits COSMIC:** follows the desktop theme live, frosted see-through window, drop-down mode

## Features

### Terminal

- **Tabs and split panes** – local shells and SSH sessions side by side; drag
  tabs to reorder them, split a tab right or down, resize panes by dragging the
  line between them, maximize one pane. → [Split panes](docs/guides/split-panes.md)
- **Search the scrollback** – matches light up as you type. →
  [Search and links](docs/guides/search-and-links.md)
- **Clickable links** – hold <kbd>Ctrl</kbd> to underline URLs, hyperlinks
  (OSC 8) and existing files; <kbd>Ctrl</kbd>+click opens them
- **Broadcast** – what you type into one terminal of the group goes to all of
  them: one command on ten servers at once. →
  [Tutorial](docs/tutorials/04-one-command-many-servers.md)
- **Command palette** (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>) – switch
  tabs, connect to hosts, run snippets, trigger actions or pick a theme by typing
  a few letters. → [Command palette](docs/guides/command-palette.md)
- **Activity in background tabs** – a dot for new output or the bell, and
  "watch for silence" with a desktop notification
- **A look before risky pastes** – several lines that would run at once, `sudo`,
  `curl … | sh`, `rm -rf`, or anything going to several terminals is shown first
- **Restores the last session** – tabs, splits, working directories, SSH
  connections, files and settings tabs. → [Sessions](docs/guides/sessions.md)
- **Drop-down window** (`terminaal --quake`) – drops down from the top of the
  screen and hides again, a real layer surface on COSMIC, KDE, Sway and
  Hyprland. → [Drop-down window](docs/guides/drop-down-window.md)
- Bracketed paste, mouse selection that scrolls along, the mouse wheel in
  full-screen programs (`less`, `htop`, `vim`), a right-click menu, and a
  settings tab for every option

### Shell integration

Set up by Terminaal itself for **fish, bash and zsh**; any shell that sends
OSC 7 and OSC 133 works too, also over SSH. →
[Shell integration](docs/guides/shell-integration.md)

- Tab titles show the working directory (or the running program), and a new tab
  opens in the same folder
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> jump from prompt to prompt
- A failed command gets its exit code (`✘ 1`) next to the prompt, a slow one its
  run time (`4.2 s`)
- Copy the last command's output, or click a prompt to select a command with its output
- A desktop notification when a long command finishes in a tab you're not looking at

### SSH

→ [SSH hosts](docs/guides/ssh-hosts.md) ·
[Tutorial: your first SSH host](docs/tutorials/01-your-first-ssh-host.md)

- **Saved hosts plus your `~/.ssh/config`** (`Include`, `Match`, `%` tokens …),
  read-only or copied over to edit
- **Several logins per host** (user + key), one of them the default
- **ProxyJump** chains and **ProxyCommand**
- **Reconnects on its own** when a connection drops, keeping the scrollback
- **Port forwarding** – local, remote and SOCKS (`DynamicForward`), Unix
  sockets too; pause, start and retry each one live from the sidebar. →
  [Tutorial](docs/tutorials/03-port-forwarding.md)
- **Per-host options** with their `ssh_config` keyword: timeouts, keepalives,
  compression, agent forwarding, `StrictHostKeyChecking`, `RemoteCommand`,
  `SetEnv`, algorithms and more
- **A color per host** – a warning color on its tabs and frames, or a theme of
  its own, so production never looks like a test box
- **Host keys** checked against `~/.ssh/known_hosts`; a changed key shows both
  fingerprints, and old entries can be removed from the sidebar. →
  [Host keys](docs/guides/host-keys.md)
- Prompts for host keys, passphrases and passwords appear **inside the tab**,
  like OpenSSH – nothing secret is stored

### Files on the server (SFTP)

<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> in an SSH terminal opens its
files tab, over the terminal's own connection. →
[Files on a server](docs/guides/files-and-sftp.md)

- **Browse, upload, download and delete** – several entries at once with
  <kbd>Ctrl</kbd>/<kbd>Shift</kbd>+click, or by dropping files onto the window.
  Copies never overwrite; local deletions go to the trash
- **Keep folders in sync** – per host, two-way or one-way, when the files tab
  opens or **live in the background** while a terminal is logged in. Conflicts
  keep both versions; deletions only travel where you allow it. →
  [Syncing folders](docs/guides/files-and-sftp.md#syncing-folders)
- **Edit a server file in your local editor** – every save goes back, after
  checking (by SHA-256 on the server) that nobody changed it meanwhile
- **Files only root may change** go through sudo in the terminal, where it asks
  for the password as usual
- Transfers and saves wait out a dropped connection and carry on once it's back

### Shells and commands

→ [Commands and snippets](docs/guides/commands-and-snippets.md)

- Any shell from `/etc/shells` for a new tab, and a default one
- **Aliases, functions and lines of your own** for fish, bash and zsh, managed
  in the sidebar. They live in a separate file only Terminaal loads – your
  `.bashrc` and friends are never touched
- **Built-in commands** as buttons: update the system (plus Flatpak and AUR
  separately), pending updates, disk space, memory, top processes, failed
  services, log errors, open ports – tailored to the system of the active tab
  (pacman, apt, dnf, zypper, apk, xbps, emerge, nixos-rebuild, brew, pkg),
  locally and over SSH
- **Snippets** – your own commands as buttons, optionally only for one system or
  host, or run by themselves with every new shell or after an SSH login

### Keys

→ [Tutorial: keys and the agent](docs/tutorials/02-keys-and-the-agent.md)

- Generate **Ed25519** keys (optionally with a passphrase), add existing key
  files, or take over keys from your **SSH agent** (e.g. 1Password)
- Copy the public key with one click; renaming a key updates the hosts that use it
- Only keys generated in Terminaal can have their files deleted, and only after asking

### Look and feel

→ [Appearance](docs/guides/appearance.md) ·
[Tutorial: your own theme](docs/tutorials/05-make-your-own-theme.md)

- **Themes** for console and interface alike: seven built in, and your own in
  Alacritty's format (its themes work as they are)
- A **COSMIC theme** that follows the desktop's colors, light/dark and accent live
- A **see-through window**, frosted on COSMIC (and other compositors with
  `ext-background-effect`) and KDE
- Your choice of **console font** and **menu font**
- **Every keyboard shortcut configurable**, several per action if you like
- **English and German**, following your locale or set in the settings

## Installation

Terminaal is built from source.

**Requirements**

- Rust **1.88** or newer ([rustup](https://rustup.rs/))
- A C compiler, `pkg-config`, and the OpenSSL and zlib development headers (for libssh2).
  On Debian/Ubuntu: `sudo apt install build-essential pkg-config libssl-dev zlib1g-dev`
- A GPU driver with Vulkan or OpenGL support
- [ImageMagick](https://imagemagick.org/) (`magick`), only for `install.sh`, which scales the icons

**Install for the current user**

```sh
git clone https://github.com/mergedeyes/terminaal terminaal
cd terminaal
./install.sh
```

This installs a release build to `~/.cargo/bin/terminaal`, plus a desktop entry,
icons and the man page (`man terminaal`) under `~/.local/share`, so Terminaal
appears in your app launcher.

| Command | What it does |
| --- | --- |
| `./install.sh` | Binary, desktop entry, icons and the `terminaal(1)` man page |
| `./install.sh --no-binary` | Desktop entry, icons and man page only |
| `./install.sh --uninstall` | Removes all of the above |

Run `./install.sh` again after updating the source – the launcher always starts
the installed binary.

**Just try it**

```sh
cargo run --release
```

New here? [Getting started](docs/getting-started.md) walks through the window
and the first steps.

## Getting around

### Keyboard shortcuts

The ones you'll use most:

| Shortcut | Action |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | New tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | Close the pane (the tab, when it's the only one) |
| <kbd>Ctrl</kbd>+<kbd>Tab</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd> | Next / previous tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd> | Split right / down |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> / <kbd>V</kbd> | Copy / paste |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> | Search the scrollback |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> | Command palette |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> | Files of the SSH connection |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> | Show or hide the sidebar |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Settings |

<details>
<summary><b>All default shortcuts</b></summary>

| Shortcut | Action |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> | New tab with the default shell |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | Close the pane (the tab, when it's the only one) |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>W</kbd> | Close the tab with all its panes |
| <kbd>Ctrl</kbd>+<kbd>Tab</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd> | Next / previous tab |
| <kbd>Alt</kbd>+<kbd>1</kbd> … <kbd>9</kbd> | Go to tab 1 … 9 |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> | Move the tab left / right |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd> | Split right / split down |
| <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Go to the pane in that direction |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> | Maximize the pane / show all panes again |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> | Terminal joins / leaves the broadcast |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> | Files of the SSH connection (SFTP) |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> | Show or hide the sidebar |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Open the settings tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> | Command palette |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> / <kbd>V</kbd> | Copy / paste |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>C</kbd> | Copy the last command's output |
| <kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> | Scroll the scrollback a page up / down |
| <kbd>Shift</kbd>+<kbd>Home</kbd> / <kbd>End</kbd> | Scroll to the top / bottom |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> | Search the scrollback |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd> / <kbd>↓</kbd> | Previous / next prompt |
| <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>-</kbd> / <kbd>0</kbd> | Font bigger / smaller / back (until Terminaal quits) |

"Paste and run" and "Watch for silence" have no default. In full-screen programs
such as `less` or `vim`, the scrolling keys go to the program.

</details>

Every shortcut can be changed, removed or given more key combinations – in the
settings tab under **Shortcuts** (click **+** and press the keys) or in
`config.toml`. → [Keyboard shortcuts](docs/guides/keyboard-shortcuts.md)

### Mouse

- <kbd>Ctrl</kbd>+click a link or file name to open it
- Middle-click a tab to close it; the ☰ button in the tab bar toggles the sidebar
- Clicking into a pane gives it the keyboard; the wheel scrolls the pane under the mouse
  (<kbd>Shift</kbd>+wheel scrolls Terminaal's scrollback even in full-screen programs)
- Right-click in a terminal for copy, paste, broadcast, splitting and the files tab

### Command line

```sh
terminaal                              # a local shell
terminaal --connect myserver           # connect to a saved or ~/.ssh/config host
terminaal --connect admin@myserver     # ...using that host's login for "admin"
terminaal -s                           # list the installed shells and exit
terminaal -s fish                      # first tab with that shell (name or path)
terminaal -c "journalctl -f"           # run that line instead of a shell, like xterm -e
terminaal -s bash -c "make" --hold     # ...in bash, and keep the tab once it ends
terminaal --connect web -c "htop"      # run it on the server (RemoteCommand)
terminaal --quake                      # show or hide the drop-down window (bind it to a key)
terminaal --help                       # all of it, and `man terminaal` for more
```

With `--connect`, `-s` or `-c` the saved session is neither restored nor
written. → [The command line](docs/guides/command-line.md)

## Configuration

Everything is optional and can be set in the settings tab (**⚙** in the sidebar,
or <kbd>Ctrl</kbd>+<kbd>,</kbd>). Behind it, everything lives in
`~/.config/terminaal/` – plain TOML you may edit by hand. None of it holds
anything secret.

| File | Contents |
| --- | --- |
| `config.toml` | Your settings. Terminaal changes just the line you changed in the settings and keeps your comments |
| `hosts.toml` | Saved SSH hosts, logins, options and synced folders |
| `keys.toml` | Named keys: file paths, or the public half of agent keys |
| `snippets.toml` | Your own commands |
| `themes/*.toml` | Your own color themes, in [Alacritty's format](https://alacritty.org/config-alacritty.html#colors) |
| `shell-integration/` | Startup files Terminaal generates for bash, zsh and fish |

A missing or broken `config.toml` just means defaults, never a failed start.

<details>
<summary><b>Example <code>config.toml</code> with every option</b></summary>

```toml
font_size = 15.0              # logical pixels
line_height_factor = 1.25
padding = 8.0                 # around the terminal grid
scrollback_lines = 10000
scroll_lines = 3.0            # lines per mouse-wheel notch
scroll_select_factor = 3.0    # wheel multiplier while marking text (1 = off)
default_width = 1000.0        # window size at start
default_height = 650.0
cursor_blink = true
cursor_blink_interval_ms = 600
notify_after_secs = 10        # notify when a command ran this long unseen (0: never)
paste_warning = true          # ask before risky pastes (several lines, sudo, curl | sh, rm -rf)
silence_secs = 15             # a terminal watched for silence is quiet after this long
tab_bar = true
sidebar = true                # show the sidebar at start
sidebar_width = 300.0
splash = true                 # start-up animation
restore_session = true        # open last time's tabs again
quake_height = 50.0           # drop-down window height, percent of the screen
quake_hide_on_unfocus = true  # hide it when another window gets the keyboard
# shell = "/usr/bin/fish"     # default shell for new tabs (default: $SHELL)
# language = "en"             # "en" or "de" (default: from your locale)
# theme = "Dracula"           # color theme (default: "Terminaal")
# font_family = "Hack"        # console font (default: Noto Sans Mono)
# ui_font_family = "Inter"    # font of menus and panels (default: built in)
opacity = 1.0                 # below 1 the window is see-through (0.2–1.0)
blur = true                   # blur what's behind a see-through window
commands_run = true           # command buttons run at once; off: typed into the prompt
commands_assume_yes = false   # let them skip confirmations (-y, --noconfirm)
# system = "debian"           # what the built-in commands build for
                              # (default: from /etc/os-release)
# editor = "code"             # opens server files edited locally
                              # (default: the desktop's default app)

[shortcuts]                   # only what differs from the defaults
new_tab = "Ctrl+Alt+N"
copy = ["Ctrl+Shift+C", "Ctrl+Insert"]
paste_and_run = "Ctrl+Shift+Alt+V"
tab_9 = []                    # no shortcut
```

</details>

→ The [configuration reference](docs/guides/configuration.md) documents every
key of every file, including `hosts.toml` and `snippets.toml`.

## Documentation

The [`docs/`](docs/README.md) folder goes into detail:

| | |
| --- | --- |
| **Start here** | [Getting started](docs/getting-started.md) · [Tips and tricks](docs/tips.md) · [Troubleshooting](docs/troubleshooting.md) |
| **Tutorials** | [Your first SSH host](docs/tutorials/01-your-first-ssh-host.md) · [Keys and the agent](docs/tutorials/02-keys-and-the-agent.md) · [Port forwarding](docs/tutorials/03-port-forwarding.md) · [One command on many servers](docs/tutorials/04-one-command-many-servers.md) · [Your own theme](docs/tutorials/05-make-your-own-theme.md) |
| **Guides** | [Configuration](docs/guides/configuration.md) · [Shortcuts](docs/guides/keyboard-shortcuts.md) · [Command line](docs/guides/command-line.md) · [SSH hosts](docs/guides/ssh-hosts.md) · [Host keys](docs/guides/host-keys.md) · [Files and SFTP](docs/guides/files-and-sftp.md) · [Shell integration](docs/guides/shell-integration.md) · [Commands and snippets](docs/guides/commands-and-snippets.md) · [Command palette](docs/guides/command-palette.md) · [Search and links](docs/guides/search-and-links.md) · [Split panes](docs/guides/split-panes.md) · [Sessions](docs/guides/sessions.md) · [Drop-down window](docs/guides/drop-down-window.md) · [Appearance](docs/guides/appearance.md) |
| **Background** | [Architecture](docs/explanations/architecture.md) · [Security and your files](docs/explanations/security-and-your-files.md) · [How prompt marks work](docs/explanations/prompt-marks.md) |

## Known limitations

**Platform**
- Linux only for now
- A see-through window needs Wayland; on X11 it stays opaque
- The drop-down window needs a Wayland desktop with the layer shell (not GNOME);
  on X11 it's an always-on-top window. It takes no dropped files and no input
  method (dead keys work)
- Desktop notifications need `notify-send`, opening links `xdg-open`, the local
  trash `gio`

**SSH**
- Based on libssh2: the server can't listen on a Unix socket, and there's no
  `ControlMaster`
- Only Ed25519 keys can be generated; existing RSA/ECDSA keys work
- `Match exec` in `~/.ssh/config` is never evaluated, on purpose
- Reconnecting on its own needs a login without prompts (agent, key without
  passphrase); otherwise press <kbd>Enter</kbd> and answer them. A dropped
  connection is noticed through keepalives – with the defaults after about a
  minute and a half
- A paused remote port forward keeps listening on the server and turns
  connections away; libssh2 can't cancel it cleanly mid-session
- The built-in commands ask an SSH host what it is right after login, which can
  hold the tab for up to two seconds on a slow link; set the host's system under
  **Advanced** to skip that

**Files**
- Deleting on the server is final. Transfers resume after a dropped connection
  only while Terminaal stays open
- Folder sync compares by size and modification time; live pairs watch only the
  local folder, so server changes arrive with the next sync
- Editing through sudo pastes a command into the terminal, so that terminal
  should be at a shell prompt

**Terminal**
- A command's exit code shows once the next prompt appears. Over SSH, prompt
  marks and the working directory only work if the remote shell sends them
- With broadcast on, the built-in command buttons send the line for the focused
  terminal's system to every terminal in the group
- Split panes are resized with the mouse only
- A restored session starts every terminal fresh: no scrollback, and SSH
  terminals log in again. Closing the last tab leaves nothing to restore – close
  the window to keep your tabs

## Roadmap

<details>
<summary><b>Done</b></summary>

- [x] Local terminal core, tabs and sessions
- [x] Shell management with per-shell aliases, functions and lines of your own
- [x] SSH sessions, host manager, keys, per-host options
- [x] Configurable keyboard shortcuts for every action
- [x] Appearance: themes for console and interface, console and menu font,
  COSMIC theme sync, see-through and frosted window
- [x] Built-in commands per system, local and over SSH
- [x] Terminal correctness and search: bracketed paste, search in the
  scrollback, `known_hosts` entries shown and removable from the sidebar
- [x] Shell integration: working directory (OSC 7) and prompt marks (OSC 133),
  plus clickable URLs and paths
- [x] Own commands: snippets bound to a system or host, input broadcast, a live
  list of a session's port forwards
- [x] Split panes
- [x] SFTP: browse, transfer, edit server files locally with conflict check,
  through sudo where needed
- [x] Restoring the last session
- [x] Drop-down (Quake) window
- [x] Startup commands: snippets that run with every new shell, or after an SSH login
- [x] Separate update buttons for Flatpak and AUR helpers (yay/paru)
- [x] A color per host
- [x] Command palette
- [x] Command output through the prompt marks: copy the last output, select a command, run times
- [x] A warning before risky pastes
- [x] Activity in background tabs
- [x] `terminaal -c`, `-s` and `--hold`, and a `terminaal(1)` man page
- [x] Folder sync over SFTP – two-way or one-way, on opening or live in the background
- [x] Selecting several files at once in the files tab

</details>

**Everyday comfort**
- [ ] Reopen a closed tab
- [ ] Resize and swap panes with the keyboard
- [ ] Search: number of matches, optional regex

**Terminal protocols**
- [ ] OSC 52: programs (also over SSH) may set the clipboard, after asking
- [ ] Kitty keyboard protocol
- [ ] Synchronized output (mode 2026)
- [ ] Keyboard hints: pick URLs, paths, IPs and hashes on screen by a letter
- [ ] Vi mode for selecting and copying in the scrollback
- [ ] Images in the terminal (Kitty graphics protocol or Sixel)

**SSH**
- [ ] Folders and tags for hosts, search in the sidebar
- [ ] X11 forwarding
- [ ] Add a port forward on the fly from a connected tab
- [ ] Connection quality in the tab (latency, reconnecting)
- [ ] Import hosts from Termius, PuTTY and Remmina
- [ ] More session types: serial console, containers (`docker`/`podman exec`, distrobox/toolbox, `kubectl exec`)

**Files (SFTP)**
- [ ] Drag and drop between the local and the server side
- [ ] Sort by name, type, size and date
- [ ] Filters: hidden files, folders or files only, name pattern, size, date range, file type
- [ ] Change permissions in a small dialog, with the octal mode alongside; recursively for folders
- [ ] Preview text and images
- [ ] A preview of what a folder sync would copy

**Bigger pieces**
- [ ] Workspaces: named layouts with splits, folders, hosts and startup commands
- [ ] Recording a tab's output as an asciinema cast or a log file
- [ ] tmux control mode (`tmux -CC`): a server's tmux windows as real tabs and splits
- [ ] Detach a tab into a window of its own
- [ ] Profiles: font, theme and environment per shell or host

## Contributing

### Development

```sh
cargo run                            # dev build
cargo run -- --connect myserver      # pass arguments after --
cargo test
cargo clippy --all-targets           # should stay warning-free
RUST_LOG=terminaal=debug cargo run   # per-frame timing
RUST_LOG=terminaal=trace cargo run   # plus the SSH worker's timeline
```

Code comments are in English. User-facing text goes in `locales/*.ftl`, never
inline in the code. The [architecture overview](docs/explanations/architecture.md)
explains how the pieces fit together.

### Translations

The texts live in [`locales/`](locales/), one [Fluent](https://projectfluent.org/fluent/guide/)
file per language, compiled into the binary. To improve a translation, edit the
`.ftl` file; `cargo test` checks that every language has the same messages with
the same arguments. To add a language, copy `locales/en.ftl`, translate it, and
register it in `Language` in [`src/i18n.rs`](src/i18n.rs) – a few lines.
