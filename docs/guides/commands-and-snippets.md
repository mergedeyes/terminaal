# Built-in commands and snippets

The sidebar's **Shells** section ends with command buttons: built-in ones for
everyday chores, and your own (snippets). A click sends the command to the active
tab – or to every tab in the broadcast group.

## Built-in commands

| Group | Button | Example (Arch) |
| --- | --- | --- |
| Packages | Update the system | `sudo pacman -Syu` |
| | Update Flatpaks | `flatpak update` – only where Flatpak is installed |
| | Update AUR packages | `paru -Sua` or `yay -Sua` – only where one of them is installed, not as root |
| | Available updates | `pacman -Qu` |
| Disk | Free space | `df -h -x tmpfs -x devtmpfs -x efivarfs` |
| | Folder sizes here | `du -h --max-depth=1 . 2>/dev/null \| sort -h` |
| System | Memory | `free -h` |
| | Top processes | `ps -eo pid,user,pcpu,pmem,comm --sort=-pcpu \| head -n 16` |
| | Uptime | `uptime` |
| | Failed services | `systemctl --failed` |
| | Errors in the log | `journalctl -p err -b --no-pager \| tail -n 50` |
| Network | Open ports | `sudo ss -tulpn` |
| | IP addresses | `ip -brief address` |

Hover a button to see the exact line before clicking. Only the three update
buttons change anything – that's why they're highlighted. Flatpak and the AUR
helper get buttons of their own, so updating the system doesn't also pull in
every Flatpak and AUR package. They show wherever the tool is found: locally on
the `PATH`, over SSH in the same probe that detects the system (paru wins if
both helpers are installed). A host with its system set by hand skips that
probe. If that system is Arch (or a derivative), it still gets the AUR button,
whose line picks paru or yay when it runs. It gets no Flatpak button, since
nothing checked whether Flatpak is there. There's deliberately no "clean
up" button: orphaned packages aren't necessarily unused.

### Tailored per system

The lines depend on the system the active tab is on:

| System | Package manager |
| --- | --- |
| Arch, CachyOS, Manjaro, EndeavourOS | pacman |
| Debian, Ubuntu, Mint, Pop!_OS | apt |
| Fedora, RHEL, Rocky, Alma | dnf |
| openSUSE, SLES | zypper |
| Alpine | apk |
| Void | xbps |
| Gentoo | emerge |
| NixOS | nixos-rebuild |
| macOS | brew |
| FreeBSD | pkg |

- **Local tabs:** read from `/etc/os-release` (`ID`, then `ID_LIKE`, so
  derivatives land in the right family).
- **SSH tabs:** Terminaal runs `id -u; uname -s; cat /etc/os-release` in a
  separate channel right after login. Nothing of it appears in the tab; it can
  hold the tab for up to two seconds on a slow link.
- **Override:** **Settings → Shell → System** for local tabs; for a host,
  **Advanced → Session → System (for the commands)** – which also skips the
  detection.
- As **root**, `sudo` is left out. On an unknown system only the commands every
  system has are offered, nothing that changes anything.

### Running vs. typing, and confirmations

- **Run commands right away** (`commands_run`, on): a click runs the line. Off:
  the line is typed into the prompt and you press Enter.
- The first time a click would run something, a warning explains that commands go
  straight to the shell. Confirm once and it's remembered.
- **Let commands skip their prompts** (`commands_assume_yes`, off): adds
  `--noconfirm`/`-y`, so updates run without asking – even when they'd replace or
  remove packages. Leave it off unless you know what you're doing. (Gentoo works
  the other way round: without the setting, `--ask` is added.)

## Snippets

Your own commands, in `~/.config/terminaal/snippets.toml`.

- **Add:** **Your commands → Manage → + Add command**. Give it a name and a
  command – several lines are fine.
- **Only on system:** show it only in tabs detected as that system.
- **Where:** **Everywhere (local and all hosts)**, **Local only** (never in
  SSH tabs), or one host – then only in SSH tabs of that host, by its name in
  the sidebar. **After login** doesn't go with **Local only**; the form says so.
- **Edit/delete:** under **Manage**; deleting asks once more.
- **Run automatically:** a snippet can also run by itself when a terminal
  starts – see [Startup commands](#startup-commands).
- **Hide the button:** the snippet gets no button among the commands, so many
  startup commands don't crowd the sidebar. It's still listed under **Manage**,
  where ▶ runs it in the active tab.

Sending works like pasting: several lines arrive together (with bracketed paste,
if the shell turned that on), and with **Run commands right away** one Enter
follows. The first-run warning applies to snippets too.

Example `snippets.toml`:

```toml
[[snippet]]
name = "Docker cleanup"
command = "docker system df && docker image prune"

[[snippet]]
name = "Restart app"
command = "sudo systemctl restart app && journalctl -fu app"
host = "web1"
```

### Startup commands

Set **Run automatically** in a snippet's form:

| Choice | Runs in | When |
| --- | --- | --- |
| **Never, button only** | – | Only when you click it (the default) |
| **With the shell** | Every new terminal, local and SSH | Once its shell is ready |
| **After login** | SSH terminals only | Once logged in and the shell is ready |

Both run again after an SSH terminal reconnects, and in every new tab, split
pane and restored session. **Only on system** and **Where** apply as
for buttons, so a snippet bound to `web1` with **After login** runs on every
login to `web1` and nowhere else. The button stays as well.

"Ready" means the shell showed its prompt (fish, bash and zsh tell Terminaal
through shell integration). A shell that doesn't say gets the commands once its
output has paused for half a second, or after eight seconds of silence. Startup
commands go in like a paste with one Enter each, whatever **Run commands right
away** says. They go only to that terminal, never to a broadcast, and there's no
first-run warning: you set them up to run.

```toml
[[snippet]]
name = "tmux"
command = "tmux new -A -s main"
host = "web1"
autorun = "login"     # or "shell"
hidden = true         # no button, only under Manage
```

## With broadcast

When the focused terminal is in the broadcast group (red line in the tab bar,
red frame in a split tab), the buttons send to every terminal of the group, and
the sidebar says *Broadcast: goes to N terminals*.

- Snippets are sent as they are.
- Built-in commands send the line for the **active** tab's system. Don't mix
  systems in the group when you use them.

See the tutorial [One command on many servers](../tutorials/04-one-command-many-servers.md).

## Aliases and functions

Also in the Shells section: aliases and functions for fish, bash and zsh. They
go into a file of their own that only Terminaal loads –
`~/.config/fish/terminaal.fish`, `~/.bash_terminaal` or `$ZDOTDIR/.zsh_terminaal` –
so your other terminals are unaffected. Aliases and functions share one
namespace per shell; the form refuses a name that's already taken.
