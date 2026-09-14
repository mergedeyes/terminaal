# Terminaal documentation

Terminaal is a terminal emulator with a built-in SSH manager. These pages go
further than the [README](../README.md): step-by-step tutorials, guides for
single tasks, background on how things work, and a collection of tips.

## Start here

- [Getting started](getting-started.md) – install, first launch, and a tour of
  tabs, sidebar and settings

## Tutorials

Walk through a complete task from start to finish.

1. [Your first SSH host](tutorials/01-your-first-ssh-host.md) – save a server,
   log in with a key, reach it through a jump host
2. [Keys and the SSH agent](tutorials/02-keys-and-the-agent.md) – generate a key,
   put it on a server, use keys from 1Password or `ssh-agent`
3. [Port forwarding](tutorials/03-port-forwarding.md) – reach a database behind a
   server, share a local dev server, a SOCKS proxy, and watching forwards live
4. [One command on many servers](tutorials/04-one-command-many-servers.md) –
   snippets and broadcast together
5. [Make your own theme](tutorials/05-make-your-own-theme.md) – from an Alacritty
   theme to colors for the whole interface

## Guides

Look something up, or get one thing done.

- [Configuration reference](guides/configuration.md) – every file and every key
- [Keyboard shortcuts](guides/keyboard-shortcuts.md) – defaults, changing them,
  what's allowed
- [SSH hosts and `~/.ssh/config`](guides/ssh-hosts.md) – what's read, per-host
  options, logins, jump hosts
- [Host keys and `known_hosts`](guides/host-keys.md) – new hosts, changed keys
- [Shell integration](guides/shell-integration.md) – what it gives you, other
  shells, remote servers
- [Built-in commands and snippets](guides/commands-and-snippets.md)
- [Search, links and the scrollback](guides/search-and-links.md)
- [Split panes](guides/split-panes.md) – several terminals in one tab
- [Restoring the last session](guides/sessions.md) – what comes back at start,
  when it's saved, several windows
- [The drop-down window](guides/drop-down-window.md) – a terminal that drops
  down from the top of the screen at a key press
- [Files on a server (SFTP)](guides/files-and-sftp.md) – browse, copy, edit
  server files locally, sudo for root's files
- [Appearance](guides/appearance.md) – themes, COSMIC, translucency, fonts

## Explanations

How Terminaal works, and why it works that way.

- [Architecture](explanations/architecture.md) – renderer, threads, when it
  redraws
- [Security and your files](explanations/security-and-your-files.md) – what's
  stored, what's never touched, what a click may open
- [How prompt marks work](explanations/prompt-marks.md) – shell integration
  under the hood

## More

- [Tips and tricks](tips.md)
- [Troubleshooting](troubleshooting.md)
