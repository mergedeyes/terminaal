<p align="center">
  <img src="assets/terminaal_logo.png" alt="Terminaal logo" width="160">
</p>

<h1 align="center">Terminaal</h1>

<p align="center">
  A GPU-rendered terminal emulator for COSMIC with a built-in SSH manager.<br>
  Written in Rust. English and German UI.
</p>

---

Terminaal combines an everyday terminal with Termius-style host management
in the style of Tabby/Terminus. You can open local shells and SSH sessions side by side in tabs, keep your hosts, logins and keys
in a sidebar, and never have secrets written to disk.

> **Status:** under active development. It's used as a daily driver on
> Linux (Wayland and X11). Expect rough edges, and expect the config format to change.

> **DISCLAIMER:** This project is coded mostly by Claude Code, I do make all decisions though; every feature was planned by me.

**SCREENSHOTS:**
<img width="2557" height="1383" alt="image" src="https://github.com/user-attachments/assets/95a72250-0e38-4be0-86cd-d9cbce21ce06" />
<img width="2557" height="1383" alt="image" src="https://github.com/user-attachments/assets/5d820773-2745-474c-bb17-9327a6339164" />

## Features

### Terminal
- Custom **wgpu** renderer for the terminal grid; VT parsing by
  [`alacritty_terminal`](https://crates.io/crates/alacritty_terminal)
- Redraws only when something changes (output, input, cursor blink), so it stays
  idle when you do
- Multiple tabs, each a local shell or an SSH session; a clickable tab bar
- **Drop-down window** (`terminaal --quake`, bound to a key in your desktop's
  settings): a terminal that drops down from the top of the screen and hides
  again, with tabs of its own that keep running. A real layer surface on
  COSMIC, KDE, Sway and Hyprland
- **Restores the last session** at start: tabs, splits, shells in their
  working directories, SSH connections, files and settings tabs (no scrollback).
  A second window starts fresh
- **Split panes**: divide a tab into several terminals side by side or one above
  the other (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd>, or the right-click menu). A new pane
  runs the same shell in the same folder, or connects to the same host. Drag the
  line between panes to resize them, click or <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+arrow key to move
  between them, and <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> shows one pane over the whole tab
- Scrollback with the mouse wheel (speed adjustable in the settings), mouse selection,
  clipboard copy and paste
- **Search the scrollback** (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd>): matches
  light up as you type, <kbd>Enter</kbd> and then <kbd>n</kbd>/<kbd>N</kbd> jump
  between them
- Bracketed paste: pasted text goes to shells and editors as one paste, so a
  multi-line paste isn't run line by line
- **Shell integration** for fish, bash and zsh, set up by Terminaal itself
  (any other shell that sends OSC 7 and OSC 133 works too, and so do remote
  shells over SSH that send them – fish 4 does on its own):
  - tab titles show the working directory, and a new tab opens in it
  - <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> jump from prompt to prompt
  - a failed command gets its exit code (`✘ 1`) next to its prompt
  - a desktop notification when a long command finishes in a tab you're not
    looking at (after 10 seconds by default, adjustable)
- **Files on the server (SFTP)** over the terminal's own connection
  (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> or the right-click menu): browse,
  upload and download files and folders (also by dragging files onto the window),
  and **edit a server file in your local editor** – every save goes back, with a
  check (a SHA-256 computed on the server) that nobody changed it meanwhile.
  Transfers and saves wait out a dropped connection and carry on once it's back. Files only root may change go through
  sudo in the terminal, where it asks for the password as usual
- **Broadcast**: put terminals into a broadcast group (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd>
  or the right-click menu), and what you type, paste or send with a command
  button in one of them goes to all of them – the same command on ten servers
  at once, in separate tabs or in the panes of one. Tabs holding a terminal of
  the group are marked red in the tab bar, panes get a red frame
- **Clickable links**: hold <kbd>Ctrl</kbd> to underline URLs, hyperlinks
  (OSC 8) and existing files under the mouse; <kbd>Ctrl</kbd>+click opens them
  with your default application. File names count relative to the shell's
  working directory, and only in local tabs
- Mouse wheel in full-screen programs: arrow keys for `less`/`man`, wheel
  reports for programs with mouse support (`htop`, `mc`, `vim` with `mouse=a`);
  hold <kbd>Shift</kbd> to scroll Terminaal's scrollback instead
- Right-click menu in the terminal: copy, paste, paste and run, joining or
  leaving the broadcast, splitting and closing the pane, the files of an SSH
  connection
- A **settings tab** for every option (**⚙** in the sidebar or <kbd>Ctrl</kbd>+<kbd>,</kbd>)
- **Themes** for the console and the interface alike: seven built in, and your
  own in Alacritty's format (its themes work as they are)
- On the COSMIC desktop, a **COSMIC theme** that follows the desktop's colors,
  light/dark switch and accent color included, as they change
- A **see-through window** with adjustable opacity, frosted (blurred behind) on
  COSMIC and other compositors with `ext-background-effect`, and on KDE
- Choose the **console font** and the **menu font** from the installed ones
- **Configurable keyboard shortcuts** for every action, several per action if you like
- Optional start-up animation

### Shells
- Choose any shell listed in `/etc/shells` for a new tab, and set a default
- Manage **aliases and functions** for fish, bash and zsh from the sidebar.
  They live in a separate file that only Terminaal loads, so your other
  terminals are unaffected:
  `~/.config/fish/terminaal.fish`, `~/.bash_terminaal`, `$ZDOTDIR/.zsh_terminaal`
- **Built-in commands**: buttons for the everyday chores – update the system,
  and separately Flatpaks and AUR packages (paru/yay) where installed, list what has updates, free space, folder sizes, memory, top processes,
  uptime, failed services, log errors, open ports, addresses. The
  lines are tailored to the system the active tab is on (pacman, apt, dnf,
  zypper, apk, xbps, emerge, nixos-rebuild, brew, pkg), detected from
  `/etc/os-release` locally and asked of the host over SSH – overridable in
  the settings and per host. Confirmation prompts are left alone unless you
  turn that off (`commands_assume_yes`), and the first click explains that
  commands go straight to the shell
- **Your own commands** (snippets) as buttons next to the built-in ones: a
  name and a command of one or more lines, optionally only for one system or
  one host. Add, edit and delete them under **Manage**. A snippet can also run
  by itself as a **startup command**: with every new shell, or after an SSH
  login

### SSH
- Saved hosts plus the hosts from your **`~/.ssh/config`**, read-only or
  copied over to edit. Supports `Include`, `Match`, `%` tokens and more
- **Several logins per host** (user + key), with one as the default
- **ProxyJump** chains and **ProxyCommand**
- **Reconnects on its own** when a connection drops – after 2, 4, 8 … seconds, as
  long as logging in needs no answer – keeping the scrollback. <kbd>Enter</kbd>
  tries right away, <kbd>Ctrl</kbd>+<kbd>D</kbd> closes the tab
- **Port forwarding**: `LocalForward`, `RemoteForward` and `DynamicForward`
  (a SOCKS 4/4a/5 proxy; a `RemoteForward` with only a port runs one on the
  server). Unix socket paths work on either side, except for the server
  listening on one
- Per-host options, grouped in the form with their `ssh_config` keyword:
  timeouts, keepalives, compression, address family, authentication order,
  agent forwarding (`ForwardAgent`), `StrictHostKeyChecking`, `RemoteCommand`, `SetEnv`/`SendEnv`, and the
  algorithm lists (kex, host key, ciphers, MACs)
- The sidebar shows the active tab's **port forwards** – running, paused or
  failed and why – and pauses, starts or retries each while connected
- Host keys are checked against `~/.ssh/known_hosts`, and new entries are appended.
  If a host's key changed, the tab shows the stored fingerprint next to the new
  one. The **Host key** button on a host in the sidebar shows what's stored and
  removes it after asking – only those lines, the rest of the file stays as it is
- Host-key prompts, passphrases and passwords are asked **inside the tab**,
  like OpenSSH does. Nothing secret is ever stored

### Keys
- Generate **Ed25519** keys (optionally with a passphrase) in OpenSSH
  format, add existing key files, or take over keys from your **SSH agent**
  (e.g. 1Password)
- Copy the public key with one click, and rename keys; hosts that use a key follow the rename
- Only keys generated in Terminaal can have their files deleted, and only
  after asking

### Languages
- English and German, via [Project Fluent](https://projectfluent.org/).
  The default follows your system locale (German for a German locale, English
  otherwise). Switch it any time in the settings tab

## Documentation

The [`docs/`](docs/README.md) folder has more: a
[getting started](docs/getting-started.md) guide, tutorials (your first SSH host,
keys and agents, port forwarding, one command on many servers, your own theme),
guides for configuration, shortcuts, SSH hosts and host keys, shell integration,
commands and snippets, search and links, split panes, restoring the session, the
drop-down window, files over SFTP and appearance, background on the architecture,
the security model and prompt marks, plus
[tips](docs/tips.md) and [troubleshooting](docs/troubleshooting.md).

## Installation

Terminaal is built from source.

**Requirements**
- Rust **1.88** or newer ([rustup](https://rustup.rs/))
- A C compiler, `pkg-config`, and the OpenSSL and zlib development headers
  (for libssh2). On Debian/Ubuntu: `sudo apt install build-essential pkg-config libssl-dev zlib1g-dev`
- A GPU driver with Vulkan or OpenGL support
- [ImageMagick](https://imagemagick.org/) (`magick`), only for `install.sh`,
  which scales the icons

**Install for the current user**

```sh
git clone https://github.com/mergedeyes/terminaal terminaal
cd terminaal
./install.sh
```

This installs a release build to `~/.cargo/bin/terminaal`, plus a desktop entry and
icons under `~/.local/share`, so Terminaal appears in your app launcher.

| Command | What it does |
| --- | --- |
| `./install.sh` | Binary, desktop entry and icons |
| `./install.sh --no-binary` | Desktop entry and icons only |
| `./install.sh --uninstall` | Removes all of the above |

Run `./install.sh` again after updating the source; the launcher always starts the installed binary.

**Just try it**

```sh
cargo run --release
```

## Usage

### Keyboard shortcuts

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
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> / <kbd>V</kbd> | Copy / paste |
| <kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> | Scroll the scrollback a page up / down |
| <kbd>Shift</kbd>+<kbd>Home</kbd> / <kbd>End</kbd> | Scroll to the top / bottom |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> | Search the scrollback |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd> / <kbd>↓</kbd> | Previous / next prompt |
| <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>-</kbd> / <kbd>0</kbd> | Font bigger / smaller / back (until Terminaal quits) |

Every shortcut can be changed, removed or given more key combinations in the
settings tab under **Shortcuts** (click **+** and press the keys), or in
`config.toml` (see below). "Paste and run" has no default. In full-screen
programs such as `less` or `vim`, the scrolling keys go to the program.

In the search bar, type to search upwards from the bottom of the screen (case
matters only once you type a capital letter). <kbd>Enter</kbd> jumps to the next
match up, <kbd>Shift</kbd>+<kbd>Enter</kbd> down; after that <kbd>n</kbd> and
<kbd>N</kbd> do the same, and <kbd>/</kbd> or <kbd>Backspace</kbd> edit the query
again. <kbd>Esc</kbd> closes the bar, as does any other key, which then goes to
the shell.

Hold <kbd>Ctrl</kbd> and click a link or file name to open it (not a
configurable shortcut). Tabs can also be closed with a middle click; the ☰
button in the tab bar toggles the sidebar as well. Clicking into a pane gives
it the keyboard; the mouse wheel scrolls the pane under the mouse.

### Command line

```sh
terminaal                          # a local shell
terminaal --connect myserver       # connect to a saved or ~/.ssh/config host
terminaal --connect admin@myserver # ...using that host's login for "admin"
terminaal --quake                  # show or hide the drop-down window (bind it to a key)
```

## Configuration

Everything is optional. Terminaal reads `~/.config/terminaal/config.toml`.
A missing or broken file just means defaults, never a failed start.

```toml
font_size = 15.0              # logical pixels
line_height_factor = 1.25
padding = 8.0                 # around the terminal grid
scrollback_lines = 10000
scroll_lines = 3.0            # lines per mouse-wheel notch
default_width = 1000.0        # window size at start
default_height = 650.0
cursor_blink = true
cursor_blink_interval_ms = 600
notify_after_secs = 10        # notify when a command ran this long unseen (0: never)
tab_bar = true
sidebar = true                # show the sidebar at start
sidebar_width = 300.0
splash = true                 # start-up animation
restore_session = true        # open last time's tabs again
quake_height = 50.0           # drop-down window height, percent of the screen
quake_hide_on_unfocus = true  # hide it when another window gets the keyboard
# shell = "/usr/bin/fish"     # default shell for new tabs (default: $SHELL)
# language = "en"             # "en" or "de" (default: from your locale)
# theme = "Dracula"           # color theme, see below (default: "Terminaal")
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

Shortcut names are the ones listed in the settings tab's tooltips (`new_tab`,
`close_tab`, `next_tab`, `previous_tab`, `tab_1` … `tab_9`, `move_tab_left`,
`move_tab_right`, `split_right`, `split_down`, `close_pane`,
`focus_pane_left`, `focus_pane_right`, `focus_pane_up`, `focus_pane_down`,
`zoom_pane`, `toggle_broadcast`, `open_files`, `toggle_sidebar`, `open_settings`,
`copy`, `paste`, `paste_and_run`, `scroll_page_up`, `scroll_page_down`,
`scroll_to_top`, `scroll_to_bottom`, `search`, `previous_prompt`,
`next_prompt`, `font_bigger`, `font_smaller`, `font_reset`). Key
combinations are modifiers (`Ctrl`, `Shift`, `Alt`, `Super`) plus one key,
joined by `+`; write `Plus` and `Minus` for those keys.

### Themes

A theme colors the console as well as the sidebar, tab bar and settings.
Built in are Terminaal, Catppuccin Mocha, Dracula, Gruvbox Dark, Nord,
Solarized Dark and Solarized Light. On COSMIC there's also **COSMIC**, made
from the desktop's own theme and following it live. Your own go into
`~/.config/terminaal/themes/` as `.toml` files, named after the file. The
format is [Alacritty's](https://alacritty.org/config-alacritty.html#colors),
so its themes (e.g. from [alacritty-theme](https://github.com/alacritty/alacritty-theme))
work as they are:

```toml
[colors.primary]
background = "#1e1e2e"
foreground = "#cdd6f4"

[colors.normal]   # black red green yellow blue magenta cyan white, all eight
black = "#45475a"
# ...

# Optional: [colors.bright], [colors.dim], [colors.cursor] cursor,
# [colors.selection] background/text,
# [colors.search.matches] and [colors.search.focused_match] background/foreground.

[ui]              # optional, all keys too; the rest follows from the colors above
accent = "#cba6f7"
# background, row, hover, selected, border, border_strong, text, text_weak,
# error, success, input, text_selection
```

After editing a theme file, click **Reload** next to the theme in the settings.

Every option can also be set in the settings tab (**⚙** in the sidebar, or
<kbd>Ctrl</kbd>+<kbd>,</kbd>; hover a setting's name to see its key). Font,
padding, tab bar, cursor, scrolling and notifications apply right away, the sidebar width once
you let go of its slider; window size, sidebar and animation at the next start. When you
change something there, Terminaal edits just that line and keeps your comments and
formatting.

Next to it, Terminaal keeps its own files. None of them holds anything secret:

| File | Contents |
| --- | --- |
| `hosts.toml` | Saved SSH hosts, logins and options |
| `keys.toml` | Named keys: file paths, or the public half of agent keys |
| `snippets.toml` | Your own commands |
| `shell-integration/` | Startup files Terminaal generates for bash, zsh and fish: they load your own config, then the managed aliases and the shell integration |
| `themes/*.toml` | Your own color themes (you write these; Terminaal only reads them) |

### Snippets

Snippets are managed in the sidebar, but `snippets.toml` is plain TOML too:

```toml
[[snippet]]
name = "Follow logs"
command = "journalctl -f"
system = "arch"     # optional: only on this system (arch, debian, fedora, suse,
                    # alpine, void, gentoo, nixos, macos, freebsd)

[[snippet]]
name = "Deploy"
command = """
cd /srv/app
./deploy.sh"""
host = "web1"       # optional: only in SSH tabs of this host (its name in the sidebar)
# local = true      # optional instead of host: only in local terminals
autorun = "login"   # optional: run by itself, "shell" (every new terminal)
                    # or "login" (SSH terminals, after login)
hidden = true       # optional: no button, only listed under Manage
```

## Translations

The texts live in [`locales/`](locales/), one [Fluent](https://projectfluent.org/fluent/guide/)
file per language, and are compiled into the binary. To improve a
translation, edit the `.ftl` file. `cargo test` checks that every
language has the same messages with the same arguments.

To add a language, copy `locales/en.ftl`, translate it, and register it
in `Language` in [`src/i18n.rs`](src/i18n.rs), which takes a few lines.

## Known limitations

- Linux only for now
- SSH is based on libssh2, which can't have the server listen on a Unix
  socket. `ControlMaster` isn't supported either
- Only Ed25519 keys can be generated. Existing RSA/ECDSA keys work
- `Match exec` in `~/.ssh/config` is never evaluated, on purpose
- A see-through window needs Wayland; on X11 it stays opaque
- The built-in commands ask an SSH host what it is right after login, which
  can hold the tab for up to two seconds on a slow link; set the host's
  system under "Advanced" to skip that
- Desktop notifications need `notify-send`, opening links needs `xdg-open`
- A command's exit code shows once the next prompt appears. Over SSH, prompt
  marks and the working directory only work if the remote shell sends them
- A paused remote port forward keeps listening on the server and turns
  connections away; libssh2 can't cancel it cleanly mid-session
- With broadcast on, the built-in command buttons send the line for the focused
  terminal's system to every terminal in the group
- Split panes are resized with the mouse only
- The drop-down window needs a Wayland desktop with the layer shell (not
  GNOME); on X11 it's an always-on-top window. It takes no dropped files and
  no input method (dead keys work)
- A restored session starts every terminal fresh: no scrollback, and SSH
  terminals log in again. Closing the last tab (or `exit` in the last shell)
  leaves nothing to restore – close the window to keep your tabs
- SFTP never overwrites on copying (taken names get a number) and deletes only
  files and empty folders. Transfers resume after a dropped connection only while
  Terminaal stays open. Editing through sudo pastes a command into the terminal,
  so that terminal should be at a shell prompt
- Reconnecting on its own needs a login without prompts (agent, key without
  passphrase); otherwise press Enter and answer them. A dropped connection is
  noticed through keepalives – with the defaults after about a minute and a half

## Roadmap

- [x] Local terminal core, tabs and sessions
- [x] Shell management with per-shell aliases and functions
- [x] SSH sessions, host manager, keys, per-host options
- [x] Configurable keyboard shortcuts for every action
- [x] Appearance: themes for console and interface, console and menu font,
  COSMIC theme sync, see-through and frosted window
- [x] Built-in commands per system, local and over SSH
- [x] Terminal correctness and search: bracketed paste, search in the
  scrollback, `known_hosts` entries shown and removable from the sidebar
- [x] Shell integration: working directory (OSC 7) and prompt marks (OSC 133)
  for new tabs in the same folder, jumping between prompts, exit codes and
  notifications, plus clickable URLs and paths
- [x] Own commands: named snippets bound to a system or host, input broadcast
  to several tabs, and a live list of a session's port forwards
- [x] Split panes: several terminals in one tab, resizable, with keyboard focus
  moves and a maximized view
- [x] SFTP: browse, transfer, edit server files locally with conflict check,
  through sudo where needed
- [x] Restoring the last session: tabs, splits, directories, SSH connections
- [x] Drop-down (Quake) window: a layer surface toggled by `terminaal --quake`
- [x] Startup commands: snippets that run with every new shell, or after an SSH login
- [x] Separate update buttons for Flatpak and for AUR helpers (yay/paru)

### Planned

**Everyday comfort**
- [ ] A color per host: a warning frame or tab color, or a theme of its own – so a production server never looks like a test box
- [ ] Command palette (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>): fuzzy search over actions, hosts and logins, snippets, open tabs and themes
- [ ] Working with command output through the prompt marks: copy the last output, click a prompt to select its output, show how long a command ran
- [ ] A warning before pasting several lines or risky commands (`sudo`, `curl … | sh`), especially with broadcast on
- [ ] Activity in background tabs: a mark for new output, the bell, or silence for a while
- [ ] Reopen a closed tab
- [ ] Resize and swap panes with the keyboard
- [ ] Search: number of matches, optional regex

**Terminal protocols**
- [ ] OSC 52: programs (also over SSH) may set the clipboard, after asking
- [ ] Kitty keyboard protocol
- [ ] Synchronized output (mode 2026)
- [ ] Keyboard hints: pick URLs, paths, IPs and hashes on screen by a letter to copy or open them
- [ ] Vi mode for selecting and copying in the scrollback
- [ ] Images in the terminal (Kitty graphics protocol or Sixel)

**SSH**
- [ ] Folders and tags for hosts, search in the sidebar
- [ ] X11 forwarding
- [ ] Add a port forward on the fly from a connected tab
- [ ] Connection quality in the tab (latency, reconnecting)
- [ ] Import hosts from Termius, PuTTY and Remmina
- [ ] More session types: serial console (`/dev/ttyUSB*`), containers (`docker`/`podman exec`, distrobox/toolbox, `kubectl exec`)

**Files (SFTP)**
- [ ] Drag and drop between the local and the server side
- [ ] Sort by name, type, size and date
- [ ] Filters that combine: hidden files, folders or files only, name pattern, size (below, above or between), modification date range, file type
- [ ] Change permissions in a small dialog: tick read/write/execute for owner, group and others (plus setuid/setgid/sticky), with the octal mode shown alongside and editable; recursively for folders; owner and group too
- [ ] Preview text and images
- [ ] Sync a folder one way, with a preview of what would be copied

**Bigger pieces**
- [ ] Workspaces: named layouts with splits, folders, hosts and startup commands, opened with one click
- [ ] Recording a tab's output as an asciinema cast or a log file, optionally per host
- [ ] tmux control mode (`tmux -CC`): a server's tmux windows and panes as real tabs and splits
- [ ] Detach a tab into a window of its own
- [ ] Profiles: font, theme and environment per shell or host

## Development

```sh
cargo run                        # dev build
cargo run -- --connect myserver  # pass arguments after --
cargo test
cargo clippy --all-targets       # should stay warning-free
RUST_LOG=terminaal=debug cargo run   # per-frame timing
RUST_LOG=terminaal=trace cargo run   # plus the SSH worker's timeline
```

Code comments are in English. User-facing text goes in `locales/*.ftl`,
never inline in the code.
