# SSH hosts and `~/.ssh/config`

## Two kinds of hosts

The SSH section lists:

- **Saved hosts** – yours to edit, stored in `~/.config/terminaal/hosts.toml`.
- **From ~/.ssh/config** – read-only. Every concrete `Host` entry (no wildcards)
  shows up with all its options. **Adopt** copies one into your saved hosts.

A saved host with the same name as a config host wins, for jump hosts too.

## The host form

The form shows what most hosts need, and folds the rest away:

- **Host**, **Name**, **Port**
- **Login** – user and key; **+ Another login** adds more
- **Jump host (ProxyJump)** – any saved or config host
- **Port forwarding** – see [the tutorial](../tutorials/03-port-forwarding.md)
- **Advanced** – everything else, grouped below

Fold-outs open by themselves when something in them is set. Each field names its
`ssh_config` keyword, so you can compare with your existing config.

## Logins and keys

A host has a default login (`user`/`key`) plus any number of further ones. For
each login the key is either a named key from the Keys section, or
**Automatic**:

1. Keys from the SSH agent
2. The host's `IdentityFile`s
3. Unless restricted: `~/.ssh/id_ed25519`, `id_ecdsa`, `id_rsa`
4. keyboard-interactive, then password

A named key – or **Offer only configured keys** (`IdentitiesOnly`) – is offered
exclusively, also when it comes from the agent. That avoids *Too many
authentication failures* on servers with a low `MaxAuthTries`; if a login fails
after the agent tried three or more keys, the error message points you there.

The order follows **PreferredAuthentications** when set.

## Advanced options

### Connection

| Option | Keyword | Notes |
| --- | --- | --- |
| Connect timeout | `ConnectTimeout` | seconds |
| Keepalive interval | `ServerAliveInterval` | default **30** (OpenSSH: off); `0` turns it off |
| Keepalives before giving up | `ServerAliveCountMax` | default 3 |
| Compression | `Compression` | |
| Address family | `AddressFamily` | any, IPv4, IPv6 |
| Proxy command | `ProxyCommand` | run with `sh -c`; `%h %p %r %n` are replaced; its error output shows in the error message |

### Authentication

| Option | Keyword |
| --- | --- |
| Key files | `IdentityFile` (several) |
| Offer only configured keys | `IdentitiesOnly` |
| Agent socket | `IdentityAgent` – a path, `SSH_AUTH_SOCK` or `none` |
| Forward the agent | `ForwardAgent` – yes, a socket path, or `$VARIABLE` |
| Methods | `PreferredAuthentications` |

### Host key

| Option | Keyword |
| --- | --- |
| Unknown host keys | `StrictHostKeyChecking` – ask, accept-new, yes (`no` acts like accept-new) |
| known_hosts file | `UserKnownHostsFile` |

A *changed* host key always aborts the connection. See [Host keys](host-keys.md).

### Session

| Option | Keyword | Notes |
| --- | --- | --- |
| Command instead of login shell | `RemoteCommand` | e.g. `tmux new -A -s main`; the tab closes when it ends |
| Environment | `SetEnv` | `NAME=value` per line; `TERM=…` sets the terminal type (default `xterm-256color`) |
| Pass on variables | `SendEnv` | names, `*` and `?` allowed; the server must accept them (`AcceptEnv`) |
| System (for the commands) | – | Terminaal's own; skips detecting the system after login |

### Algorithms

`KexAlgorithms`, `HostKeyAlgorithms`, `Ciphers`, `MACs` – empty means libssh2's
defaults. As in OpenSSH, `+list` appends, `-list` removes, `^list` moves to the
front, and `*`/`?` match. Names libssh2 doesn't support are dropped.

## Jump hosts

- Pick a host in **Jump host**, or type `user@host:port` in `hosts.toml`
  (`proxy_jump`). Several, comma-separated, form a chain.
- Each hop is a session of its own with its own host key check and login.
- Jump hosts get no port forwards, agent forwarding, remote command or
  environment – those belong to the final host.
- `ProxyJump` and `ProxyCommand` can't be combined.

## What's read from `~/.ssh/config`

- `Host` blocks with concrete names, and `Match` with `all`, `host`,
  `originalhost`, `user`, `localuser` and `final`. `Match exec` is never run, on
  purpose – it would execute commands just by opening Terminaal.
- `Include`, with wildcards; relative paths are relative to `~/.ssh`.
- `HostName`, `User`, `Port`, `IdentityFile`, `IdentitiesOnly`, `IdentityAgent`,
  `ProxyJump`, and every option in the tables above, plus `LocalForward`,
  `RemoteForward`, `DynamicForward`.
- `%` tokens and `${VAR}`, quoted values.
- First value wins, as in OpenSSH; `IdentityFile`, forwards, `SetEnv` and
  `SendEnv` accumulate.
- Unknown options are ignored.

## Connecting from the command line

```sh
terminaal --connect web1          # default login
terminaal --connect root@web1     # web1's login for root
```

## Not supported

- `ControlMaster` (connection sharing), `ExitOnForwardFailure` (a failed forward
  is reported, the connection stays)
- The server listening on a Unix socket (`streamlocal-forward`), and
  `StreamLocalBindUnlink`/`StreamLocalBindMask` – stale local sockets are
  replaced, new ones are always `0600`
- `HostKeyAlias`, `CheckHostIP`
- Generating keys other than Ed25519 (existing RSA and ECDSA keys work)
