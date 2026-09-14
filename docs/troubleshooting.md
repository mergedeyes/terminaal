# Troubleshooting

## Getting more information

Start Terminaal from another terminal with logging:

```sh
RUST_LOG=terminaal=info terminaal                   # connection failures, warnings
RUST_LOG=terminaal=debug terminaal                  # plus per-frame timing
RUST_LOG=terminaal=trace terminaal --connect web1   # plus the SSH worker's timeline
```

## Installation and startup

**The launcher doesn't start Terminaal, or starts an old version.**
Run `./install.sh` again after updating the source. The desktop entry points at
`~/.cargo/bin/terminaal` with its full path.

**The dock shows a generic icon, or can't restore the window after minimizing.**
The desktop entry is missing. Run `./install.sh` (or `./install.sh --no-binary` if
you installed the binary another way).

**Build fails with errors about OpenSSL or libssh2.**
Install the C toolchain, `pkg-config`, and the OpenSSL and zlib development
packages (see [Getting started](getting-started.md#install)).

**A warning about an sRGB framebuffer at start.**
Harmless – colors are correct.

## Display

**The window isn't see-through.**
Translucency needs Wayland; on X11 the window stays opaque. Also check the
settings: the page says when the driver or compositor doesn't offer it.

**No blur behind the window.**
Blur needs a compositor with `ext-background-effect` (COSMIC) or KDE.

**Text looks misaligned with the cursor.**
Choose a real monospace font under Settings → Appearance → Console.

**The font I set isn't used.**
The list marks fonts that aren't installed. Check the family name with
`fc-list : family`.

**Terminaal uses CPU while idle.**
It should draw about two frames per second for the blinking cursor. Check with
`RUST_LOG=terminaal=debug`: dozens of frames per second while idle is a bug worth
reporting.

## Keyboard

**A shortcut does nothing.**
- A text field in the sidebar or settings may have the keyboard – click into the
  terminal.
- In full-screen programs, scrolling and prompt-jump keys go to the program.
- Your desktop may grab the combination first.
- Another action may use it: Settings → Shortcuts strikes shadowed combinations
  through.

**The settings refuse my shortcut.**
It's either taken, or it would swallow typing (no Ctrl/Alt/Super, and not an
F-key or Shift+navigation key). You can still set it in `config.toml`.

## Shell integration

**No prompt jumps, exit codes or directory titles.**
- The integration is set up for fish, bash and zsh started by Terminaal – not for
  a shell started from inside another shell.
- Over SSH, the remote shell has to send the sequences; see
  [Shell integration](guides/shell-integration.md#other-shells-or-shells-over-ssh).

**The tab title shows `user@host: dir` instead of just the directory.**
A program (often your distribution's bash setup) sets the title; that wins.

**No notification when a command finishes.**
- It only fires when the command ran at least `notify_after_secs` and the tab
  wasn't visible (window unfocused, or another tab active).
- `notify-send` must be installed (`libnotify`).
- The shell must send OSC 133 `C` and `D`.

**Ctrl+click on a file name does nothing.**
- The file must exist; relative names need shell integration (the working
  directory) and a local tab.
- `xdg-open` must be installed and have a default application for the type.

## SSH

**"Too many authentication failures".**
Your agent offered too many keys. Pick the host's key in its login, or enable
**Offer only configured keys**. The error message mentions this when the agent
tried three or more keys.

**The host key changed – I reinstalled the server.**
See [Host keys](guides/host-keys.md#removing-the-old-key).

**The tab hangs for a moment right after login.**
Terminaal asks the host which system it runs, for the command buttons (up to two
seconds). Set **Advanced → Session → System** to skip it.

**The connection drops after some idle time.**
Keepalives are on by default (30 s). If a firewall is stricter, lower
**ServerAliveInterval**.

**A dead connection takes a while to be noticed.**
With the defaults, after three unanswered keepalives 30 seconds apart – roughly a
minute and a half. Lower the interval or count for faster detection. Once noticed,
the tab reconnects by itself (see below), and a files tab carries on.

**The tab says "Connection lost" and counts down.**
Terminaal tries again after 2, 4, 8 … seconds (at most a minute), keeping the
scrollback. Press <kbd>Enter</kbd> to try right away, <kbd>Ctrl</kbd>+<kbd>D</kbd>
to close the tab. Programs that were running on the server are gone – use `tmux`
(see [Tips](tips.md)) to keep them.

**It says "Logging in needs an answer" and stops trying.**
Automatic attempts can't type a passphrase, a password or answer a host-key
question. Press <kbd>Enter</kbd> and answer in the tab – or use the SSH agent or a
key without passphrase for that host.

**`unknown terminal type` or broken colors on a server.**
Set `TERM=xterm-256color` (the default) or `TERM=xterm` under **Advanced →
Session → Environment**.

**`SetEnv`/`SendEnv` variables don't arrive.**
The server only accepts variables listed in its `AcceptEnv`.

**A port forward fails with "Address already in use".**
Something else listens on the port: `ss -ltnp | grep PORT`. Free it, then **Try
again** in the SSH section.

**A remote forward fails.**
The server may forbid it (`AllowTcpForwarding`, `GatewayPorts` for non-local
binds). The reason is shown in the SSH section and in the tab.

**Connecting through a remote forward's target freezes the tab for a moment.**
Connecting to the local target blocks the connection's worker for up to three
seconds if the target doesn't answer.

**A remote shell with `RemoteCommand` closes the tab immediately.**
The tab closes when the command ends, like a local shell. Use something that stays,
e.g. `tmux new -A -s main`.

**Agent forwarding doesn't work.**
The server must allow it (`AllowAgentForwarding`), and the local agent socket must
exist – the tab prints a line when either fails.

## Files

**My hosts/keys/snippets disappeared, or saving says the file is unreadable.**
The file has a syntax error. Terminaal shows the error and refuses to save over the
file, so nothing is lost. Fix it in an editor, then click **Reload** in the SSH or
Keys section – for `snippets.toml`, restart Terminaal.

**Changes I made to `hosts.toml` by hand were lost.**
Edit it while Terminaal is closed – saving in the sidebar writes the whole file
from what Terminaal loaded at startup.
