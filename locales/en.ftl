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

cli-usage = Usage: terminaal [--connect [USER@]HOST]
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
menu-shortcut-copy = Ctrl+Shift+C
menu-shortcut-paste = Ctrl+Shift+V

## Sidebar

sidebar-keys = Keys
sidebar-settings = Settings

## Sidebar: settings

settings-language = Language
settings-language-auto = Automatic · { $language }
settings-language-auto-hint = Follows the system locale (LANG): German for a German locale, English otherwise
settings-language-note = Applies right away. Saved as “language” in ~/.config/terminaal/config.toml.
settings-scroll = Scroll speed
settings-scroll-lines = { $lines ->
    [one] { $lines } line per mouse-wheel notch
   *[other] { $lines } lines per mouse-wheel notch
}
settings-scroll-note = Default 3. Touchpads scroll smoothly. Saved as “scroll_lines” in ~/.config/terminaal/config.toml.

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
host-forward-local-hint = LocalForward: a port here leads to a target the server can reach
host-forward-remote-hint = RemoteForward: a port on the server leads to a target this machine can reach
host-remove-forward = Remove forward
host-local-listen-hint = Port here, e.g. 8080
host-local-target-hint = Target as seen from the server, e.g. localhost:5432
host-remote-listen-hint = Port on the server, e.g. 9000
host-remote-target-hint = Target as seen from here, e.g. localhost:3000
host-add-forward = +  Forward
host-forwards-note = Active while the tab is connected. The port may be preceded by an address, e.g. *:8080 for all network interfaces.
host-forward-missing-port = Forward { $row } is missing the port.
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
adv-methods = Methods in this order
adv-host-key = Host key
adv-unknown-host-keys = Unknown host keys
adv-changed-host-key-note = A changed host key always aborts the connection.
adv-known-hosts-file = known_hosts file
adv-session = Session
adv-remote-command = Command instead of login shell
adv-remote-command-hint = e.g. tmux new -A -s main
adv-set-env = Environment variables, one NAME=value per line
adv-set-env-hint = LANG=en_US.UTF-8
adv-send-env = Pass on local variables
adv-env-note = The server only takes what its AcceptEnv list allows. TERM=… sets the terminal type.
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
opt-forward-invalid = { $keyword } “{ $spec }”: { $reason }
opt-forward-unix = Unix sockets are not supported
opt-forward-socks = dynamic forwarding (SOCKS) is not supported
opt-forward-syntax = expected [address:]port target:port
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
conn-any-key-closes = Press any key to close the tab.
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
    New { $kind } fingerprint: { $fingerprint }
    Connection aborted. If the change is expected, remove the old entry with
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
conn-algorithms-failed = { $keyword } for { $hop }: { $err }
conn-unknown-key-type = unknown key type

## Port forwarding (ssh/forward.rs)

forward-up = Forwarding { $forward }
forward-failed = Forwarding { $forward } failed: { $err }
forward-no-address = no address found
