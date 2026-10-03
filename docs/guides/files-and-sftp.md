# Files on a server (SFTP)

Every SSH connection can show its server's files: browse folders, copy files
and folders in both directions, keep folders in sync, and edit a file in your
local editor – every save goes back to the server. Files only root may change
work too, through sudo in the terminal.

## Opening the files tab

In a terminal connected over SSH, press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd>
or right-click and choose **Files (SFTP)**. A tab **Files: user@host** opens
next to the terminal; pressing the shortcut again in that terminal switches to
it.

The files tab uses the terminal's connection – no second login, no second
passphrase prompt. It needs the server's SFTP subsystem, which OpenSSH servers
have by default. Opened before the login in the terminal is done, it waits for
it. If the server has no SFTP, the tab says why; **Reconnect** tries again.

Closing the files tab ends the SFTP session, the terminal keeps running.

## When the connection drops

The files tab checks every second whether its connection is still there. When
it's gone, it says **Connection interrupted**; transfers and saves wait. As soon
as the terminal is connected again – it reconnects by itself – the files tab
carries on:

- A **download** continues from its hidden `.name.part` file, an **upload** from
  the part already on the server – as long as the source file didn't change
  meanwhile; otherwise it starts over.
- **Saves** you made in the editor meanwhile are uploaded, after the usual check
  that nobody changed the file.

If you close the terminal, the files tab stays and says so. Open the same host
with the same login again (from the sidebar, for example) and the files tab takes
over the new connection. This lasts as long as Terminaal is open.

## Browsing

The left side is this computer, the right side the server.

- **Double-click** a folder to open it. On the server, double-clicking a file
  edits it (see below); a symlink opens whatever it points to.
- **⬆** goes to the parent folder, **~** (server) to the home folder, **⟳**
  reloads.
- Click the path above a side to type one and press <kbd>Enter</kbd>
  (`~/` works on the local side).
- **Click** an entry to select it. <kbd>Ctrl</kbd>+click adds or removes one
  more, <kbd>Shift</kbd>+click selects everything from the last click to this
  one (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+click adds that range). With more than one
  selected, the buttons show how many.
- **New folder**, **Rename** and **Delete** on the server side act on the
  server. Delete removes all selected entries – folders with everything in them
  (symlinks inside are removed, not followed) – and asks with **Really delete?**
  first. **Rename** and **Edit** need exactly one entry selected.
- **Delete** on the local side moves the selected entries to the desktop's
  trash (with `gio trash`), after the same **Really delete?**.

## Copying

- Select local entries and click **Upload**: they go into the server folder on
  show. Select server entries and click **Download**: they go into the local
  folder on show.
- Folders are copied with everything in them. Symlinks inside a folder are
  skipped.
- **Drag files from your file manager** onto the files tab to upload them.
- **Nothing is ever overwritten.** If the name is taken, the copy gets a
  number: `notes (1).txt`.
- **Transfers** below the lists show progress; **×** cancels one and removes
  what was copied so far. **Remove finished** clears the list.

Transfers run one after another, each with many requests in flight at once, so
they stay fast even over a slow round trip.

## Syncing folders

A host can keep pairs of folders in sync – a folder here and one on the server.
Set them up in the host form under **Folder sync** (saved hosts only):

| Field | In hosts.toml | Meaning |
| --- | --- | --- |
| Direction | `direction` | **Two-way** (`both`, the default): whatever changed goes to the other side. **Upload only** (`upload`): this computer's folder is the original. **Download only** (`download`): the server's is |
| Folder here | `local` | Absolute, or starting with `~/`. Created if it doesn't exist yet on the first sync |
| Folder on the server | `remote` | Absolute (`/srv/www`), or relative to the login's home folder (`site`, `~/site`). Also created on the first sync |
| In the background, live | `live` | See [When it syncs](#when-it-syncs) |
| Delete on the server what's deleted here | `delete_remote` | Two-way and upload only. Off by default |
| Delete here what's deleted on the server | `delete_local` | Two-way and download only. Off by default |
| Leave out | `exclude` | Names like `.git` or `*.log`; a pattern with a `/` is a path inside the folder (`build/out`). `*` and `?` are wildcards |

In `hosts.toml`, each pair is a `[[host.sync]]` table after the host's other
settings:

```toml
[[host]]
name = "web1"
host = "web1.example.com"
user = "deploy"

[[host.sync]]
local = "~/Projects/site/public"
remote = "/srv/www/site"
direction = "upload"
delete_remote = true
live = true
exclude = [".git", "*.tmp"]

[[host.sync]]
local = "~/Notes"
remote = "notes"
```

The pairs apply to every login of the host. Saving the host form applies them
right away – to open files tabs and background syncs too.

### When it syncs

- A normal pair syncs **when you open the files tab** of a terminal on that host,
  and again after every reconnect.
- A **live** pair syncs **in the background** as long as a terminal to the host
  is open – no files tab needed. It syncs when the terminal has logged in, after
  every reconnect, and about a second after something changes in its local
  folder (a folder that keeps changing waits up to 10 seconds). Changes on the
  server show up with the next sync; there's no watching the server.
- **Sync now** in the files tab syncs a pair right away; **Stop** ends a sync
  under way – what's copied stays copied.

The files tab lists all of the host's pairs under **Synced folders** – the live
ones too, marked **live** – with what they're doing and when they were last in
sync. Copies of a sync don't show up under **Transfers**.

### How it decides

After each sync, Terminaal remembers every file's size and modification time on
both sides. Next time, a file that differs from that changed on that side.
Copies keep their source's modification time, so after a sync both sides agree
even without that record.

- **Changed on one side:** copied to the other, replacing the old version. The
  copy is written next to the target under a hidden name and renamed over it at
  the end, so an interrupted copy never leaves half a file. A file on the server
  keeps its mode; new files get the mode of their source.
- **Changed on both sides** (two-way), or in one-way mode **the target changed
  too:** nothing is lost. If both versions are the same size, their SHA-256 is
  compared first (on the server with `sha256sum` or `shasum`) – the same content
  is no conflict. Otherwise the other version is kept next to the file as
  `name (conflict).ext`: in two-way mode the newer version takes the name and the
  older one is kept on both sides; one-way, the target's own version is kept, on
  the target only. The files tab lists each conflict.
- **New on one side:** copied to the other – except in one-way mode, where a file
  only the target has is left alone.
- **Deleted on one side:** by default it comes back from the other side (in
  one-way mode, a file deleted on the target comes back; one deleted at the
  source stays on the target). In two-way mode the files tab then says how many
  came back and which switch would have deleted them instead. With deleting switched on for that direction, the
  other side's file is deleted too – but only if it was synced before and hasn't
  changed since. A change always wins over a deletion. Folders are deleted only
  when they're empty by then.
- **A side whose folder is completely empty never deletes anything** on the other
  side – an unmounted disk or a wrong path mustn't empty the server.

Left out on both sides: symlinks, names that aren't valid UTF-8 or contain a
line break, Terminaal's own `.name.part` files and whatever matches **Leave out**.
A folder that can't be read is left alone as a whole, and a name that's a file on
one side and a folder on the other is reported and skipped.

### Where the record lives

In `$XDG_STATE_HOME/terminaal/sync/` (else `~/.local/state/terminaal/sync/`), one
file per host login and folder pair. Delete it to start over: the next sync then
treats both sides as new – equal files are recognized, different ones become
conflicts.

A pair syncs in one place at a time. If another files tab or a second Terminaal
is syncing it right now, it says **running elsewhere** and tries again 30 seconds
later.

## Editing a server file locally

Select a file and click **Edit**, or double-click it. Terminaal downloads it and
opens it in your editor. Whenever you save, the change goes back to the server.
The section **Edited locally** lists these files with their state:

| State | Meaning |
| --- | --- |
| on the server | Your last save is on the server |
| uploading … | It's on its way |
| changed on the server meanwhile | Someone else changed the file since you opened it. Nothing was overwritten – choose **Overwrite** (your version wins) or **Take the server's version** (your changes are discarded and the file reloads) |
| no permission to read / write | See [Files only root may change](#files-only-root-may-change) |

**Open in editor** opens it again; **Close** deletes the local copy (with
unsaved changes it asks **Discard changes?** first).

### Which editor

By default the desktop's default application for the file type opens it
(`xdg-open`). To pick one, set **Settings → Shell → Editor for files from the
server**, or `editor` in `config.toml`: a command the path is appended to, such
as `code`, `gedit`, `kate` or `alacritty -e nvim`.

### How it stays safe

- The local copy lives in a private folder (`$XDG_RUNTIME_DIR/terminaal/edit/`,
  mode `0700`, the file `0600`). That folder is in memory and gone when you log
  out. Closing an edit, or the files tab, deletes it – except a copy with changes
  the server never got, which stays until you close it.
- Before uploading, Terminaal checks that the server's file is still what it
  last synced: the server computes its SHA-256 (`sha256sum`, or `shasum` on macOS
  and BSD) and only that one line comes back, however big the file. Without either
  tool, files up to 1 MB are downloaded and compared, bigger ones by size and
  modification time.
- The upload writes a hidden file next to the original and renames it over the
  original, with the original's mode – an interrupted upload never leaves half a
  file. Where that would change the file's owner, or the server can't rename
  over a file, it's written in place instead.
- Files over 32 MB aren't opened for editing; download them instead.

## Files only root may change

When the server says **no permission to read** or **no permission to write**,
click **With sudo …**. Terminaal then:

1. Puts a small script into `~/.cache/terminaal/sudo/<random>/` on the server –
   for saving, together with your version of the file.
2. Shows the command to run, like ` sh '/home/you/.cache/terminaal/sudo/…/run.sh'`.
3. **Run in terminal** pastes it into the connection's terminal, runs it and
   switches to that terminal. sudo asks for your password there, as usual.

The script reads the file with `sudo cat`, or writes it with `sudo cp` – writing
into the existing file, so it keeps its owner and mode – and leaves its exit
code for Terminaal. Once it's done, the edit continues: the file opens in your
editor, or the save is marked as on the server. A file opened this way saves
through sudo each time; since sudo remembers your password for a few minutes,
running the next command is usually just a click.

Things to know:

- **Run it at a shell prompt.** The command is typed into the terminal like a
  paste – if an editor or another program is running there, it goes there.
- Only the short command passes through your shell, whatever shell that is. The
  paths are inside the script, which `sh` reads.
- The command starts with a space, so shells set to ignore such lines leave it
  out of their history.
- **Cancel** removes the script again. The script deletes itself when it has run,
  and Terminaal removes its folder.
- The script compares the file's SHA-256 with what you last synced right before
  copying. If someone changed it meanwhile, it copies nothing and the edit shows
  the conflict; **Overwrite** then makes a script without that check.

## Limits

- Folder sync compares by size and modification time (to the second); a change
  that keeps both isn't noticed. Live pairs watch only the local folder; inotify
  has a per-user limit on watched folders (`fs.inotify.max_user_watches`), beyond
  which changes in further folders wait for the next sync.
- Scanning a pair goes through the whole tree on both sides; while it runs, the
  files tab's other actions wait.
- Nothing is overwritten on copying; to replace a file, delete it first or edit it.
- Deleting on the server is final – there's no trash there.
- Transfers resume only while Terminaal stays open.
- On a server without `sha256sum` or `shasum`, a change within the same second
  that keeps the size isn't noticed for files over 1 MB, or at all for files
  edited through sudo.
