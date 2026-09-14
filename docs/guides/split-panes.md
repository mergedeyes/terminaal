# Split panes

A tab can hold several terminals at once, side by side or one above the other –
a server's log next to its shell, or three servers in view while you broadcast
to them.

## Splitting

| To… | Press | Or right-click into the terminal |
| --- | --- | --- |
| Put a new pane right of this one | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> | **Split right** |
| Put a new pane below this one | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd> | **Split down** |

The pane you split halves its room with the new one. What the new pane runs
follows the one you split:

- **A local shell**: the same shell, in the same working directory (when the
  shell reports it, see [Shell integration](shell-integration.md)).
- **An SSH session**: a new connection to the same host with the same login.
  Prompts for passphrases or passwords come up again, in the new pane.

A pane that would end up smaller than 8 columns or 2 rows isn't split.

## Moving around

- **Click** into a pane to give it the keyboard.
- <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd>
  moves to the neighbouring pane in that direction.

The pane with the keyboard shows a solid cursor, the others an outlined one. The
tab bar and the window title show its title. Everything that acts on "the
terminal" acts on that pane: copy and paste, search, prompt jumps, the command
buttons, the port forwards listed in the SSH section. Only the **mouse wheel**
scrolls whichever pane is under the mouse.

## Resizing

Drag the thin line between two panes. The mouse pointer turns into a resize arrow
over it. Each side keeps room for at least a few columns or rows.

## Maximizing

<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> shows the focused pane over the
whole tab; press it again for all panes. Moving to another pane with
<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+arrow keeps the tab maximized and shows that one
instead. Splitting, or closing the focused pane, brings all panes back.

## Closing

- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> or **Close pane** in the right-click
  menu closes the focused pane; its neighbour takes over the room.
- Ending the shell (`exit`, <kbd>Ctrl</kbd>+<kbd>D</kbd>) closes its pane too.
- A tab closes with its last pane.
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>W</kbd>, the tab's **×** or
  a middle click on it close the whole tab with all its panes.

## Broadcast

The broadcast group is made of terminals, not tabs:
<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> puts the focused pane in or takes it
out. Panes in the group get a red frame, and their tab a red line in the tab bar.
Typing into a pane of the group reaches every terminal in it, in this tab and in
others. See [One command on many servers](../tutorials/04-one-command-many-servers.md).

## Shortcuts

All of these can be changed under **Settings → Shortcuts → Split panes** or in
`config.toml`: `split_right`, `split_down`, `close_pane`, `focus_pane_left`,
`focus_pane_right`, `focus_pane_up`, `focus_pane_down`, `zoom_pane`. See
[Keyboard shortcuts](keyboard-shortcuts.md).

## Limits

- Panes can only be resized with the mouse.
- The layout isn't saved: Terminaal starts with one tab and one pane.
