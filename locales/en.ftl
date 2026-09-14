# Terminaal – English texts.
#
# Fluent syntax: https://projectfluent.org/fluent/guide/
# Every message here must exist in de.ftl too, with the same arguments
# (checked by `cargo test`). Terminal colors, and spaces or line breaks
# around a whole message, are added by the code.

## Common

common-save = Save
common-cancel = Cancel
common-delete = Delete
common-edit = Edit
common-remove = Remove
common-no = No
common-reload = Reload
common-default = Default
common-name-missing = The name is missing.
common-saved = Saved “{ $name }”.
common-deleted = Deleted “{ $name }”.
common-home-unset = $HOME is not set.
common-file-unreadable = Could not read { $path }: { $err }
common-file-invalid = { $path } is invalid: { $err }

## Command line

cli-usage = Usage: terminaal [--connect [USER@]HOST | --quake]
cli-quake-failed = Couldn't start or toggle the drop-down terminal: { $err }
cli-unknown-argument = Unknown argument '{ $arg }'.
cli-unexpected-argument = Unexpected argument '{ $arg }'.
cli-connect-needs-host = --connect needs a host name.
cli-unknown-host = No saved or ~/.ssh/config host named '{ $host }'.

## Config file

config-invalid-toml = { $path } is not valid TOML: { $err }

## Actions carried out by the app

app-shell-start-failed = Could not start { $shell }: { $err }
app-ssh-tab-failed = Could not open an SSH tab for { $target }: { $err }
app-default-shell-set = { $shell } is now the default shell for new tabs.
app-language-changed = Language changed.

## Context menu (right-click into the terminal)

menu-copy = Copy
menu-paste = Paste
menu-paste-run = Paste and run
menu-broadcast-on = Broadcast to this terminal
menu-broadcast-off = Stop broadcasting
menu-split-right = Split right
menu-split-down = Split down
menu-close-pane = Close pane
menu-files = Files (SFTP)

## Scrollback search (render/search_bar.rs)

search-prompt = Search:
search-no-match = No matches
search-hint-typing = Enter ↑ · Shift+Enter ↓ · Esc
search-hint-jumping = n ↑ · N ↓ · / edit · Esc

## Shell integration (terminal/integration.rs, app.rs)

notify-finished = Command finished
notify-failed = Command failed (exit code { $code })
notify-body = { $tab } – after { $duration }
duration-seconds = { $secs } s
duration-minutes = { $mins } min { $secs } s
duration-hours = { $hours } h { $mins } min
prompt-exit = ✘ { $code }

## Key names in shortcuts

key-ctrl = Ctrl
key-shift = Shift
key-alt = Alt
key-super = Super
key-space = Space
key-backspace = Backspace
key-delete = Del
key-insert = Ins
key-home = Home
key-end = End
key-page-up = PgUp
key-page-down = PgDn

## Sidebar

sidebar-keys = Keys
sidebar-settings = Settings

## Sidebar: settings

settings-language = Language
settings-language-auto = Automatic · { $language }
settings-language-auto-hint = Follows the system locale (LANG): German for a German locale, English otherwise
settings-appearance = Appearance
settings-font-size = Font size
settings-line-height = Line height
settings-line-height-value = { $factor } ×
settings-padding = Padding
settings-pixels = { $px } px
settings-tab-bar = Show tab bar
settings-sidebar-width = Sidebar width
settings-cursor = Cursor
settings-cursor-blink = Blinking cursor
settings-cursor-interval = Blink interval
settings-milliseconds = { $ms } ms
settings-scroll = Scrolling
settings-scroll-speed = Mouse wheel
settings-scroll-lines = { $lines ->
    [one] { $lines } line per notch
   *[other] { $lines } lines per notch
}
settings-scroll-speed-hint = Default 3. Touchpads scroll smoothly.
settings-scrollback = Scrollback
settings-scrollback-lines = { $lines ->
    [one] { $lines } line
   *[other] { $lines } lines
}
settings-scrollback-note = Applies to all tabs once released; set lower, it drops the oldest lines.
settings-integration = Shell integration
settings-notify-after = Notify after
settings-notify-never = never
settings-seconds = { $secs } s
settings-integration-note = Notifies when a command ran this long and its tab isn't in view. fish, bash and zsh report prompts and directory in Terminaal by themselves, other shells with OSC 133 and OSC 7.
settings-shell = Default shell
settings-startup = At start
settings-startup-sidebar = Show sidebar
settings-startup-splash = Start-up animation
settings-startup-restore = Restore last session
settings-startup-restore-hint = Opens last time's tabs again: splits, shells with their directory, SSH connections, files and settings tabs. No scrollback. A second Terminaal window always starts fresh.
settings-window-size = Window size
settings-quake = Drop-down window
settings-quake-height = Height
settings-quake-hide = Hide when another window gets the focus
settings-quake-note = The command shows a terminal along the top of the screen and hides it again, starting it the first time. Add it as a custom shortcut in your system settings (COSMIC: Input Devices, Keyboard, View and customize shortcuts, Custom shortcuts). The drop-down window has tabs and a saved session of its own.
settings-quake-copy = Copy command
settings-window-size-current = Use current
settings-window-size-current-hint = Takes the window's current size ({ $width } × { $height })
settings-startup-note = Takes effect the next time Terminaal starts.
settings-key-hint = config.toml: { $key }
settings-note = Changes apply right away and are saved to ~/.config/terminaal/config.toml, keeping its comments and formatting. Hover a setting's name to see its key.
settings-page-general = General
settings-page-terminal = Terminal
settings-page-shell = Shell
settings-page-shortcuts = Shortcuts
settings-font = Font
settings-layout = Window
settings-shell-aliases-hint = Aliases and functions per shell: sidebar, "Shells" section.
settings-editor = Editor for files from the server
settings-editor-default = Default application (xdg-open)
settings-editor-hint = A command the local copy's path is appended to, e.g. “code”, “gedit” or “kate”. Empty: the desktop's default application.
settings-commands-run = Run commands right away
settings-commands-run-hint = Off: the command only lands in the prompt, you press Enter yourself.
settings-commands-yes = Let commands skip their prompts
settings-commands-yes-hint = Adds --noconfirm or -y: updating then runs through without asking, even when that replaces or removes packages.
settings-commands-system = System
settings-commands-system-auto = Automatic · { $system }
settings-commands-system-hint = Decides which commands the sidebar offers for local tabs. For SSH hosts it's in the host form under “Advanced”.
settings-theme = Theme
settings-theme-builtin = Built-in theme
settings-theme-own = Your theme from { $path }
settings-theme-cosmic = Follows the desktop's COSMIC theme, switching between light and dark too
settings-transparency = Translucency
settings-opacity = Opacity
settings-percent = { $value } %
settings-blur = Blur what's behind (frosted)
settings-blur-unsupported = Blur needs a compositor with ext-background-effect (such as COSMIC); on KDE it works anyway.
settings-transparency-unsupported = The graphics driver or compositor offers no see-through windows.
settings-transparency-x11 = On X11 the window stays opaque; see-through needs Wayland.
settings-themes-folder = Your own themes: TOML files in { $path }, in Alacritty's format (its themes work as they are), optionally with [ui] for the interface. After changes, click "Reload".
settings-themes-reloaded = { $count ->
    [one] Loaded { $count } theme.
   *[other] Loaded { $count } themes.
}
settings-font-terminal = Console
settings-font-ui = Menus
settings-font-default = Default · { $font }
settings-font-missing = { $font } (not installed)
settings-font-note = The tab bar uses the console font.

## Themes

theme-missing-color = The color { $key } is missing.
theme-bad-color = { $key }: “{ $value }” is not a color (#rrggbb).

## Settings: keyboard shortcuts

shortcuts-group-tabs = Tabs
shortcuts-group-panes = Split panes
shortcuts-group-window = Window
shortcuts-group-clipboard = Clipboard
shortcuts-group-scroll = Scrolling
shortcuts-group-font = Font size
shortcut-new-tab = New tab with the default shell
shortcut-close-tab = Close tab with all its panes
shortcut-next-tab = Next tab
shortcut-previous-tab = Previous tab
shortcut-select-tab = Tab { $number }
shortcut-move-tab-left = Move tab left
shortcut-move-tab-right = Move tab right
shortcut-split-right = Split right
shortcut-split-down = Split down
shortcut-close-pane = Close pane (the tab with its last one)
shortcut-focus-pane-left = Focus pane on the left
shortcut-focus-pane-right = Focus pane on the right
shortcut-focus-pane-up = Focus pane above
shortcut-focus-pane-down = Focus pane below
shortcut-zoom-pane = Maximize pane / restore
shortcut-toggle-broadcast = Broadcast on/off (input to all marked terminals)
shortcut-open-files = Open the SSH connection's files (SFTP)
shortcut-toggle-sidebar = Show or hide the sidebar
shortcut-open-settings = Open settings
shortcut-command-palette = Command palette (actions, hosts, snippets, tabs, themes)
palette-placeholder = Search actions, hosts, snippets, tabs, themes …
palette-nothing-found = Nothing found.
palette-hint = ↑↓ choose · Enter run · Esc close
palette-kind-tab = Tab
palette-kind-host = Host
palette-kind-snippet = Snippet
palette-kind-theme = Theme
palette-kind-shell = Shell
palette-tab-number = Tab { $number }
palette-theme-current = current
palette-new-tab = New tab: { $shell }
shortcut-copy = Copy
shortcut-paste = Paste
shortcut-paste-and-run = Paste and run
shortcut-scroll-page-up = One page up
shortcut-scroll-page-down = One page down
shortcut-scroll-to-top = To the top of the scrollback
shortcut-scroll-to-bottom = To the bottom
shortcut-search = Search the scrollback
shortcut-previous-prompt = To the previous prompt
shortcut-next-prompt = To the next prompt
shortcut-font-bigger = Bigger
shortcut-font-smaller = Smaller
shortcut-font-reset = Reset
shortcuts-none = not set
shortcuts-add-hint = Record another key combination
shortcuts-press = Press keys …
shortcuts-press-hint = Escape or a click here cancels
shortcuts-remove-hint = Remove { $combo }
shortcuts-reset-hint = Back to the default: { $combos }
shortcuts-shadowed = Also bound to "{ $action }", which gets it.
shortcuts-taken = { $combo } is already bound to "{ $action }"; remove it there first.
shortcuts-swallows-typing = { $combo } would catch normal typing. Shortcuts need Ctrl, Alt or Super, or are an F key or Shift with PgUp/PgDn, Home, End, Ins, Del or an arrow key.
shortcuts-key-hint = config.toml: [shortcuts] { $key }
shortcuts-font-note = Lasts until Terminaal quits; the saved size is under Appearance.
shortcuts-note = Changes apply right away and are saved under [shortcuts] in ~/.config/terminaal/config.toml. Keyboard scrolling doesn't apply in full-screen programs such as less or vim; they get the key.

## Sidebar: shells, aliases and functions

shells-installed = Installed shells
shells-double-click = Double-click opens a new tab
shells-new-tab = ▶  New tab
shells-new-tab-hint = Opens a new tab with { $shell }
shells-make-default = ★  Make default
shells-make-default-hint = New tabs (Ctrl+Shift+T, “+”) start with this shell
shells-managed-title = Aliases & functions · { $shell }
shells-managed-unsupported = Terminaal can't manage aliases or functions for { $shell } – fish, bash and zsh are supported.
shells-managed-file-hint = Only loaded in Terminaal, not in other terminals. Changes apply to newly opened tabs.
shells-save-failed = Saving failed: { $err }
shells-aliases = Aliases ({ $count })
shells-functions = Functions ({ $count })
shells-no-aliases = No aliases yet.
shells-no-functions = No functions yet.
shells-add-alias = +  Add alias
shells-add-function = +  Add function
shells-new-alias = New alias
shells-edit-alias = Edit alias
shells-new-function = New function
shells-edit-function = Edit function
shells-command = Command
shells-body = Body
shells-args-fish = Arguments are in $argv.
shells-args-posix = Arguments: $1, $2, … or "$@".
shells-no-entry-open = No entry open.
shells-command-missing = The command is missing.
shells-body-missing = The body is missing.
shells-name-taken = “{ $name }” already exists ({ $kind }).
shells-saved = Saved “{ $name }” – applies to new { $shell } tabs.

## Sidebar: built-in commands (commands.rs)

cmd-title = Commands
cmd-broadcast = Broadcast: goes to { $count } terminals
snip-title = Your commands
snip-manage = Manage
snip-manage-done = Done
snip-none = No commands of your own yet – “Manage” adds some.
snip-none-here = None of your commands are for this system or host.
snip-add = +  Add command
snip-new = New command
snip-edit = Edit command
snip-name-hint = e.g. Follow logs
snip-command-note = Several lines arrive together, as if pasted.
snip-system = Only on system
snip-all-systems = All systems
snip-host = Where
snip-autorun = Run automatically
snip-autorun-never = Never, button only
snip-autorun-shell = With the shell
snip-autorun-login = After login
snip-autorun-note = "With the shell": in every new terminal once its shell is ready, local and over SSH. "After login": in SSH terminals only, once logged in. Both again after a reconnect, only where system and host match, without asking. The button stays.
snip-hidden = Hide the button
snip-hidden-hint = No button among the commands; it's only listed under "Manage" and runs from there or automatically.
snip-hidden-short = hidden
snip-run = Run
snip-all-hidden = { $count ->
    [one] One hidden command – under "Manage".
   *[other] { $count } hidden commands – under "Manage".
}
snip-all-hosts = Everywhere (local and all hosts)
snip-local-only = Local only
snip-local-login = "After login" is for SSH terminals only and doesn't go with "Local only".
snip-everywhere = Everywhere
snip-on-host = on { $host }
snip-command-missing = The command is missing.
snip-name-taken = There is already a command “{ $name }”.
cmd-system = System: { $system }
cmd-system-local-hint = Detected from /etc/os-release. If that's wrong, set it in the settings under “Shell”.
cmd-system-remote-hint = Detected on { $host } while connecting. If that's wrong, set it in the host form under “Advanced”.
cmd-system-detect = The system is detected automatically again.
cmd-system-probing = Detecting the system…
cmd-system-unknown = System not recognized – the package commands stay hidden. You can set it in the settings under “Shell”.
cmd-no-tab = No terminal tab open – commands need a shell.
cmd-run-hint = Runs right away: { $line }
cmd-type-hint = Types into the prompt: { $line }
cmd-warn-title = Commands run right away
cmd-warn-body = A click sends the command straight to the active tab's shell, Enter included. The settings under “Shell” can switch that to typing it out instead.
cmd-warn-run = Got it, run it
cmd-group-packages = Packages
cmd-group-disk = Disk
cmd-group-system = System
cmd-group-network = Network
cmd-update = Update the system
cmd-update-flatpak = Update Flatpaks
cmd-update-aur = Update AUR packages
cmd-outdated = Available updates
cmd-disk-free = Free space
cmd-disk-usage = Folder sizes here
cmd-memory = Memory
cmd-processes = Top processes
cmd-uptime = Uptime
cmd-failed-services = Failed services
cmd-log-errors = Errors in the log
cmd-ports = Open ports
cmd-addresses = IP addresses
cmd-family-arch = Arch, CachyOS, Manjaro (pacman)
cmd-family-debian = Debian, Ubuntu, Mint (apt)
cmd-family-fedora = Fedora, RHEL, Rocky (dnf)
cmd-family-suse = openSUSE, SLES (zypper)
cmd-family-alpine = Alpine (apk)
cmd-family-void = Void (xbps)
cmd-family-gentoo = Gentoo (emerge)
cmd-family-nixos = NixOS (nixos-rebuild)
cmd-family-macos = macOS (brew)
cmd-family-freebsd = FreeBSD (pkg)
cmd-family-unknown = Detect automatically

## Managed alias/function files (shells/managed.rs)

managed-alias = alias
managed-function = function
managed-unsupported = this shell is not supported
managed-invalid-name = Only letters, digits and _ . : + - are allowed (and no - at the start).
managed-header =
    # Aliases and functions for Terminaal.
    #
    # Managed by Terminaal and only loaded in Terminaal. The blocks between
    # the markers may be edited by hand as well; anything outside of them
    # is lost the next time the sidebar saves this file.

## Sidebar: SSH hosts

ssh-store-unreadable = { $file } is unreadable – nothing is saved until “Reload”.
ssh-auto-key = Automatic (agent, ~/.ssh/id_*)
ssh-files-key = IdentityFile (see Advanced)
ssh-saved-hosts = Saved hosts
ssh-double-click-connects = Double-click connects
ssh-no-hosts = No saved hosts yet.
ssh-add-host = +  Add host
ssh-from-config = From ~/.ssh/config
ssh-connect = ▶  Connect
ssh-connect-as = Connect as { $login }
ssh-adopt = Adopt
ssh-adopt-hint = Copy into the saved hosts and edit
ssh-secrets-note = Passwords and passphrases are never saved; the tab asks for them. Host keys are checked against ~/.ssh/known_hosts.
ssh-via = { $address } · via { $jump }
ssh-more-logins =
    { $count ->
        [one] +1 login
       *[other] +{ $count } logins
    }
ssh-is-jump-host = “{ $name }” is the jump host of { $hosts } – change that first.

## Sidebar: host form

host-new = New host
host-edit = Edit host
host-host-hint = server.example.com or 192.168.1.10
host-name-optional = Name (optional)
host-name-hint = same as host
host-jump = Jump host (ProxyJump)
host-jump-none = None
host-no-host-open = No host open.
host-missing = The host is missing.
host-has-spaces = The host must not contain spaces.
host-bad-port = The port must be a number between 1 and 65535.
host-name-taken = There already is a host named “{ $name }”.
host-bad-user = A user name must contain neither spaces nor @.
host-login-twice = The login “{ $login }” is listed twice.
host-jumps-itself = A host can't jump via itself.
host-jump-and-proxy = Jump host and ProxyCommand exclude each other – clear one of them.
host-not-a-number = { $label }: “{ $value }” is not a whole number.
host-login = Login
host-login-more = Additional { $index }
host-remove-login = Remove login
host-make-default-login = Make default – double-click and jump host use this login
host-user-hint = User (default: { $user })
host-add-login = +  Another login
host-add-login-hint = Another user or key for the same host
host-keys-hint = Create your own keys in the “Keys” section.
host-forwards =
    { $count ->
        [0] Port forwarding
       *[other] Port forwarding ({ $count })
    }
host-forward-local = Local
host-forward-remote = Remote
host-forward-dynamic = SOCKS
host-forward-remote-dynamic = SOCKS on server
host-forward-local-hint = LocalForward: a port or socket here leads to a target the server can reach
host-forward-remote-hint = RemoteForward: a port on the server leads to a target this machine can reach
host-forward-dynamic-hint = DynamicForward: a SOCKS proxy here – programs say where to, and the connection continues from the server
host-forward-remote-dynamic-hint = RemoteForward without a target: a SOCKS proxy on the server – connections continue from this machine
host-remove-forward = Remove forward
host-local-listen-hint = Port here, e.g. 8080, or socket path
host-local-target-hint = Target as seen from the server, e.g. localhost:5432 or /run/app.sock
host-remote-listen-hint = Port on the server, e.g. 9000
host-remote-target-hint = Target as seen from here, e.g. localhost:3000 or ~/app.sock
host-dynamic-listen-hint = Port here for the proxy, e.g. 1080
host-remote-dynamic-listen-hint = Port on the server for the proxy, e.g. 1080
host-add-forward = +  Forward
host-forwards-note = Active while the tab is connected. The port may be preceded by an address, e.g. *:8080 for all network interfaces. Paths with a / are Unix sockets – though the server can only listen on ports.
host-forward-missing-port = Forward { $row } is missing the port or socket path.
host-forward-missing-target = Forward { $row } is missing the target.
host-forward-invalid = Forward { $row }: { $err }
host-advanced =
    { $count ->
        [0] Advanced
       *[other] Advanced ({ $count } set)
    }

## Sidebar: host form, advanced options

adv-connection = Connection
adv-connect-timeout = Connect timeout in s
adv-connect-timeout-name = Connect timeout
adv-alive-interval = Keepalive every … s (0 = off)
adv-alive-interval-name = Keepalive interval
adv-alive-count = Give up after … unanswered keepalives
adv-alive-count-name = Unanswered keepalives
adv-compression = Compression
adv-compression-hint = Compression – helps on slow links
adv-address-family = Address family
adv-proxy-command = Proxy command instead of TCP
adv-proxy-command-hint = e.g. nc -X connect -x proxy:3128 %h %p
adv-auth = Authentication
adv-identity-files = Key files, one per line
adv-identity-files-note = Used by logins with “Automatic” as their key.
adv-identities-only = Offer only configured keys
adv-identities-only-hint = IdentitiesOnly – no other keys from the agent
adv-agent-socket = Agent socket
adv-agent-socket-hint = $SSH_AUTH_SOCK · none = no agent
adv-forward-agent = Forward the agent to the server
adv-forward-agent-hint = ForwardAgent – programs on the server can sign with the agent's keys while connected. Only for servers you trust. Forwards the agent socket above, otherwise $SSH_AUTH_SOCK.
adv-forward-agent-socket = Forwards { $socket }
adv-methods = Methods in this order
adv-host-key = Host key
adv-unknown-host-keys = Unknown host keys
adv-changed-host-key-note = A changed host key always aborts the connection. 🔑 Host key on the selected host shows and removes the stored one.
adv-known-hosts-file = known_hosts file
adv-session = Session
adv-remote-command = Command instead of login shell
adv-remote-command-hint = e.g. tmux new -A -s main
adv-set-env = Environment variables, one NAME=value per line
adv-set-env-hint = LANG=en_US.UTF-8
adv-send-env = Pass on local variables
adv-env-note = The server only takes what its AcceptEnv list allows. TERM=… sets the terminal type.
adv-system = System (for the commands)
adv-system-hint = Decides which built-in commands the sidebar's shell section offers – package managers and service tools differ per system. Automatic: found out while connecting. Terminaal's own option, not an ssh_config keyword.
adv-look = Look
adv-color = Warning color
adv-color-hint = Marks this host's tabs with a colored line and its terminals with a frame – e.g. red for production servers. Terminaal's own option, not an ssh_config keyword.
adv-color-none = None
adv-color-red = Red
adv-color-orange = Orange
adv-color-yellow = Yellow
adv-color-green = Green
adv-color-blue = Blue
adv-color-purple = Purple
adv-color-custom = Custom
adv-theme = Theme
adv-theme-hint = The console colors of this host's terminals; the rest of the window keeps its theme. Terminaal's own option, not an ssh_config keyword.
adv-theme-window = Like the window
adv-look-note = Applies to terminals opened afterwards.
adv-algorithms = Algorithms
adv-kex = Key exchange
adv-host-key-types = Host key types
adv-ciphers = Encryption
adv-macs = Integrity
adv-algorithms-note = Empty = default. +list appends, -list removes, ^list moves to the front; * and ? are allowed.

## SSH options (ssh/options.rs)

opt-family-any = IPv4 and IPv6
opt-family-inet = IPv4 only
opt-family-inet6 = IPv6 only
opt-check-ask = Ask (ask)
opt-check-accept-new = Save new ones without asking (accept-new)
opt-check-yes = Known hosts only (yes)
opt-unknown-method = PreferredAuthentications: unknown method “{ $name }”.
opt-no-method = PreferredAuthentications names no method Terminaal supports (publickey, keyboard-interactive, password).
opt-no-algorithm = none of the algorithms “{ $spec }” is supported (possible: { $supported })
opt-list-empty = { $keyword }: the list is empty.
opt-bad-color = color: “{ $value }” is neither red, orange, yellow, green, blue, purple nor #rrggbb.
opt-forward-invalid = { $keyword } “{ $spec }”: { $reason }
opt-forward-remote-unix = with libssh2 the server can't listen on a Unix socket, only on a port
opt-forward-local-no-target = without a target, this only works as DynamicForward (SOCKS)
opt-forward-syntax = expected [address:]port or socket path, then target:port or socket path
opt-forward-dynamic-syntax = expected [address:]port
opt-forward-port = invalid port
opt-forward-port-zero = port 0 only works with RemoteForward
opt-forward-target-port = invalid target port
opt-forward-target-missing = the target is missing
opt-setenv-syntax = SetEnv “{ $entry }”: expected NAME=value.
opt-setenv-name = SetEnv “{ $entry }”: “{ $name }” is not a valid variable name.

## Resolving hosts (ssh/mod.rs)

catalog-jump-and-proxy = “{ $name }” has both ProxyJump and ProxyCommand – only one of them works.
catalog-jump-loop = The ProxyJump chain via “{ $name }” is too long or goes in circles.
catalog-key-gone = The key “{ $key }” of “{ $host }” no longer exists.
catalog-bad-jump = Invalid jump host “{ $spec }” – expected a host name from the list or [user@]host[:port].

## Key store (ssh/keys.rs)

keystore-public-invalid = The public key of “{ $name }” is invalid: { $err }
keystore-no-source = “{ $name }” has neither a file nor an agent key.
keystore-unreadable-file = { $path } is not a readable OpenSSH key: { $err }
keystore-generate-failed = Could not generate the key: { $err }
keystore-exists = { $path } already exists.
keystore-delete-failed = could not delete { $path }: { $err }
keystore-no-agent = No SSH agent found ($SSH_AUTH_SOCK is not set).
keystore-agent-unreachable = SSH agent not reachable: { $err }

## Sidebar: keys

keys-unreadable = unreadable
keys-badge-agent = Agent
keys-badge-file = File
keys-click-details = Click for details
keys-none = No keys yet.
keys-generate = +  Generate
keys-add-file = +  File
keys-add-file-hint = Add an existing key file
keys-from-agent = +  From agent
keys-from-agent-hint = Take over a key from the SSH agent, e.g. 1Password
keys-note = New keys: Ed25519 in OpenSSH format. Passphrases are never saved. “Remove” deletes the entry – and, if you confirm, the files of keys generated here; other key files never.
keys-unknown = unknown
keys-type = Type: { $algorithm }
keys-comment-line = Comment: { $comment }
keys-file-line = File: { $file }
keys-agent-only = Only in the SSH agent.
keys-used-by = Used by: { $hosts }
keys-delete-files-question = “{ $name }” is removed from Terminaal. Delete the key files too?
keys-delete-files = Delete files
keys-keep-files = Keep files
keys-copy-public = 📋  Copy public key
keys-rename = Rename
keys-copied = Copied the public key of “{ $name }” – add it to ~/.ssh/authorized_keys on the server.
keys-still-used = “{ $name }” is still used by { $hosts }.
keys-removed-with-files = Removed “{ $name }” and deleted its key files.
keys-removed-but = Removed “{ $name }”, but { $err }
keys-removed-file-kept = Removed “{ $name }” – the key file itself is kept.
keys-removed = Removed “{ $name }”.
keys-new-title = New key (Ed25519)
keys-name-hint = Work
keys-comment = Comment
keys-location = Location
keys-passphrase-optional = Passphrase (optional)
keys-passphrase-repeat = Repeat passphrase
keys-no-passphrase-note = Without a passphrase the key is stored unencrypted on disk.
keys-add-file-title = Add key file
keys-private-file = Private key file
keys-agent-title = Key from the SSH agent
keys-agent-empty = The agent has no keys.
keys-no-comment = (no comment)
keys-rename-title = Rename key
keys-new-name = New name
keys-rename-note = Hosts using the key are updated too.
keys-no-form = No form open.
keys-passphrase-mismatch = The passphrases don't match.
keys-location-missing = The location is missing.
keys-generated = Generated “{ $name }” ({ $path }). Add the public key to ~/.ssh/authorized_keys on the server.
keys-file-missing = The file is missing.
keys-added = Added “{ $name }”.
keys-choose-from-list = Please choose a key from the list.
keys-agent-duplicate = This key already exists as “{ $name }”.
keys-taken-over = Took over “{ $name }”.
keys-gone = The key no longer exists.
keys-renamed = Renamed “{ $old }” to “{ $new }”.
keys-name-taken = There already is a key named “{ $name }”.

## SSH connection, shown in the tab (ssh/connection.rs)

conn-ssh-error = SSH error: { $err }
conn-io-error = I/O error: { $err }
conn-retry-in = Trying again in { $secs } s – Enter: try now · Ctrl+D: close the tab
conn-retry-or-close = Enter: try again · Ctrl+D: close the tab
conn-needs-input = Logging in needs an answer – an automatic attempt can't give one.
conn-reconnecting = Reconnecting to { $target } …
conn-reconnecting-auto = Reconnecting to { $target } (automatically) …
conn-connecting = Connecting to { $target } …
conn-connecting-via = Connecting to { $target } via { $hops } …
conn-tunnel-failed = { $hop } could not open a tunnel to { $host }:{ $port }: { $err }
conn-handshake-failed = SSH handshake with { $hop } failed: { $err }
conn-proxy-says = ProxyCommand says: { $output }
conn-session-closed = Terminaal: session closed
conn-tunnel-closed = Terminaal: tunnel closed
conn-resolve-failed = Could not resolve { $host }: { $err }
conn-connect-failed = Connection to { $host }:{ $port } failed: { $err }
conn-no-address = No address found for { $host } ({ $family }).
conn-proxy-start-failed = Could not start ProxyCommand “{ $command }”: { $err }
conn-no-host-key = The server sent no host key.
conn-unknown-type = unknown
conn-host-key-changed =
    WARNING: The host key of { $entry } has changed!
    This may be an attack (man-in-the-middle) – or the server was set up anew.
    Stored in { $file }:
    { $stored }
    New from the server: { $kind } { $fingerprint }
    Connection aborted. If the change is expected, remove the old entry
    in the sidebar (SSH → select the host → Host key) or with
      ssh-keygen -R '{ $entry }'{ $file_option }
conn-host-key-refused = The host key of “{ $entry }” is not in { $file }, and StrictHostKeyChecking only allows known hosts.
    { $kind } fingerprint: { $fingerprint }
conn-host-key-question =
    The authenticity of “{ $entry }” can't be established.
    { $kind } fingerprint: { $fingerprint }
    Connect and save the host key in { $file }? (yes/no):
conn-host-key-rejected = Aborted: host key not confirmed.
conn-known-hosts-failed = Could not add to { $file }: { $err }
conn-host-key-saved = Saved the host key of “{ $entry }” ({ $kind }, { $fingerprint }) in { $file }.
known-hosts-changed = The file has changed in the meantime – please reload.
conn-stored-key = { $kind } { $fingerprint } (line { $line })
known-hosts-button = 🔑 Host key
known-hosts-button-hint = Show or remove the host key stored in known_hosts
known-hosts-title = Host key of { $entry }
known-hosts-none = No entry stored – the next connection asks about the host key as StrictHostKeyChecking says.
known-hosts-line = Line { $line }
known-hosts-hashed = name hashed
known-hosts-also = applies to { $hosts }
known-hosts-remove =
    { $count ->
        [one] Remove entry
       *[other] Remove { $count } entries
    }
known-hosts-confirm = Only remove it if you know the server has a new key – otherwise this may be an attack. The whole line goes, for the other names in it too. The next connection shows the new host key for you to confirm.
known-hosts-removed = Removed the host key of { $entry } from { $file }.
known-hosts-failed = Could not change { $file }: { $err }
known-hosts-no-file = No known_hosts path ($HOME is not set).
conn-denied = Permission denied.
conn-password-prompt = Password for { $target }:
conn-auth-failed = Login as { $user } at { $host } failed (offered by the server: { $methods }).
conn-auth-failed-preferred = Login as { $user } at { $host } failed (offered by the server: { $methods }; PreferredAuthentications: { $preferred }).
conn-max-auth-tries = The SSH agent tried { $count } keys – many servers give up after a few attempts (MaxAuthTries). Fix: assign the host a fixed key in the “SSH” section.
conn-key-file-missing = Key file { $path } not found.
conn-wrong-passphrase = Wrong passphrase.
conn-passphrase-prompt = Passphrase for { $path }:
conn-keepalive-dead = Connection lost: { $target } stopped responding ({ $count } keepalives unanswered).
conn-lost = Connection lost: { $err }
conn-agent-forward-up = Forwarding the agent at { $socket }
conn-agent-forward-failed = Can't forward the agent: { $err }
conn-agent-forward-refused = the server doesn't allow it (AllowAgentForwarding)
conn-agent-forward-unset = { $var } isn't set
conn-agent-forward-off = IdentityAgent is none
conn-agent-forward-missing = no agent at { $socket }
conn-algorithms-failed = { $keyword } for { $hop }: { $err }
conn-unknown-key-type = unknown key type

## Port forwarding (ssh/forward.rs)

forward-up = Forwarding { $forward }
forward-failed = Forwarding { $forward } failed: { $err }
forward-no-address = no address found

## Port forwards of the active tab (ui/ssh_panel.rs)

fwd-title = Forwards · { $tab }
fwd-active = active
fwd-starting = setting up …
fwd-paused = paused
fwd-failed = failed: { $err }
fwd-pause = Pause
fwd-start = Start
fwd-retry = Try again
fwd-remote-paused-note = A paused remote forward keeps listening on the server but turns connections away.

## Files of an SSH connection (sftp/, ui/files_panel.rs)

files-tab = Files: { $host }
files-tab-attention = ⚠ Files: { $host }
files-title = Files on { $host }
files-connect-failed = SFTP not available: { $err }. Has the login in the terminal finished?
files-reconnect = Reconnect
files-waiting-login = Waiting for the login in the terminal …
files-waiting-reconnect = Connection interrupted – this carries on once the terminal is connected again.
files-terminal-closed = The terminal of this connection is closed. Open the host again – the files tab takes over the new connection.
files-offline = Not connected right now.
files-reconnected = The terminal reconnected.
files-stream-closed = SFTP channel closed.
files-transfer-waiting = waiting for the connection
files-local = This computer
files-remote = Server { $host }
files-up = Parent folder
files-home = Home folder
files-reload = Reload
files-upload = Upload
files-download = Download
files-edit = Edit
files-edit-hint = Open locally in an editor; every save goes back to the server
files-rename = Rename
files-rename-to = Rename “{ $name }” to:
files-new-folder = New folder
files-new-folder-name = Name of the new folder:
files-delete = Delete
files-delete-confirm = Really delete?
files-delete-hint = Files and empty folders; a second click deletes
files-ok = OK
files-cancel = Cancel
files-created = Created “{ $name }”.
files-renamed = Renamed to “{ $name }”.
files-removed = Deleted “{ $name }”.
files-list-failed = Can't open { $dir }: { $err }
files-local-failed = { $path }: { $err }
files-transfers = Transfers
files-clear-transfers = Remove finished
files-cancel-transfer = Cancel
files-transfer-done = done
files-transfer-cancelled = cancelled
files-edits = Edited locally
files-edits-note = Local copies live in a private folder and are deleted when closed. Before uploading, Terminaal checks whether the file on the server changed meanwhile.
files-edit-local = Local copy: { $path }
files-edit-synced = on the server
files-edit-uploading = uploading …
files-edit-conflict-state = changed on the server meanwhile – nothing overwritten
files-edit-denied-read = no permission to read
files-edit-denied-write = no permission to write – change not on the server yet
files-edit-overwrite = Overwrite
files-edit-overwrite-hint = Replace the changed file on the server with the local version
files-edit-take-theirs = Take the server's version
files-edit-take-theirs-hint = Discard the local changes and load the file from the server again
files-edit-reopen = Open in editor
files-edit-close = Close
files-edit-close-unsaved = Discard changes?
files-edit-close-hint = Delete the local copy
files-edit-saved = Saved “{ $name }” on the server.
files-edit-conflict = “{ $name }” changed on the server while you edited it. Nothing was overwritten.
files-edit-failed = Can't edit “{ $name }”: { $err }
files-edit-not-a-file = “{ $name }” isn't a regular file.
files-edit-too-big = “{ $name }” is too big to edit (more than { $limit } MB).
files-edit-upload-failed = Upload failed: { $err }
files-sudo-use = With sudo …
files-sudo-use-hint = Puts a small script on the server that you run in the terminal – sudo asks for the password there as usual
files-sudo-ready = run with sudo in the terminal:
files-sudo-run = Run in terminal
files-sudo-run-hint = Pastes the command into this connection's terminal and runs it
files-sudo-no-terminal = The terminal of this connection is closed
files-sudo-running = waiting for sudo in the terminal …
files-sudo-failed = sudo failed (exit code { $code }).
files-sudo-prepare-failed = Can't create the sudo script: { $err }
files-sudo-path = The home folder { $path } has characters that can't safely go into a command.
files-sudo-reading = Terminaal: reading { $path } with sudo
files-sudo-writing = Terminaal: writing { $path } with sudo
