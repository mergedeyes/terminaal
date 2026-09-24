# Configuration reference

Terminaal keeps everything in `~/.config/terminaal/` (or
`$XDG_CONFIG_HOME/terminaal/`). All files are optional, and none holds a
password or passphrase.

| File | What's in it | Who writes it |
| --- | --- | --- |
| [`config.toml`](#configtoml) | Options and shortcuts | You and the settings tab |
| [`hosts.toml`](#hoststoml) | Saved SSH hosts | The SSH section (you may edit it) |
| [`keys.toml`](#keystoml) | Named SSH keys | The Keys section (you may edit it) |
| [`snippets.toml`](#snippetstoml) | Your own commands | "Your commands" (you may edit it) |
| `themes/*.toml` | Your color themes | You – see [Appearance](appearance.md) |
| `shell-integration/` | Startup files for bash, zsh and fish | Terminaal – don't edit, they're regenerated |

The open tabs are kept elsewhere, in `~/.local/state/terminaal/session.toml`
(or `$XDG_STATE_HOME/terminaal/`) – see [Restoring the last session](sessions.md).

## config.toml

The settings tab edits this file in place: it changes just the key you touched
and keeps your comments and formatting. A missing file means defaults.

### Appearance

| Key | Default | Meaning |
| --- | --- | --- |
| `theme` | `"Terminaal"` | Theme name, any case. See [Appearance](appearance.md) |
| `font_family` | Noto Sans Mono | Console and tab bar font; must be an installed monospace family |
| `ui_font_family` | built in | Font of the sidebar, menus and settings |
| `font_size` | `15.0` | In logical pixels |
| `line_height_factor` | `1.25` | Line height as a multiple of the font size |
| `padding` | `8.0` | Space around the terminal grid, in logical pixels |
| `opacity` | `1.0` | `0.2`–`1.0`; below 1 the window is see-through (Wayland only) |
| `blur` | `true` | Blur what's behind a see-through window |
| `cursor_blink` | `true` | |
| `cursor_blink_interval_ms` | `600` | |

### Window and startup

| Key | Default | Meaning |
| --- | --- | --- |
| `default_width`, `default_height` | `1000.0`, `650.0` | Window size at start |
| `tab_bar` | `true` | Show the tab bar |
| `sidebar` | `true` | Show the sidebar at start |
| `sidebar_width` | `300.0` | |
| `splash` | `true` | Start-up animation |
| `quake_height` | `50` | Height of the drop-down window in percent of the screen, `20`–`100`. See [The drop-down window](drop-down-window.md) |
| `quake_hide_on_unfocus` | `true` | Hide the drop-down window when another window gets the keyboard |
| `restore_session` | `true` | Open last time's tabs again. Turning it off deletes the saved session. See [Restoring the last session](sessions.md) |
| `language` | from the locale | `"en"` or `"de"` |

### Terminal

| Key | Default | Meaning |
| --- | --- | --- |
| `shell` | `$SHELL` | Default shell for new tabs, a path |
| `scrollback_lines` | `10000` | Lines kept per tab; lowering it drops the oldest |
| `scroll_lines` | `3.0` | Lines per mouse-wheel notch |
| `scroll_select_factor` | `3.0` | How much faster the wheel scrolls while text is being marked (1–10, `1` for the usual speed) |
| `notify_after_secs` | `10` | Notify when a command that ran this long finishes unseen; `0` = never |
| `silence_secs` | `15` | A terminal watched for silence counts as quiet after this many seconds without output (3–300) |
| `paste_warning` | `true` | Ask before pasting several lines that would run at once, `sudo`, a download piped into a shell, `rm -rf` and the like |

### Built-in commands

| Key | Default | Meaning |
| --- | --- | --- |
| `commands_run` | `true` | A click runs the command; `false` only types it into the prompt |
| `commands_assume_yes` | `false` | Add `-y`/`--noconfirm` so updates don't ask |
| `commands_warned` | `false` | Set once you confirmed the first-run warning |
| `commands_collapsed` | `[]` | Command groups folded away in the sidebar; set by clicking their headings (built-in groups by key, your categories as `custom:<name>`) |
| `system` | detected | Which system local tabs' commands are for: `arch`, `debian`, `fedora`, `suse`, `alpine`, `void`, `gentoo`, `nixos`, `macos`, `freebsd` |

### Files

| Key | Default | Meaning |
| --- | --- | --- |
| `editor` | unset | Command a server file edited locally opens with, its path appended (`code`, `gedit`, `alacritty -e nvim`). Unset: the desktop's default application. See [Files on a server](files-and-sftp.md) |

### Shortcuts

```toml
[shortcuts]
new_tab = "Ctrl+Alt+N"                    # one combination
copy = ["Ctrl+Shift+C", "Ctrl+Insert"]    # several
tab_9 = []                                # none
```

Actions left out keep their defaults. See [Keyboard shortcuts](keyboard-shortcuts.md)
for the action names and key syntax.

## hosts.toml

One `[[host]]` table per saved host. The keys of the per-host options are the
`ssh_config` keywords in `snake_case`.

```toml
[[host]]
name = "web1"                 # shown in the sidebar; defaults to `host`
host = "server.example.com"
port = 22
user = "deploy"               # empty: your local user name
key = "Work"                  # a name from keys.toml; leave out for automatic

# Further logins (the first one above is the default)
[[host.login]]
user = "root"
key = "1Password"

[[host]]
name = "db1"
host = "10.0.0.5"
user = "deploy"
proxy_jump = "web1"           # a host name, or [user@]host[:port], comma-separated for a chain
identity = ["~/.ssh/id_db"]   # IdentityFile, one or more
identities_only = true
identity_agent = "~/.1password/agent.sock"
connect_timeout = 10
server_alive_interval = 30    # 0 turns keepalives off
server_alive_count_max = 3
compression = true
address_family = "inet"       # any, inet, inet6
# proxy_command = "nc -X connect -x proxy:3128 %h %p"   # instead of proxy_jump, not both
preferred_authentications = "publickey,keyboard-interactive,password"
forward_agent = "yes"         # yes, a socket path, or $VARIABLE
strict_host_key_checking = "accept-new"   # ask, accept-new, yes
user_known_hosts_file = "~/.ssh/known_hosts_work"
remote_command = "tmux new -A -s main"
set_env = ["LANG=en_US.UTF-8", "TERM=xterm-256color"]
send_env = ["LC_*"]
local_forward = ["5432 db.internal:5432"]
remote_forward = ["8080 localhost:3000"]
dynamic_forward = ["1080"]
kex_algorithms = "+diffie-hellman-group14-sha1"
host_key_algorithms = "^ssh-ed25519"
ciphers = "aes256-gcm@openssh.com"
macs = "hmac-sha2-256"
system = "debian"             # Terminaal's own: skip detecting the system
color = "red"                 # Terminaal's own: red, orange, yellow, green, blue, purple or "#rrggbb"
theme = "Dracula"             # Terminaal's own: console colors of this host's terminals
```

`proxy_jump` and `proxy_command` exclude each other. Details on every option:
[SSH hosts](ssh-hosts.md).

## keys.toml

```toml
[[key]]
name = "Work"
file = "~/.ssh/id_ed25519_work"
generated = true              # created in Terminaal: removing may delete the files

[[key]]
name = "1Password"
agent_key = "ssh-ed25519 AAAAC3Nza... laptop"   # public half of a key kept in the agent
```

## snippets.toml

```toml
[[snippet]]
name = "Follow logs"
command = "journalctl -f"
system = "arch"               # optional: only on this system (same values as `system` above)

[[snippet]]
name = "Deploy"
command = """
cd /srv/app
./deploy.sh"""
host = "web1"                 # optional: only in SSH tabs of this host
# local = true                # optional instead of host: only in local terminals
autorun = "login"             # optional: run by itself -- "shell" in every new
                              # terminal, "login" in SSH terminals after login
hidden = true                 # optional: no button, only listed under Manage
```

## Editing files by hand

- Terminaal reads `hosts.toml`, `keys.toml` and `snippets.toml` at startup.
  Edit them while it's closed, or your change may be overwritten the next time
  you save something in the sidebar.
- If one of them can't be read, the sidebar shows the error and refuses to save
  over the file, so a typo never costs you your hosts.
- `config.toml` is safe to edit any time – the settings tab changes only the keys
  you touch there – but your own edits take effect at the next start.
- Theme files are read again with the **Reload** button.
