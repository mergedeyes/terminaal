# Command palette

<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> (`command_palette` in
[`[shortcuts]`](keyboard-shortcuts.md)) opens a search field at the top of the
terminal. Type a few letters, pick a line with the arrow keys, press
<kbd>Enter</kbd>.

## What it finds

Without a query the list shows everything in this order:

| Kind | Line | Picking it |
| --- | --- | --- |
| **Tab** | the tab's title, with its number | switches to it |
| **Host** | the host's name, one line per login (`user@address` on the right) | opens an SSH tab with that login |
| **Snippet** | your own commands that apply to the active terminal (hidden ones too) | runs it, like its button |
| Action group (**Tabs**, **Split panes**, …) | every keyboard shortcut action, with its shortcut | does what the shortcut does |
| **Theme** | every theme, the one in use marked *current* | switches to it and saves that |
| **Shell** | **New tab:** and each installed shell | opens a tab with it |

Hosts from `~/.ssh/config` show up unless a saved host has the same name. A
snippet runs the way `commands_run` says, and before the very first command
ever runs, the warning in the sidebar comes up instead.

## Matching

- The letters of the query have to appear in order, not necessarily next to
  each other: `spr` finds **Sp**lit **r**ight.
- Matches at the start of a word and runs of letters rank higher; the query as
  one piece ranks highest. Matched letters are highlighted.
- Several words must all match: `prod root` finds the host *prod* with the login
  *root*.
- A word that isn't in the title may match the text on the right (a login, a
  shortcut, a snippet's command) or the kind – it ranks lower then.

## Keys

| Key | |
| --- | --- |
| typing, <kbd>Backspace</kbd> | edit the query |
| <kbd>↑</kbd> / <kbd>↓</kbd>, <kbd>Tab</kbd> / <kbd>Shift</kbd>+<kbd>Tab</kbd> | previous / next line (wraps around) |
| <kbd>PageUp</kbd> / <kbd>PageDown</kbd> | eight lines up / down |
| <kbd>Ctrl</kbd>+<kbd>V</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> | paste the clipboard's first line into the query |
| <kbd>Enter</kbd> or a click | run the line |
| <kbd>Esc</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>, a click elsewhere | close |

While the palette is open, every key belongs to it – nothing reaches the
terminal. The list is made when the palette opens; a host added meanwhile shows
up the next time.
