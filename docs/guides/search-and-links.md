# Search, links and the scrollback

## Scrolling

- **Mouse wheel:** 3 lines per notch by default (**Settings → Terminal → Mouse
  wheel**, 1–20). Touchpads scroll smoothly, pixel by pixel.
- **Scrollbar:** at the right edge of each terminal. It shows while you're
  scrolled back, for a moment after scrolling, and when the pointer is on the
  edge. Drag the thumb, or click beside it to jump there. Off with **Settings →
  Terminal → Scrollbar** (`scrollbar = false`). Full-screen programs have none.
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

## Vi mode

<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Space</kbd> gives the terminal a second
cursor that moves through the scrollback with vi's keys – to select and copy
without the mouse. It starts where the terminal's cursor is; the bar at the
bottom says on which line of how many you are. Nothing you type reaches the
program meanwhile; the program keeps running and printing.

| Key | Does |
| --- | --- |
| <kbd>h</kbd> <kbd>j</kbd> <kbd>k</kbd> <kbd>l</kbd>, arrows | left, down, up, right |
| <kbd>w</kbd> <kbd>b</kbd> <kbd>e</kbd> | next word, word back, end of word (words as for a double-click) |
| <kbd>W</kbd> <kbd>B</kbd> <kbd>E</kbd> | the same for words between spaces |
| <kbd>0</kbd> <kbd>^</kbd> <kbd>$</kbd>, <kbd>Home</kbd> <kbd>End</kbd> | start of line, first character, end of line |
| <kbd>H</kbd> <kbd>M</kbd> <kbd>L</kbd> | top, middle, bottom of the screen |
| <kbd>g</kbd> / <kbd>G</kbd> | top of the scrollback / the bottom |
| <kbd>{</kbd> <kbd>}</kbd>, <kbd>%</kbd> | paragraph up / down, matching bracket |
| <kbd>Ctrl</kbd>+<kbd>U</kbd> / <kbd>D</kbd>, <kbd>Ctrl</kbd>+<kbd>B</kbd> / <kbd>F</kbd>, <kbd>PageUp</kbd> / <kbd>PageDown</kbd> | half a page, a page up / down |
| <kbd>Ctrl</kbd>+<kbd>Y</kbd> / <kbd>E</kbd> | scroll a line, the cursor along |
| <kbd>v</kbd>, <kbd>V</kbd>, <kbd>Ctrl</kbd>+<kbd>V</kbd>, <kbd>Alt</kbd>+<kbd>V</kbd> | select characters, lines, a block, words – from the cursor on; the same key again ends the selection, another changes its kind |
| <kbd>y</kbd> | copy the selection and leave vi mode |
| <kbd>/</kbd> or <kbd>?</kbd> | search; the cursor goes to each match, to select from there |
| <kbd>Enter</kbd> | open the link under the cursor |
| <kbd>Esc</kbd> | drop the selection; without one, leave vi mode |
| <kbd>i</kbd>, <kbd>q</kbd>, <kbd>Ctrl</kbd>+<kbd>C</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Space</kbd> | leave vi mode |

A click moves the vi cursor there, and a selection made with the mouse goes on
with the keys. Vi mode belongs to one terminal: in a split tab, the others keep
taking typing.

## Selection and clipboard

- Drag with the left mouse button to select.
- Double-click selects a word – an IP address, a file name or a whole path.
  With **Double-click marks one part of a path** (Settings → Terminal) only the
  folder or file you clicked. Triple-click selects the whole line. Keep the button down and drag to extend by words or lines.
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> or right-click → **Copy**.
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> or right-click → **Paste**.
- Right-click → **Paste and run**: pastes, removes trailing line breaks and
  presses Enter exactly once. Give it a shortcut in the settings if you use it
  often.

Pasting is safe: when the program supports bracketed paste (bash, zsh, fish, vim
do), a multi-line paste is inserted as a whole instead of running line by line.
Escape characters and Ctrl+C in the clipboard are always removed, so pasted text
can't smuggle in terminal sequences.

### Programs and the clipboard (OSC 52)

Programs can copy into the clipboard themselves with the OSC 52 escape
sequence – vim, Neovim, tmux, and that works over SSH too: copy in vim on the
server and paste on your computer. That's on by default; turn it off with
**Programs may write to the clipboard** (Settings → Terminal → Clipboard for
programs, `clipboard_write`).

The other direction – a program *reading* your clipboard – is riskier: it would
get whatever you copied last, a password too. By default Terminaal asks each
time (`clipboard_read = "ask"`): a dialog at the top names the terminal and how
many characters the clipboard holds, never the text itself.

- **Allow** – this once.
- **Always for this terminal** – this and every later request of that
  terminal, until it closes.
- **Deny** – the program gets an empty clipboard right away.

The dialog doesn't take the keyboard – an <kbd>Enter</kbd> typed just then goes
to the terminal, not to the dialog – and lapses unanswered after 20 seconds.
Only one terminal can ask at a time; another one asking meanwhile is turned
down. `"never"` answers every request with an empty clipboard, `"always"`
hands it over without asking.

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

## Hints

<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>H</kbd> labels everything on the screen
worth picking with a letter or two, highlighted over its start, and the bar at
the bottom shows how many there are. Type a label:

- its letters **copy** the text,
- with <kbd>Shift</kbd> held for the last letter, it's **opened** like a
  <kbd>Ctrl</kbd>-clicked link (what can't be opened – an IP, a hash – is copied),
- with <kbd>Alt</kbd>, it's **typed into the terminal** – a hash into
  `git show `, a path into the command line.

<kbd>Backspace</kbd> takes a letter back, <kbd>Esc</kbd>, a click or any other key
ends the hints (the key then goes on as usual). The labels follow the screen: new
output or scrolling relabels.

What gets a label, earlier kinds first where they overlap:

1. Hyperlinks a program set (OSC 8) and URLs, as for <kbd>Ctrl</kbd>+click
2. Paths: absolute, `~/…`, `./…`, `../…`, and any word naming a file or folder
   that exists here (relative to the shell's working directory). Over SSH all of
   these can be copied or inserted, but not opened – they're the server's.
3. IPv4 and IPv6 addresses – only valid ones, so a time like `12:30:45` or a
   version like `v1.2.3.4` isn't one
4. Hashes and IDs: hex of 7 characters or more with a letter and a digit (git
   commits, container IDs, checksums), UUIDs, MAC addresses

Labels start at the bottom of the screen, so the newest output gets the easiest
letters (`f`, `j`, `d`, `k`, …). Labels use no `y` or `z`, which swap places
between German and US keyboards.
