# Restoring the last session

When Terminaal starts, it opens the tabs you had open last time: same order,
same splits, and the tab you were in comes up in front.

## What comes back

| Tab | Restored as |
| --- | --- |
| Local shell | The same shell, in the working directory it was last in |
| SSH terminal | A new connection to the same host with the same login |
| Split panes | The same layout, divider positions and focused pane; a maximized pane stays maximized |
| Settings | The settings tab |
| Files (SFTP) | The files tab over the restored terminal to that host, in the folder it showed |

The working directory needs [shell integration](shell-integration.md) (fish,
bash and zsh have it in Terminaal). A shell that never reports its directory
starts in the folder it started in last time.

SSH terminals connect again as if you had opened them from the sidebar.
Passphrase and password prompts come up in their tabs, and changes you made
to the host in the meantime apply. If a host was deleted or renamed, its
terminal is left out. A files tab without its terminal is left out too.

What doesn't come back:

- **The scrollback** and anything running in the shells. Every terminal starts
  fresh.
- **Broadcast.** No terminal is in the broadcast group after a start, so input
  never goes to several servers without you choosing it.
- **Search, zoom and the window size.** The window opens at the size from
  Settings → General.

## When it's saved

Terminaal saves the session a couple of seconds after you open, close,
split, move or switch tabs, or `cd` somewhere. It also saves when you close
the window. If Terminaal crashes or your computer turns off, the next start
still finds your tabs from a few seconds before.

**Closing the last tab** ends Terminaal with nothing to restore. That also
happens when you type `exit` in the last shell. The next start opens one tab
with your default shell. To keep your tabs for next time, close the window
instead.

## Several windows

Only the first Terminaal that's running restores and saves the session. A
second window, started while the first is still open, starts with a single
tab and doesn't touch the saved session. Without that, the second window would
open all your SSH connections again, and the two windows would overwrite each
other's tabs.

`terminaal --connect HOST` never restores the session and never saves it.

## Turning it off

Untick **Restore last session** under Settings → General, or set
`restore_session = false` in `config.toml`. Turning it off deletes the saved
session. From then on Terminaal starts with one tab.

## The file

The session is stored in `~/.local/state/terminaal/session.toml` (or
`$XDG_STATE_HOME/terminaal/`). It holds shell paths, directories, and host
names with user names. Nothing secret is in it.

You can edit or delete it while Terminaal isn't running. If the file can't be
read, Terminaal starts with one tab and the next save replaces the file.

```toml
active = 0              # the tab in front, counted from 0

[[tab]]
kind = "terminals"
focus = 1               # the pane with the keyboard, an index into the panes below
# Leaves are pane indices; a split has axis "horizontal" (side by side)
# or "vertical" (one above the other) and the first half's share.
layout = { axis = "horizontal", ratio = 0.5, first = 0, second = 1 }
# zoomed = true         # the focused pane over the whole tab

[[tab.panes]]
kind = "shell"
shell = "/usr/bin/fish"
cwd = "/home/me/src"    # optional

[[tab.panes]]
kind = "ssh"
host = "web1"           # the host's name in the sidebar
user = "admin"

[[tab]]
kind = "files"
host = "web1"
user = "admin"
dir = "/var/log"        # optional

[[tab]]
kind = "settings"
```

A tab whose layout doesn't name each of its panes exactly once is left out.
