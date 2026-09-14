# Built-in commands and snippets

The sidebar's **Shells** section ends with command buttons: built-in ones for
everyday chores, and your own (snippets). A click sends the command to the active
tab – or to every tab in the broadcast group.

## Built-in commands

| Group | Button | Example (Arch) |
| --- | --- | --- |
| Packages | Update the system | `sudo pacman -Syu` |
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

Hover a button to see the exact line before clicking. Only **Update the system**
changes anything – that's why it's highlighted. There's deliberately no "clean
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
- **Only on host:** show it only in SSH tabs of that host (by its name in the
  sidebar). Such a snippet never shows in local tabs.
- **Edit/delete:** under **Manage**; deleting asks once more.

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
