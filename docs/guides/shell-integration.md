# Shell integration

When the shell tells Terminaal where prompts start, when commands run and which
directory it's in, you get:

| Feature | Needs |
| --- | --- |
| Tab title shows the working directory | OSC 7 |
| New tab opens in the same directory | OSC 7 (local tabs) |
| Ctrl+click opens relative file names | OSC 7 (local tabs) |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> jump between prompts | OSC 133 `A` |
| `✘ code` next to a failed command, and how long a command ran | OSC 133 `A`, `C`, `D` |
| Copy a command's output, select a command with a click on its prompt | OSC 133 `A`, `C`, `D` |
| Notification when a long command finishes unseen | OSC 133 `C`, `D` |

## fish, bash and zsh: nothing to do

Terminaal starts these shells with small generated startup files in
`~/.config/terminaal/shell-integration/`. They load your own config first
(`~/.bashrc`, your `.zshrc`, fish's config), then Terminaal's managed aliases,
then the integration. Your dotfiles aren't touched, and other terminals don't
get any of it.

- **bash** reports through `PROMPT_COMMAND` (put first, so it sees the command's
  exit status; array-style `PROMPT_COMMAND` works) and `PS0`.
- **zsh** uses `precmd` and `preexec` hooks.
- **fish 4 and newer** report everything themselves; Terminaal only adds hooks
  for older fish versions.

Tools that rewrite your prompt (Starship, oh-my-zsh, powerlevel10k) keep
working: the integration doesn't touch `PS1`.

## Other shells, or shells over SSH

Terminaal reads standard sequences, so any shell that sends them works – also on
a server you've connected to. Add this to the **server's** `~/.bashrc`:

```bash
if [[ $- == *i* ]]; then
    __term_prompt() {
        local ret=$?
        printf '\e]133;D;%s\a\e]7;file://%s%s\a\e]133;A\a' "$ret" "$HOSTNAME" "${PWD// /%20}"
        return $ret
    }
    PROMPT_COMMAND="__term_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
    PS0="${PS0}\e]133;C\a"
fi
```

For zsh (`~/.zshrc`):

```zsh
__term_precmd() { local ret=$?; printf '\e]133;D;%s\a\e]7;file://%s%s\a\e]133;A\a' $ret $HOST ${PWD// /%20} }
__term_preexec() { printf '\e]133;C\a' }
precmd_functions=(__term_precmd $precmd_functions)
preexec_functions+=(__term_preexec)
```

Other terminals ignore these sequences, so it's safe to leave them in.

What each sequence means:

| Sequence | Sent | Meaning |
| --- | --- | --- |
| `ESC ] 7 ; file://host/path BEL` | before each prompt | Working directory, percent-encoded |
| `ESC ] 133 ; A BEL` | right before the prompt | A prompt starts here |
| `ESC ] 133 ; B BEL` | after the prompt (optional) | The prompt ends, input starts |
| `ESC ] 133 ; C BEL` | when a command starts | Output of a command follows |
| `ESC ] 133 ; D ; code BEL` | before the next prompt | The command finished with `code` |

`ST` (`ESC \`) works as terminator instead of `BEL`.

## Using the features

- **Prompt jumps** put the prompt at the top of the screen. Going down past the
  last prompt returns to the bottom.
- **Exit codes** appear at the right end of the prompt line of the failed command,
  only where the line has room. The code is known once the *next* prompt appears.
- **Run times** of commands that took a second or longer appear there too, in a
  weaker color and left of an exit code: `4,2 s`, `3 min 5 s`. Several lines
  pasted at once count as one command.
- **Copy the output**: <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Alt</kbd>+<kbd>C</kbd>
  (`copy_last_output`) copies what the last command printed – without its
  prompt and command line, without trailing blank lines. Empty Enters in between
  don't count; a command that printed nothing copies nothing. Right-click →
  **Copy output** takes the command under the mouse instead (the last one when
  there's none). Neither works while a full-screen program has the screen.
- **Select a command**: click on a prompt (on its prompt text, not what you
  typed) to select the whole block – prompt, command and output – then copy it
  as usual. Dragging from a prompt still selects normally. The prompt you're
  typing at has no block yet.
- **Notifications**: **Settings → Terminal → Shell integration → Notify after**
  (default 10 seconds, 0 turns it off). You're notified only when the window
  isn't focused or the command's tab isn't the active one. Needs `notify-send`;
  the taskbar entry is also marked as needing attention.
- **Tab titles**: while a program runs in a local tab, the tab is named after
  it (`btop`, `less`) – or after the title the program sets for itself if it
  sets one (`notes.md (~) - VIM`). At the prompt, a title set by the shell
  (OSC 0/2) wins over the directory; many distributions' bash setups set one,
  so you may see `user@host: dir` instead. Over SSH the program runs on the
  server, so only what it sends as a title shows up.
- **New tabs** open in the active tab's directory only when it's a local tab and
  the directory is on this machine.

## Troubleshooting

- **No prompt marks in a local tab?** Check you're running fish, bash or zsh as
  started by Terminaal – a shell started inside another (say, `bash` typed in
  fish) doesn't get the integration.
- **Copy output copies nothing?** The output starts where the shell said the
  command started (`C`); a command whose very first output is a hyperlink of its
  own gets no start mark. Output printed before this version has no mark either.
- **Marks, but no exit codes?** The shell sends `A` but not `C`/`D`. A `D`
  without a preceding `C` is ignored on purpose (it would repeat the last status
  on an empty Enter).
- **Wrong directory in the title over SSH?** The title shows `host:path` when the
  host isn't this machine – that's the remote directory.

How it works inside: [How prompt marks work](../explanations/prompt-marks.md).
