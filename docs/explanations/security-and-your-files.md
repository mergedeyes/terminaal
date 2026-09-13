# Security and your files

A terminal with an SSH manager handles sensitive things: credentials, other
people's servers, your dotfiles. This page describes what Terminaal does – and
deliberately doesn't – to keep them safe.

## Nothing secret is stored

- **Passwords and passphrases** are asked in the tab when needed and kept only
  as long as the login takes. They never reach a file, a log or the config.
- **`hosts.toml`** holds addresses, user names, key *names* and options.
- **`keys.toml`** holds key file *paths*, or the *public* half of agent keys.
- **Generating a key** is done in-process with the RustCrypto `ssh-key` library,
  not by calling `ssh-keygen` – a passphrase on a command line would be visible
  to other processes.
- Keys without a passphrase are stored unencrypted on disk, like OpenSSH does;
  the key form says so.

## Your files are changed precisely, or not at all

| File | What Terminaal may do |
| --- | --- |
| `~/.ssh/known_hosts` | Append a line for a new host. Remove the lines of one host – only when you ask, after confirming, leaving every other byte as it is |
| `~/.ssh/config` | Only read. Adopting a host copies it into `hosts.toml` |
| `config.toml` | Change the one key you changed in the settings, keeping comments |
| Key files | Create new ones (never overwriting an existing file, mode `0600`). Delete them only for keys generated in Terminaal, only after a second confirmation, and only if no other entry uses the file |
| Your shell configs (`.bashrc`, `.zshrc`, fish config) | Never touched. Terminaal's aliases and integration live in separate files that only Terminaal loads |
| `hosts.toml`, `keys.toml`, `snippets.toml` | Rewritten whole, atomically. If one can't be read, Terminaal refuses to save over it |

## Host keys

- A **changed** host key always aborts the connection, no matter what
  `StrictHostKeyChecking` says (`no` behaves like `accept-new`).
- The warning shows the stored and the new fingerprint side by side.
- Removing the old key and confirming the new one are two separate, deliberate
  steps.

## `~/.ssh/config`

`Match exec` is never evaluated. Otherwise just opening Terminaal – which reads
the file to list your hosts – would run commands from it.

## What a click can do

- **Links:** <kbd>Ctrl</kbd>+click opens URLs only for a fixed set of schemes
  (`https`, `http`, `mailto`, `file`, `ftp`, `git`, `gemini`, `gopher`, `news`,
  `magnet`, `ipfs`, `ipns`). A program printing an OSC 8 hyperlink can't make a
  click launch an arbitrary URL handler.
- **Executable files** are never opened directly – their folder opens instead.
- **File links in SSH tabs** are never resolved: the names refer to the server,
  and opening a local file of the same name would be misleading.
- **Command buttons** show their exact line on hover; the first run explains that
  commands go straight to the shell. **Update the system** is the only built-in
  command that changes anything, and confirmation prompts stay on unless you turn
  that off.

## Pasting

Escape characters and Ctrl+C are removed from pasted text. With bracketed paste
active, this also prevents a crafted clipboard from ending the paste early and
running the rest as typed commands.

## Broadcast

Input goes to other tabs only while the **active** tab is in the broadcast group,
and tabs in the group are marked red. Replies to terminal queries and mouse
events are never broadcast.

## Agent forwarding

Off by default, as in OpenSSH. When on, anyone with root on that server can use
your agent while you're connected – enable it only for servers you trust.

## Tests

Terminaal's tests never touch your real `~/.ssh` or config: UI tests run with
saving disabled, and end-to-end SSH tests use a throwaway `sshd`, agent and home
directory.
