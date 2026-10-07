# Images in the terminal

Programs can show pictures right in the terminal with the
[kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/) –
locally and over SSH. File managers preview images, and command-line tools print
them between the text:

- [yazi](https://yazi-rs.github.io/) previews images in its right column,
- `kitten icat picture.png` (from kitty) prints one,
- `chafa -f kitty picture.png`, `timg -pk picture.png`, `viu` and others too.

Programs that ask the terminal what it can do (yazi, icat) find out by
themselves. Others decide by the name of the terminal and may need to be told:
`chafa -f kitty`, `timg -pk`.

Turn it off under **Settings → Terminal → Images** (`images = false` in
`config.toml`): programs then get no answer when they ask and fall back to
something else – coloured blocks, or no picture.

## How it behaves

- An image sits at the cursor where the program put it and **scrolls with the
  text**. Scrolled out of view and back, it's still there; `clear` (or anything
  else that erases those cells) takes it away.
- After an image, the cursor is below it on the right, as in kitty, so the next
  line of text starts under the picture.
- A program can show the same image several times, move it, and delete it.
- Images lie above the cell backgrounds and below the text.
- Selecting or copying over an image gives the blanks under it.

## What isn't supported

- **Reading image files or shared memory**: a program has to send the picture
  itself. A program on a server could otherwise name files on your computer.
  Programs that try the faster ways first (icat) fall back to sending the data.
- Animation, sixel images, and kitty's "unicode placeholders" (used for images
  inside tmux).
- Deleting by position (`d=c`, `d=p`, `d=x`, …): only all at once, by id, by
  number or by a range of ids.

A terminal keeps at most 256 MB of images; past that the oldest go. A single
image may be at most 10 000 pixels wide or high and 64 MB once decoded.
