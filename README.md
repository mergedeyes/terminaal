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
<img width="2557" height="1383" alt="image" src="https://github.com/user-attachments/assets/8b66f2ea-c9c3-42c5-9f1c-5429d2435855" />

## Features

### Terminal
- Custom **wgpu** renderer for the terminal grid; VT parsing by
  [`alacritty_terminal`](https://crates.io/crates/alacritty_terminal)
- Redraws only when something changes (output, input, cursor blink), so it stays
  idle when you do
- Multiple tabs, each a local shell or an SSH session; a clickable tab bar
- Scrollback with the mouse wheel (speed adjustable in the settings), mouse selection,
  clipboard copy and paste
- Mouse wheel in full-screen programs: arrow keys for `less`/`man`, wheel
  reports for programs with mouse support (`htop`, `mc`, `vim` with `mouse=a`);
  hold <kbd>Shift</kbd> to scroll Terminaal's scrollback instead
- Right-click menu in the terminal: copy, paste, and paste and run
- A **settings tab** for every option (**⚙** in the sidebar or <kbd>Ctrl</kbd>+<kbd>,</kbd>)
- **Configurable keyboard shortcuts** for every action, several per action if you like
- Optional start-up animation

### Shells
- Choose any shell listed in `/etc/shells` for a new tab, and set a default
- Manage **aliases and functions** for fish, bash and zsh from the sidebar.
  They live in a separate file that only Terminaal loads, so your other
  terminals are unaffected:
  `~/.config/fish/terminaal.fish`, `~/.bash_terminaal`, `$ZDOTDIR/.zsh_terminaal`

### SSH
- Saved hosts plus the hosts from your **`~/.ssh/config`**, read-only or
  copied over to edit. Supports `Include`, `Match`, `%` tokens and more
- **Several logins per host** (user + key), with one as the default
- **ProxyJump** chains and **ProxyCommand**
- **Port forwarding**: `LocalForward`, `RemoteForward` and `DynamicForward`
  (a SOCKS 4/4a/5 proxy; a `RemoteForward` with only a port runs one on the
  server). Unix socket paths work on either side, except for the server
  listening on one
- Per-host options, grouped in the form with their `ssh_config` keyword:
  timeouts, keepalives, compression, address family, authentication order,
  agent forwarding (`ForwardAgent`), `StrictHostKeyChecking`, `RemoteCommand`, `SetEnv`/`SendEnv`, and the
  algorithm lists (kex, host key, ciphers, MACs)
- Host keys are checked against `~/.ssh/known_hosts`, and new entries are appended,
  never rewritten
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
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | Close tab |
| <kbd>Ctrl</kbd>+<kbd>Tab</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd> | Next / previous tab |
| <kbd>Alt</kbd>+<kbd>1</kbd> … <kbd>9</kbd> | Go to tab 1 … 9 |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> | Move the tab left / right |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> | Show or hide the sidebar |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Open the settings tab |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> / <kbd>V</kbd> | Copy / paste |
| <kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> | Scroll the scrollback a page up / down |
| <kbd>Shift</kbd>+<kbd>Home</kbd> / <kbd>End</kbd> | Scroll to the top / bottom |
| <kbd>Ctrl</kbd>+<kbd>+</kbd> / <kbd>-</kbd> / <kbd>0</kbd> | Font bigger / smaller / back (until Terminaal quits) |

Every shortcut can be changed, removed or given more key combinations in the
settings tab under **Shortcuts** (click **+** and press the keys), or in
`config.toml` (see below). "Paste and run" has no default. In full-screen
programs such as `less` or `vim`, the scrolling keys go to the program.

Tabs can also be closed with a middle click; the ☰ button in the tab bar
toggles the sidebar as well.

### Command line

```sh
terminaal                          # a local shell
terminaal --connect myserver       # connect to a saved or ~/.ssh/config host
terminaal --connect admin@myserver # ...using that host's login for "admin"
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
tab_bar = true
sidebar = true                # show the sidebar at start
sidebar_width = 300.0
splash = true                 # start-up animation
# shell = "/usr/bin/fish"     # default shell for new tabs (default: $SHELL)
# language = "en"             # "en" or "de" (default: from your locale)

[shortcuts]                   # only what differs from the defaults
new_tab = "Ctrl+Alt+N"
copy = ["Ctrl+Shift+C", "Ctrl+Insert"]
paste_and_run = "Ctrl+Shift+Enter"
tab_9 = []                    # no shortcut
```

Shortcut names are the ones listed in the settings tab's tooltips (`new_tab`,
`close_tab`, `next_tab`, `previous_tab`, `tab_1` … `tab_9`, `move_tab_left`,
`move_tab_right`, `toggle_sidebar`, `open_settings`, `copy`, `paste`,
`paste_and_run`, `scroll_page_up`, `scroll_page_down`, `scroll_to_top`,
`scroll_to_bottom`, `font_bigger`, `font_smaller`, `font_reset`). Key
combinations are modifiers (`Ctrl`, `Shift`, `Alt`, `Super`) plus one key,
joined by `+`; write `Plus` and `Minus` for those keys.

Every option can also be set in the settings tab (**⚙** in the sidebar, or
<kbd>Ctrl</kbd>+<kbd>,</kbd>; hover a setting's name to see its key). Font,
padding, tab bar, cursor and scrolling apply right away, the sidebar width once
you let go of its slider; window size, sidebar and animation at the next start. When you
change something there, Terminaal edits just that line and keeps your comments and
formatting.

Next to it, Terminaal keeps its own files. None of them holds anything secret:

| File | Contents |
| --- | --- |
| `hosts.toml` | Saved SSH hosts, logins and options |
| `keys.toml` | Named keys: file paths, or the public half of agent keys |

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

## Roadmap

- [x] Local terminal core, tabs and sessions
- [x] Shell management with per-shell aliases and functions
- [x] SSH sessions, host manager, keys, per-host options
- [x] Configurable keyboard shortcuts for every action
- [ ] Appearance: translucency, COSMIC theme sync, font choice, themes

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
