# Getting started

## Install

Terminaal is built from source and runs on Linux (Wayland or X11).

You need:

- Rust 1.88 or newer ([rustup](https://rustup.rs/))
- A C compiler, `pkg-config`, and the OpenSSL and zlib headers for libssh2.
  On Debian/Ubuntu: `sudo apt install build-essential pkg-config libssl-dev zlib1g-dev`.
  On Arch: `sudo pacman -S base-devel openssl zlib`
- A GPU driver with Vulkan or OpenGL
- ImageMagick (`magick`), only for the install script, which scales the icons

```sh
git clone https://github.com/mergedeyes/terminaal terminaal
cd terminaal
./install.sh
```

This puts a release build in `~/.cargo/bin/terminaal` and a desktop entry plus
icons in `~/.local/share`, so Terminaal shows up in your app launcher. After
pulling new source, run `./install.sh` again – the launcher always starts the
installed binary. `./install.sh --uninstall` removes everything again.

Just want to try it? `cargo run --release` starts it without installing.

## The window at a glance

```
┌──────────────┬───┬──────────────┬──────────────┬───┐
│ Shells SSH … │ ☰ │ ~/projects   │ me@web1      │ + │  ← tab bar
│──────────────┼───┴──────────────┴──────────────┴───│
│              │                                     │
│   sidebar    │            the terminal             │
│              │                                     │
└──────────────┴─────────────────────────────────────┘
```

- **Tab bar** – one tab per shell or SSH session. Click to switch, middle-click
  or **×** to close, **+** for a new tab with your default shell. **☰** shows and
  hides the sidebar.
- **Sidebar** – three sections along its top:
  - **Shells**: installed shells, aliases and functions, built-in commands and
    your own commands
  - **SSH**: saved hosts and the hosts from `~/.ssh/config`, plus the port
    forwards of the active tab
  - **Keys**: your SSH keys
- **⚙** at the end of the sidebar's header opens the **settings tab**.

## First steps

1. **Open a few tabs.** <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> opens a tab
   in the directory you're in; <kbd>Alt</kbd>+<kbd>1</kbd>…<kbd>9</kbd> jumps to a
   tab; <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> closes one.
   <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> splits a tab into two terminals
   side by side (see [Split panes](guides/split-panes.md)).
2. **Pick your shell.** In the sidebar's Shells section, double-click a shell to
   open a tab with it, or select it and click **Make default**.
3. **Choose a look.** Press <kbd>Ctrl</kbd>+<kbd>,</kbd>, go to **Appearance** and
   try the themes – they apply immediately.
4. **Add a server.** Follow [Your first SSH host](tutorials/01-your-first-ssh-host.md).
5. **Find anything.** <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> opens the
   [command palette](guides/command-palette.md): a few letters find a tab, a
   host, a snippet, an action or a theme.

Everything you change in the settings is saved to
`~/.config/terminaal/config.toml` right away, keeping any comments you wrote
there. Hover over a setting's name to see its key in the file.

## Working in the terminal

| To… | Do this |
| --- | --- |
| Copy | Select with the mouse, then <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> or right-click → Copy |
| Paste | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> or right-click → Paste |
| Scroll back | Mouse wheel, or <kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> |
| Search the output | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> |
| Jump between prompts | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd> / <kbd>↓</kbd> |
| Open a URL or file | Hold <kbd>Ctrl</kbd>, click it |
| Make text bigger | <kbd>Ctrl</kbd>+<kbd>+</kbd> (back with <kbd>Ctrl</kbd>+<kbd>0</kbd>) |

## Starting from the command line

```sh
terminaal                          # a local shell
terminaal --connect myserver       # open a saved or ~/.ssh/config host
terminaal --connect admin@myserver # ...with that host's login for "admin"
terminaal --quake                  # show or hide the drop-down window
```

For `--quake`, see [The drop-down window](guides/drop-down-window.md): it's meant
for a keyboard shortcut in your desktop's settings.

## Language

The interface is in English or German. By default it follows your system
locale; switch it under **Settings → General → Language**.

## Where to go next

- [Tips and tricks](tips.md) for small things that make a big difference
- [Configuration reference](guides/configuration.md) if you prefer editing files
