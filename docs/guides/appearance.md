# Appearance

Everything here is under **Settings → Appearance** and applies immediately.

## Themes

A theme colors the console and the interface (sidebar, tab bar, settings)
together.

**Built in:** Terminaal (the default), Catppuccin Mocha, Dracula, Gruvbox Dark,
Nord, Solarized Dark, Solarized Light.

**Your own:** TOML files in `~/.config/terminaal/themes/`, in Alacritty's color
format, optionally with a `[ui]` table for the interface. The name is the file
name, or `name = "…"` inside; one named like a built-in theme replaces it. After
editing, click **Reload**. Errors in a file show under the theme list.

The full format, step by step: [Make your own theme](../tutorials/05-make-your-own-theme.md).

A saved SSH host can have a theme of its own for its terminals, and a warning
color for its tabs: **Advanced → Look** in the host form, see
[SSH hosts](ssh-hosts.md#look).

Summary of what a theme file can contain:

| Table | Keys | Required |
| --- | --- | --- |
| `[colors.primary]` | `background`, `foreground`, `bright_foreground`, `dim_foreground` | background, foreground |
| `[colors.normal]` | `black` `red` `green` `yellow` `blue` `magenta` `cyan` `white` | all eight |
| `[colors.bright]`, `[colors.dim]` | same eight | no |
| `[colors.cursor]` | `cursor` | no |
| `[colors.selection]` | `background`, `text` | no |
| `[colors.search.matches]`, `[colors.search.focused_match]` | `background`, `foreground` | no |
| `[ui]` | `accent` `background` `row` `hover` `selected` `border` `border_strong` `text` `text_weak` `error` `success` `input` `text_selection`, and `opacity` (0–1, how opaque the panels are in a see-through window) | no |

Colors are `#rrggbb`, `#rgb` or `0xrrggbb`. Alacritty's `CellForeground` /
`CellBackground` values count as "not set", and unknown keys are ignored – so
Alacritty theme files load as they are.

## COSMIC

On the COSMIC desktop there's a **COSMIC** theme built from your desktop theme:

- Background and text from COSMIC's background colors, the ANSI colors from its
  palette, the cursor in your accent color
- The interface like COSMIC's own panels – with frosted glass on, as see-through
  as they are (85 %)
- Follows changes live – switching between light and dark, a new accent color –
  without restarting

It doesn't copy COSMIC's "frosted" window look; use translucency below for that.

## Translucency

- **Opacity** (0.2–1): below 1, the terminal background, tab bar, sidebar and
  settings become see-through. Text, lines, colored cells and popups stay solid.
- **Interface opacity**: how see-through the tab bar, sidebar and settings are –
  never more than the terminal. Text sits on them, and a bright wallpaper behind
  a dark theme's grey text leaves little to read, so by default they stay at 85 %
  (the COSMIC theme: whatever COSMIC's own panels use, a theme file: `[ui]
  opacity`). **Theme default** goes back to that. The lower it is, the more the
  interface text moves towards white (black for light themes).
- **Blur what's behind (frosted)**: blurs the desktop behind the window. Works on
  COSMIC and other compositors with `ext-background-effect`, and on KDE.

Translucency needs **Wayland**. On X11 the window stays opaque, and the settings
say so.

## Fonts

- **Console:** any installed monospace family. The tab bar uses it too. Default:
  Noto Sans Mono.
- **Menus:** any installed family, for the sidebar and settings.
- **Font size** and **line height** apply live. <kbd>Ctrl</kbd>+<kbd>+</kbd> /
  <kbd>-</kbd> / <kbd>0</kbd> zoom temporarily without changing the setting.

A font that's no longer installed falls back to the default; the list marks it
"(not installed)".

## Window

- **Padding** around the terminal grid (0–40 px)
- **Sidebar width** – applies when you let go of the slider
- **Show tab bar**
- **Cursor:** blinking on/off and interval

Window size at start, showing the sidebar at start and the start-up animation are
under **Settings → General** and apply at the next start. **Use current** takes
the window's current size.
