# How prompt marks work

Shell integration looks simple from the outside – jump to a prompt, show an exit
code. Under the hood there are a few interesting problems. This page is for the
curious and for contributors.

## The problem

Shells announce prompts with OSC 133 sequences and the working directory with
OSC 7. But `alacritty_terminal`, which parses the byte stream into the grid,
doesn't understand them and silently drops them.

Two kinds of information need different treatment:

- **When** something happened – a command started, finished, the directory
  changed. The exact position in the grid doesn't matter.
- **Where** a prompt is – for jumping and for putting the exit code next to it.
  This position has to stay correct while the screen scrolls, old lines fall out
  of the scrollback, and lines rewrap when the window is resized.

## A filter in front of the parser

Terminaal puts a small state machine in front of the parser
(`src/terminal/integration.rs`). It walks the bytes, passes everything through
unchanged, and only collects OSC sequences to look at them:

- **OSC 7** → a "working directory" event; the sequence itself passes on.
- **OSC 133 `C`** → "command started", **`D;code`** → "command finished".
- Everything else is untouched.

It keeps no more than an unfinished OSC sequence between reads, so a sequence
split across two reads still works. Very long sequences (over 4096 bytes, such as
clipboard transfers) pass through without being collected. Throughput is around
700 MB/s in a release build – far from a bottleneck.

### Getting the bytes

- **SSH tabs** are easy: Terminaal's own worker feeds the parser, so the filter
  sits right there.
- **Local tabs** use alacritty's PTY event loop, which reads the PTY itself.
  Terminaal wraps the PTY: the event loop reads from one end of a socket pair,
  while a separate thread reads the real PTY, filters, and writes into the other
  end. Writing, resizing and noticing the shell's exit still go to the PTY
  directly. The byte order is preserved, so positions stay exact.

Why not filter inside the event loop's `read`? Filtering can make the output
longer. The event loop may stop reading with a full buffer and only return once
the file is readable again – anything held back would wait until the shell
printed something else, and a prompt could stay invisible until you pressed a
key.

## Marks that move with the text

To remember *where* a prompt is, Terminaal doesn't store line numbers – those
break on scrolling and rewrapping. Instead it uses something the grid already
tracks per cell: **hyperlinks** (OSC 8).

When the filter sees `133;A`, it emits an OSC 8 hyperlink with a private scheme,
`terminaal-prompt:`. Every cell printed while a hyperlink is open carries it, so
the prompt's characters are now marked – and the marks travel with those
characters wherever the grid moves them.

The mark is closed again at the first of:

- `133;B` (end of prompt), `C` or `D`
- the next `A`
- the end of the prompt's first line that has visible text on it

The last rule protects against shells or prompt tools that never send `B`:
without it, the whole session would be marked.

Selection, copying and search ignore hyperlinks, so the marks are invisible. The
link finder ignores the private scheme, so they're not clickable either.

### Exit codes

When a command finishes, its exit code is known – but its prompt has already been
printed. So the code goes into the *next* prompt's mark:
`terminaal-prompt:exit=2`. When drawing, each prompt on screen takes its status
from the prompt after it. For the last prompt on screen, Terminaal looks up to
500 lines below it.

A `D` without a preceding `C` is ignored. Shells send `D` at every prompt,
including after an empty Enter, where the status would still be that of the
command before – and the same failure would show up again next to an empty line.

### Finding prompts

A prompt's row is the first row carrying its mark: if the row above carries the
same hyperlink (same ID), it's a wrapped continuation. Jumping walks rows from the
top of the screen upwards (or downwards) until it finds such a row, then scrolls
it to the top.

## Why not…

- **…store positions as line numbers?** They shift with every line of output
  once the scrollback is full, and rewrapping on resize changes them completely.
- **…insert an invisible character?** It would be copied along with selected
  text and found by search.
- **…patch alacritty_terminal?** Terminaal uses it as a published crate;
  everything here works with its public API.

## Limits

- A real OSC 8 link *inside* the prompt replaces the mark from there on (the
  prompt stays findable by its first characters).
- The exit code of a command is only shown once the next prompt appears.
- Over SSH, marks exist only if the remote shell sends OSC 133.
