# Files on a server (SFTP)

Every SSH connection can show its server's files: browse folders, copy files
and folders in both directions, and edit a file in your local editor – every
save goes back to the server. Files only root may change work too, through sudo
in the terminal.

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
- **New folder**, **Rename** and **Delete** act on the server. Delete removes
  files and empty folders, and asks with **Really delete?** first.

## Copying

- Select a local entry and click **Upload**: it goes into the server folder on
  show. Select a server entry and click **Download**: it goes into the local
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

- Deleting a folder needs it to be empty.
- Nothing is overwritten on copying; to replace a file, delete it first or edit it.
- Transfers resume only while Terminaal stays open.
- On a server without `sha256sum` or `shasum`, a change within the same second
  that keeps the size isn't noticed for files over 1 MB, or at all for files
  edited through sudo.
