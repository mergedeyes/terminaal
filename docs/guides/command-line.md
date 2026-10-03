# The command line

Terminaal can be started for one particular job: a server, a shell, a command.
That is what desktop entries, file-manager actions and scripts use.

```sh
terminaal                              # a local shell (or the last session)
terminaal --connect myserver           # a saved or ~/.ssh/config host
terminaal --connect admin@myserver     # ...with that host's login for "admin"
terminaal -s                           # list the installed shells and exit
terminaal -s fish                      # first tab with that shell
terminaal -c "journalctl -fu nginx"    # run that line instead of a shell
terminaal -s bash -c "make" --hold     # ...in bash, and keep the tab afterwards
terminaal --connect web -c "htop"      # run it on the server
terminaal --quake                      # show or hide the drop-down window
terminaal --help                       # the summary; `man terminaal` has it all
```

Anything else is refused with a usage line and exit status 2 — including a
misspelled host or shell name, so a launcher entry fails visibly instead of
opening a window that isn't what you asked for.

## `-c`: one command instead of a shell

`-c` takes **one line**, the way `sh -c` does, not a command with separate
arguments. The shell evaluates it, so pipes, redirections, quoting and several
commands separated by `;` all work:

```sh
terminaal -c "ssh-add -l; echo ---; ssh-add -L | wc -l"
```

Two things follow from "the shell evaluates it":

- The line is one argument. Quote it, and let the shell inside Terminaal do the
  splitting — `terminaal -c ls -l` is a usage error, `terminaal -c "ls -l"` is
  what you want.
- That shell is **not interactive**. As with `sh -c` anywhere else, bash and zsh
  read no startup file, so your aliases — including the ones Terminaal manages
  in the sidebar — are not available. Write out what the alias stands for, or
  call a function from a file you source in the line itself. fish does read its
  `config.fish` for `-c`, so its functions are there.

Which shell runs the line: the one from `-s`, else the default shell
(**Settings → Shell**, else `$SHELL`).

## `--hold`: keep the tab when the command ends

Without `--hold` the tab closes as soon as the command ends — like a shell that
you `exit`. The window goes with the last tab, so a command that fails
immediately leaves nothing to read.

With `--hold` the tab stays and a last line names the status:

```
[ Finished with status 3. Close the tab with Ctrl+Shift+W ]
```

Nothing is running in that tab any more; it is there to be read, scrolled and
copied from. `--hold` works for `--connect` too, where the tab would otherwise
close when the server's `RemoteCommand` ends; the status is the remote
command's, as the server reports it when it closes the channel. If it doesn't
report one in time, the line says only that the command finished. On its own, without `--connect`,
`-s` or `-c`, it is refused: a plain start restores the saved session, and there
is nothing one-off to keep open.

## `-s`: which shell

`-s` takes a name as listed in `/etc/shells` (`fish`) or a path
(`/usr/bin/fish`). A path is started as given, even if it isn't listed — useful
for a shell in `~/.local/bin`. A name that isn't installed is an error, and the
message lists what is.

On its own, `-s` prints the installed shells with their paths and exits — the
same list the sidebar's **Shells** section shows:

```
$ terminaal -s
bash         /bin/bash
fish         /usr/bin/fish
zsh          /bin/zsh
```

## With `--connect`

`--connect` and `-c` together mean: run the line **on the server**. It becomes
that connection's `RemoteCommand`, exactly as if the host had one set in
`~/.ssh/config`, and `-s` is then about the local machine and does nothing for
it. The server's shell evaluates the line, so the same "one line, not
interactive" rules apply there.

```sh
terminaal --connect web -c "sudo journalctl -fu nginx" --hold
```

## The session stays as it is

Started plainly, Terminaal restores the tabs of
[the last session](sessions.md) and keeps saving it. Started with `--connect`,
`-s` or `-c`, it does neither: the window is what the command line asked for,
and the session on disk is left untouched for the next plain start. So a
launcher entry for one server never overwrites the tabs you had open.

## `--quake`

`--quake` shows or hides [the drop-down window](drop-down-window.md) and goes
with no other argument — it keeps its own tabs and its own session. Bind it as
a keyboard shortcut in your desktop's settings; **Settings → General** shows the
command with its full path and copies it for you.

## Desktop entries and scripts

A `.desktop` file for one server, dropped into `~/.local/share/applications`:

```ini
[Desktop Entry]
Type=Application
Name=Logs on web
Exec=/home/you/.cargo/bin/terminaal --connect web -c "sudo journalctl -f" --hold
Icon=terminaal
Terminal=false
```

Use the absolute path to the binary: launchers don't necessarily have
`~/.cargo/bin` in their `PATH`.

Terminaal is a window, not a filter: it does not pass the command's output to
standard output, and its exit status says whether the window started, not how
the command ended. For output in a pipeline use the shell; use Terminaal when
you want a window to watch.
