# Tips and tricks

## Tabs and windows

- **New tab where you are:** <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd> opens in
  the active tab's directory (local tabs, with shell integration).
- **Reorder tabs** with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>PageUp</kbd>/<kbd>PageDown</kbd>,
  close one with a middle click.
- **Maximum screen space:** hide the sidebar (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd>),
  and turn off **Show sidebar** under Settings → General for future starts.
- **Presenting or pairing?** <kbd>Ctrl</kbd>+<kbd>+</kbd> zooms temporarily;
  <kbd>Ctrl</kbd>+<kbd>0</kbd> goes back. Nothing is saved.
- **Launcher shortcuts:** make desktop entries or scripts with
  `terminaal --connect web1` for servers you open every day.

## Scrollback and output

- **Find where a command started:** <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd>
  jumps prompt by prompt – far faster than scrolling through long build output.
- **Spot failures at a glance:** failed commands have `✘ code` at the right of
  their prompt line.
- **Search for errors:** <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd>, type
  `error`, <kbd>Enter</kbd>, then <kbd>n</kbd> through them. Typing `Error` with a
  capital only finds that exact case.
- **Scroll `less` output in Terminaal's scrollback:** hold <kbd>Shift</kbd> while
  using the wheel.
- **Long-running jobs:** start the build, switch to another tab or app – you get
  a notification when it's done (after 10 s by default, adjustable).

## Clicking

- **Compiler errors:** <kbd>Ctrl</kbd>+click on `src/main.rs:42:7` opens the file.
- **`ls` output:** <kbd>Ctrl</kbd>+click a file name opens it; a folder opens in
  your file manager.
- **Real hyperlinks:** `ls --hyperlink=auto` makes every name a link, even from
  other directories.

## Pasting

- **Paste and run** (right-click menu) removes trailing newlines and presses Enter
  exactly once – handy for commands copied from docs. Bind it to a key under
  Settings → Shortcuts, e.g. <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd>.
- **Multi-line pastes** are inserted, not executed line by line, in bash, zsh,
  fish and editors – review, then press Enter.

## SSH

- **One key per host** avoids "Too many authentication failures" when your agent
  holds many keys.
- **Several roles, one host:** add logins (`deploy`, `root`) instead of duplicate
  hosts. Each gets its own ▶ button.
- **Reconnect into tmux:** set **Advanced → Session → Command instead of login
  shell** to `tmux new -A -s main`. Dropped connections no longer lose your work.
- **Skip the system check:** set **Advanced → Session → System** for hosts you
  connect to often – the tab starts up to two seconds faster on slow links.
- **Old servers:** `kex_algorithms = "+diffie-hellman-group14-sha1"` or
  `host_key_algorithms = "+ssh-rsa"` in Advanced → Algorithms, instead of
  loosening your global config.
- **A port forward failed** because the port was taken? Free it and click **Try
  again** in the SSH section – no reconnect.
- **Terminal type problems** on a server (`unknown terminal type`)? Add
  `TERM=xterm-256color` under **Advanced → Session → Environment**.

## Commands and broadcast

- **Check a fleet:** open all servers, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd>
  in each, then run `uptime`, `df -h` or a snippet once.
- **Review before running:** turn off **Run commands right away**; buttons then
  only type the command.
- **Host-specific runbooks:** snippets bound to a host appear only in that host's
  tabs – restart scripts, deploy steps, log locations.
- **Aliases stay out of your dotfiles:** manage them in the Shells section; other
  terminals don't see them.

## Looks

- **Any Alacritty theme** works – drop the `.toml` into
  `~/.config/terminaal/themes/` and click **Reload**.
- **Readable translucency:** opacity around 0.85–0.92 with blur keeps text crisp
  while showing the desktop.
- **On COSMIC**, pick the **COSMIC** theme and change your desktop's accent color –
  Terminaal follows immediately.

## Config

- **Hover** any setting's name to see its key in `config.toml`.
- **Comment freely** in `config.toml`; the settings tab keeps your comments.
- **Share your setup:** `config.toml`, `snippets.toml` and `themes/` hold nothing
  secret and are fine to keep in a dotfiles repository. `hosts.toml` holds no
  secrets either, but reveals your servers' addresses.
