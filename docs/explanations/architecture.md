# Architecture

This page explains how Terminaal is put together – useful if you want to
contribute, or just wonder why it behaves the way it does.

## The big pieces

```
                ┌──────────────────── main thread ─────────────────────┐
 keyboard,      │ winit event loop (app.rs)                            │
 mouse, resize ─▶  ├─ routes input: egui (sidebar/settings) or terminal │
                │  ├─ egui pass: sidebar, settings, menus              │
                │  └─ wgpu frame: grid, tab bar, search bar, egui       │
                └───────────▲──────────────────────────────────────────┘
                            │ UserEvent (wakeup, title, shell events …)
          ┌─────────────────┼──────────────────┐
          │                 │                  │
 ┌────────┴────────┐ ┌──────┴───────┐  ┌───────┴────────┐
 │ PTY event loop  │ │ PTY filter   │  │ SSH worker     │  one per SSH pane
 │ (alacritty)     │◀┤ thread       │  │ (libssh2)      │
 │ parses into Term│ │ per local    │  │ parses into    │
 │                 │ │ pane         │  │ Term, forwards │
 └─────────────────┘ └──────────────┘  │                │
                                       └────────────────┘
```

- **`alacritty_terminal`** is the "brain" of each terminal – every pane of every
  tab has its own: it parses the byte stream
  (VT sequences) into a grid of cells with a scrollback. Terminaal doesn't use
  Alacritty's renderer – only this library.
- **The renderer** is Terminaal's own, on **wgpu**: colored rectangles
  ("quads") for backgrounds, selection, cursor and underlines, and glyphon for
  text.
- **egui** draws only the chrome – sidebar, settings, context menu – in a
  separate pass on top.
- **libssh2** (through the `ssh2` crate) runs SSH, on a worker thread per SSH pane.
- **SFTP** runs over the terminal's own connection: the SSH worker opens the
  `sftp` subsystem on a channel and ties it to a socket, like a port forward. A
  thread per files tab speaks SFTP over that socket with Terminaal's own client
  (`src/sftp/protocol.rs`), keeping many reads or writes in flight at once –
  one at a time, every chunk would cost a round trip.
- **Split panes** are a binary tree per tab (`src/panes.rs`): each split halves
  its rectangle side by side or one above the other at a draggable ratio, the
  leaves are the terminals. Every layout change resizes only the terminals whose
  grid actually changed. A frame builds the visible panes one after another into
  the same quads and shares one shaping cache between them.

## One frame

1. **UI pass:** egui lays out the sidebar and settings.
2. **Build:** under the terminal's lock, Terminaal walks the visible cells once,
   pushing background quads and copying each row's text and style runs. The lock
   is held only for this copy.
3. **Shape:** after releasing the lock, each row is turned into positioned
   glyphs – but rows are cached by their content, so a row that didn't change,
   or only scrolled, is never shaped again. Shaping is by far the most expensive
   step; caching it is what keeps scrolling fast.
4. **Draw:** quads, text, then egui on top, in one render pass plus egui's.

The font size in physical pixels is rounded to whole pixels, so every glyph
lands exactly on its cell even at fractional scale factors (125 %, 150 %).

## When it redraws

Terminaal redraws only when there's a reason: output arrived, you typed or
clicked, the window changed, the cursor blinks, or egui asks for an animation
frame. An idle window draws about two frames per second – for the cursor. Turn
blinking off and it draws nothing.

On Wayland, a minimized or hidden window gets no frame callbacks from the
compositor, so it simply stops drawing instead of blocking.

## Input routing

The keyboard belongs to the terminal unless you clicked into the sidebar or
settings and a text field there has focus. This is deliberate: otherwise a
<kbd>Tab</kbd> meant for shell completion would move egui's focus into the
sidebar, and the next <kbd>Enter</kbd> would press a button. Clicking the
terminal, hiding the sidebar or switching tabs gives the keyboard back.

## Local shells

A local tab runs its shell in a PTY. Output is read by alacritty's event loop –
but first it passes Terminaal's **shell integration filter**, on a thread of its
own, which picks out working-directory and prompt sequences. See
[How prompt marks work](prompt-marks.md).

## SSH tabs

Each SSH tab has a worker thread that owns its libssh2 session:

- **Connecting** plays out in the tab: host-key questions, passphrases and
  passwords are read from what you type, the way OpenSSH does it.
- **Jump hosts:** each hop is a session; the next one runs over a
  `direct-tcpip` channel bridged through a socket pair.
- **The pump loop** moves bytes between the shell channel and the terminal, and
  services every port forward on the same non-blocking session. It sleeps in
  `poll()` on the session socket, the forward sockets and a wakeup socket that
  input and resize messages ping.
- **Keepalives** detect a dead connection (30 s interval, 3 misses by default).

libssh2 can open only one channel at a time per session, so new forwarded
connections queue up briefly; and because it reads data for all channels
whenever it reads for one, the loop doesn't sleep while any channel still has
data buffered.

## Files and settings

- `config.toml` is read with serde and **written with `toml_edit`**, one key at
  a time, so your comments and layout survive.
- Hosts, keys and snippets are written as whole files – first to a temporary file
  next to the real one, then renamed. Only after that succeeds does the change
  show in the sidebar, so a failed save never leaves the UI showing something
  that isn't on disk.
- All UI text comes from Fluent files (`locales/en.ftl`, `locales/de.ftl`)
  compiled into the binary.

## Module map

| Path | Responsibility |
| --- | --- |
| `src/app.rs` | Event loop, tabs and their panes, input routing, frame assembly |
| `src/panes.rs` | Split-pane layout: tree, rectangles, dividers, neighbours |
| `src/render/` | Grid, tab bar, search bar, labels, quads, palette |
| `src/terminal/` | Sessions, PTY filter, shell integration, prompts, search, links |
| `src/sftp/` | SFTP client over the terminal's connection, its session thread (listing, pipelined transfers), editing files locally with sudo |
| `src/ssh/` | Host catalog, `~/.ssh/config` parser, connection worker, forwards, SOCKS, agent forwarding, keys, `known_hosts` |
| `src/ui/` | egui sidebar, settings, SSH/keys/commands panels, context menu, splash |
| `src/shells/` | Shell detection, managed aliases, generated startup files |
| `src/commands.rs`, `src/snippets.rs` | Built-in commands, your own commands |
| `src/theme.rs`, `src/theme/cosmic.rs` | Themes, COSMIC theme and its file watcher |
| `src/shortcuts.rs`, `src/config.rs`, `src/i18n.rs` | Shortcuts, config, translations |
