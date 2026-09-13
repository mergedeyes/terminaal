# Search, links and the scrollback

## Scrolling

- **Mouse wheel:** 3 lines per notch by default (**Settings → Terminal → Mouse
  wheel**, 1–20). Touchpads scroll smoothly, pixel by pixel.
- **Keyboard:** <kbd>Shift</kbd>+<kbd>PageUp</kbd>/<kbd>PageDown</kbd> by page,
  <kbd>Shift</kbd>+<kbd>Home</kbd>/<kbd>End</kbd> to the top or bottom.
- **Typing** jumps back to the bottom.
- **How much is kept:** **Settings → Terminal → Scrollback** (default 10 000 lines
  per tab). Lowering it drops the oldest lines of every tab.

### In full-screen programs

Programs like `less`, `man`, `vim` and `htop` use the "alternate screen", which
has no scrollback. There:

- With mouse support (`htop`, `mc`, `vim` with `set mouse=a`) the wheel goes to
  the program as wheel events.
- Without (`less`, `man`) it becomes arrow keys.
- <kbd>Shift</kbd>+wheel always scrolls Terminaal's own scrollback instead.
- The keyboard scroll shortcuts go to the program.

## Search

<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> opens the search bar in the
bottom right corner (it moves to the top if it would cover the match).

**While typing the query:**

| Key | Does |
| --- | --- |
| characters | search as you type, upwards from the bottom of the screen |
| <kbd>Backspace</kbd> | remove a character; an empty query scrolls back to where you started |
| <kbd>Enter</kbd> | next match upwards (older), and stop typing |
| <kbd>Shift</kbd>+<kbd>Enter</kbd> | next match downwards (newer), and stop typing |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> | paste into the query |
| <kbd>Esc</kbd> | close |

**After Enter:**

| Key | Does |
| --- | --- |
| <kbd>n</kbd> | next match up |
| <kbd>N</kbd> | next match down |
| <kbd>/</kbd> or <kbd>Backspace</kbd> | edit the query again (a click on the bar too) |
| <kbd>Esc</kbd> | close |
| anything else | close the bar, and the key goes to the shell |

Good to know:

- The query is plain text, not a regex: `a.b` finds "a.b".
- Case is ignored until you type a capital letter.
- Matches across wrapped lines are found.
- Searching wraps around the ends of the scrollback.
- While typing, each keystroke looks at most 1000 lines up from where the search
  started; <kbd>Enter</kbd> searches the whole scrollback.
- All matches on screen are highlighted, the current one brighter. Themes can set
  both colors (`[colors.search]`).

## Selection and clipboard

- Drag with the left mouse button to select.
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> or right-click → **Copy**.
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> or right-click → **Paste**.
- Right-click → **Paste and run**: pastes, removes trailing line breaks and
  presses Enter exactly once. Give it a shortcut in the settings if you use it
  often.

Pasting is safe: when the program supports bracketed paste (bash, zsh, fish, vim
do), a multi-line paste is inserted as a whole instead of running line by line.
Escape characters and Ctrl+C in the clipboard are always removed, so pasted text
can't smuggle in terminal sequences.

## Links

Hold <kbd>Ctrl</kbd>: whatever is clickable under the mouse gets underlined and
the pointer becomes a hand. <kbd>Ctrl</kbd>+click opens it with your default
application (`xdg-open`).

What counts, in this order:

1. **Hyperlinks** a program set (OSC 8) – e.g. `ls --hyperlink`, `gcc`, `systemctl`
   in newer versions. Opened only for common schemes: `https://`, `http://`,
   `mailto:`, `file:`, `ftp://`, `git://`, `gemini://`, `gopher://`, `news:`,
   `magnet:`, `ipfs:`, `ipns:`. A program can't make a click launch an arbitrary
   URL handler.
2. **URLs** in the text, with those schemes. A trailing full stop, comma or an
   unmatched closing bracket isn't part of the link.
3. **Files and folders that exist**: absolute paths, `~/…`, or names relative to
   the shell's working directory (needs [shell integration](shell-integration.md)).
   `src/main.rs:42:7` opens `src/main.rs`. Only in local tabs – over SSH the files
   would be on the server.

An **executable file** isn't opened (that could run it); its folder opens
instead.
