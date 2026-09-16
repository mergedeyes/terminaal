# Keyboard shortcuts

## Defaults

| Action | Name in `config.toml` | Default |
| --- | --- | --- |
| New tab | `new_tab` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> |
| Close tab with all its panes | `close_tab` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>W</kbd> |
| Next tab | `next_tab` | <kbd>Ctrl</kbd>+<kbd>Tab</kbd> |
| Previous tab | `previous_tab` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd> |
| Go to tab 1…9 | `tab_1` … `tab_9` | <kbd>Alt</kbd>+<kbd>1</kbd> … <kbd>9</kbd> |
| Move tab left / right | `move_tab_left`, `move_tab_right` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> |
| Split right | `split_right` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> |
| Split down | `split_down` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd> |
| Close pane (the tab with its last one) | `close_pane` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> |
| Focus pane on the left / right / above / below | `focus_pane_left`, `focus_pane_right`, `focus_pane_up`, `focus_pane_down` | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> / <kbd>→</kbd> / <kbd>↑</kbd> / <kbd>↓</kbd> |
| Maximize pane / restore | `zoom_pane` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> |
| Broadcast on/off for the terminal | `toggle_broadcast` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> |
| Files of the SSH connection (SFTP) | `open_files` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> |
| Watch the terminal for silence on/off | `watch_silence` | none |
| Show/hide sidebar | `toggle_sidebar` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> |
| Open settings | `open_settings` | <kbd>Ctrl</kbd>+<kbd>,</kbd> |
| Command palette | `command_palette` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> |
| Copy | `copy` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> |
| Copy the last command's output | `copy_last_output` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>C</kbd> |
| Paste | `paste` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> |
| Paste and run | `paste_and_run` | none |
| Scroll a page up / down | `scroll_page_up`, `scroll_page_down` | <kbd>Shift</kbd>+<kbd>PageUp</kbd> / <kbd>PageDown</kbd> |
| Scroll to top / bottom | `scroll_to_top`, `scroll_to_bottom` | <kbd>Shift</kbd>+<kbd>Home</kbd> / <kbd>End</kbd> |
| Search the scrollback | `search` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> |
| Previous / next prompt | `previous_prompt`, `next_prompt` | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd> / <kbd>↓</kbd> |
| Font bigger | `font_bigger` | <kbd>Ctrl</kbd>+<kbd>+</kbd> and <kbd>Ctrl</kbd>+<kbd>=</kbd> |
| Font smaller | `font_smaller` | <kbd>Ctrl</kbd>+<kbd>-</kbd> |
| Font size back | `font_reset` | <kbd>Ctrl</kbd>+<kbd>0</kbd> |

Not configurable: <kbd>Ctrl</kbd>+click opens links, <kbd>Shift</kbd>+wheel always
scrolls the scrollback, and the keys inside the search bar
(see [Search, links and the scrollback](search-and-links.md)).

## Changing them in the settings

**Settings → Shortcuts** lists every action with its combinations:

- **+** records a new one: press the keys. <kbd>Esc</kbd> cancels.
- **×** on a combination removes it.
- **↺** brings back the defaults.

Recording refuses two kinds of combinations:

- **Already taken** by another action – the message names it.
- **Ones that would swallow typing**: without <kbd>Ctrl</kbd>, <kbd>Alt</kbd> or
  <kbd>Super</kbd>, only F-keys and <kbd>Shift</kbd> with a navigation key are
  allowed. <kbd>Shift</kbd>+<kbd>A</kbd> as a shortcut would mean you could never
  type a capital A.

Hover an action's name to see its key in `config.toml`.

## Changing them in config.toml

```toml
[shortcuts]
new_tab = "Ctrl+Alt+N"
copy = ["Ctrl+Shift+C", "Ctrl+Insert"]
paste_and_run = "Ctrl+Shift+Alt+V"
tab_9 = []
```

- A string for one combination, an array for several, `[]` for none.
- Modifiers: `Ctrl`, `Shift`, `Alt`, `Super`, joined with `+`, in any order.
- Keys: letters and digits as themselves; `Plus`, `Minus`, `Comma`, `Period`;
  `Tab`, `Enter`, `Escape`, `Space`, `Backspace`, `Delete`, `Insert`, `Home`,
  `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, `Right`, `F1`–`F12`.
- Keys are matched as your keyboard layout labels them without modifiers:
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> is the T key, whatever Shift makes of it.
- A value Terminaal can't read falls back to the default and logs a warning.
  The config file isn't checked for "swallows typing" – that's your call.

## Rules worth knowing

- **One combination, several actions?** The action listed first above wins; the
  settings tab strikes the combination through at the other one.
- **Clipboard, scrolling, search and prompt jumps** don't fire while a text field
  in the sidebar or settings has the keyboard – <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>
  pastes into the field then. Tab, pane, window and font shortcuts work everywhere.
- **Full-screen programs** (`less`, `vim`, `htop`) get the scrolling and
  prompt-jump keys themselves – they have no scrollback of their own in Terminaal.
- **Font zoom** isn't saved: it resets when Terminaal quits, or when you set a new
  size in the settings.
