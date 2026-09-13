# Tutorial: make your own theme

A Terminaal theme colors the console *and* the interface – sidebar, tab bar,
settings – together. You'll start from an Alacritty theme, then tune the
interface colors.

## 1. Where themes live

Your themes are TOML files in `~/.config/terminaal/themes/`. The theme's name is
the file name (or a `name = "…"` line inside). A theme with the same name as a
built-in one replaces it.

```sh
mkdir -p ~/.config/terminaal/themes
```

## 2. Start from an Alacritty theme

Terminaal reads Alacritty's color format, so the hundreds of themes in
[alacritty-theme](https://github.com/alacritty/alacritty-theme) work unchanged.
Pick one from that repository's `themes/` folder and save its raw file into your
themes folder:

```sh
curl -L -o ~/.config/terminaal/themes/NAME.toml \
  https://raw.githubusercontent.com/alacritty/alacritty-theme/master/themes/NAME.toml
```

Open **Settings → Appearance**, click **Reload** below the theme selection, and
pick the new theme. The whole window changes at once.

## 3. Write one from scratch

A theme needs only the background, the foreground and the eight normal colors:

```toml
name = "Harbor"

[colors.primary]
background = "#0f1720"
foreground = "#d5dde5"

[colors.normal]
black   = "#1c2733"
red     = "#e0707a"
green   = "#8cc98a"
yellow  = "#e6c07b"
blue    = "#6fa8dc"
magenta = "#c28ad6"
cyan    = "#6cc4c4"
white   = "#c0c8d0"
```

Save it as `~/.config/terminaal/themes/harbor.toml`, click **Reload**, select it.

Everything else is derived:

- **Bright** colors fall back to the normal ones, **dim** colors to darker
  versions of them
- The **cursor** uses the foreground
- **Selection** mixes background and blue
- **Search matches** mix background and yellow; the focused match is yellow
- The **interface** is built from background, foreground and blue – and it
  switches to egui's light style when the background is light

If something's wrong in the file, the error shows under the theme selection.

## 4. Fine-tune the console

Add any of these tables when the derived colors aren't what you want:

```toml
[colors.bright]
black = "#3a4a5a"
# red, green, ... as needed

[colors.cursor]
cursor = "#e6c07b"

[colors.selection]
background = "#2b3d52"
text = "#ffffff"          # optional: leave out to keep each cell's color

[colors.search.matches]
background = "#3d3520"

[colors.search.focused_match]
background = "#e6c07b"
foreground = "#0f1720"
```

## 5. Tune the interface

The `[ui]` table sets the chrome's colors. Every key is optional:

```toml
[ui]
accent        = "#6fa8dc"   # active tab line, highlights
background    = "#131c26"   # sidebar, tab bar, settings
row           = "#18232f"   # list rows and cards
hover         = "#1e2b39"
selected      = "#1f3347"   # selected list row
border        = "#253445"
border_strong = "#30445a"
text          = "#d5dde5"
text_weak     = "#8a97a4"
error         = "#e0707a"   # also marks broadcast tabs
success       = "#8cc98a"
input         = "#0c131b"   # text fields
text_selection = "#2b4a68"
```

A good workflow: keep the settings tab open next to a terminal tab, edit the
file, click **Reload**, look, repeat.

## 6. Or let the desktop decide

On COSMIC, the **COSMIC** theme is built from your desktop theme and follows it
live – including switching between light and dark, and your accent color.

## What you learned

- Themes are Alacritty-format TOML files, plus an optional `[ui]` table
- Only ten colors are required; everything else is derived
- **Reload** picks up your edits without restarting

**Reference:** [Appearance](../guides/appearance.md).
