# The drop-down window

A terminal that drops down from the top of the screen when you press a key,
and goes away when you press it again. It works like Guake or Yakuake, and
like the console in Quake that the idea comes from. The terminals in it keep
running while it's hidden.

## Set it up

Terminaal can't grab a global key itself: on Wayland, programs aren't allowed
to. Your desktop runs a command for the key instead:

```sh
terminaal --quake
```

The first time, the command starts the drop-down Terminaal and shows it. After
that, each run hides it or shows it again.

Settings → General → **Drop-down window** shows the command with Terminaal's
full path and a **Copy command** button. Use the full path: shortcut settings
often don't have `~/.cargo/bin` in their `PATH`.

**On COSMIC:** Settings → Input Devices → Keyboard → View and customize
shortcuts → Custom shortcuts → Add shortcut. Paste the command, give it a name,
and press the keys you want, for example <kbd>F12</kbd> or
<kbd>Super</kbd>+<kbd>`</kbd>.

**On KDE Plasma:** System Settings → Keyboard → Shortcuts → Add New → Command
or Script.

**On Sway or Hyprland:** a `bindsym` or `bind` line that runs the command.

## Using it

The drop-down window is a Terminaal of its own. It has its own tabs, splits,
sidebar and settings tab, and everything works as in the normal window. It
doesn't touch the tabs of your normal Terminaal window.

| Situation | What the key does |
| --- | --- |
| Not running | Starts it and shows it |
| Showing, you're typing in it | Hides it |
| Showing, but you clicked another window | Brings it back to the keyboard |
| Hidden | Shows it |

By default it also hides when another window gets the keyboard. You can turn
that off under Settings → General → Drop-down window, or with
`quake_hide_on_unfocus = false`.

To quit it for good, close its last tab.

## Settings

| Setting | Key | Default |
| --- | --- | --- |
| Height | `quake_height` | `50` – percent of the screen's height, 20–100. Changes apply while it's showing |
| Hide when another window gets the focus | `quake_hide_on_unfocus` | `true` |

It opens full width on the screen the compositor picks, usually the one you're
working on. It sits below panels like COSMIC's top bar. Theme, font, opacity
and blur are the same as in the normal window.

## Its session

The drop-down Terminaal saves its tabs separately from the normal window, in
`~/.local/state/terminaal/quake-session.toml`. When it starts again, for example
after logging in, its tabs come back. See [Restoring the last session](sessions.md).

## Where it works

| Desktop | What you get |
| --- | --- |
| COSMIC, KDE Plasma, Sway, Hyprland and other Wayland desktops with the layer shell | A real drop-down along the top edge, above other windows |
| X11 | A borderless, always-on-top window at the top of the screen that shows and hides |
| GNOME on Wayland | A normal window. GNOME has no layer shell, and Wayland doesn't let a window place or hide itself, so the key can't hide it |

## Limits

- You can't drop files onto the drop-down window to upload them to a files tab.
- Input methods (IME) don't work in it. Dead keys and compose sequences do.
- It has no window title and doesn't flash in the dock when a long command
  finishes. Desktop notifications still come.
- There's no slide animation. It appears and disappears at once.
