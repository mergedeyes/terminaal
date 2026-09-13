# Tutorial: one command on many servers

You have several servers and want to check their disk space, update them, or
follow their logs – all at once. You'll combine two features:

- **Snippets**: your own command buttons in the sidebar
- **Broadcast**: input typed in one tab goes to a whole group of tabs

## 1. Open the servers

Connect to each server (`web1`, `web2`, `web3`) from the SSH section. Each gets
its own tab.

## 2. Put the tabs into a broadcast group

In each tab, press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> – or right-click
into the terminal and choose **Broadcast to this tab**.

Tabs in the group get a thick red line on top in the tab bar. That's your
warning: typing in one of them types in all of them.

Try it: in any of the three tabs, type `hostname` and press Enter. All three
answer.

To take a tab out, press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> in it again.
Typing in a tab that isn't in the group only affects that tab.

## 3. Create a snippet

Typing the same long command every time gets old. Make it a button:

1. Open the sidebar's **Shells** section and scroll to **Your commands**.
2. Click **Manage**, then **+ Add command**.
3. Name: `Disk hogs`
4. Command:

   ```sh
   du -xh / 2>/dev/null | sort -rh | head -15
   ```

5. Leave **Only on system** and **Only on host** on "All".
6. **Save**, then **Done**.

A **Disk hogs** button now sits under "Your commands".

## 4. Run it everywhere

With a tab of the broadcast group active, click **Disk hogs**. The sidebar says
*Broadcast: goes to 3 tabs* above the buttons, and all three servers run it.

The first time you run a command from the sidebar, Terminaal explains that it
goes straight to the shell and asks you to confirm. If you'd rather review each
command before it runs, turn off **Settings → Shell → Run commands right away**:
the command is then only typed into the prompt, and you press Enter yourself.

## 5. Commands for one system or host

Your servers run different distributions. Make a snippet that only appears where
it makes sense:

1. **Manage → + Add command**
2. Name: `Follow nginx`, command: `journalctl -fu nginx`
3. **Only on system**: *Debian, Ubuntu, Mint (apt)*

The button now shows only in tabs whose system Terminaal detected as Debian-like
(it asks each server right after login). A snippet bound to a host appears only in
that host's SSH tabs – handy for deploy scripts:

```sh
cd /srv/app
git pull
./deploy.sh
```

Several lines arrive together, as if pasted, so the shell sees the whole script
at once.

## 6. Built-in commands, too

The sidebar also has built-in buttons – update the system, free space, failed
services and more – tailored to each tab's package manager. With broadcast on,
they send the line for the *active* tab's system to every tab, so group servers
of the same kind before you use them.

## Good habits

- Keep the broadcast group small and deliberate; take tabs out when you're done.
- Watch for the red line before typing anything destructive.
- For commands that ask for confirmation (package updates), leave
  **Let commands skip their prompts** off and answer in each tab.

## What you learned

- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> puts a tab into the broadcast group
- Snippets are named commands, optionally bound to a system or host
- A snippet clicked in a broadcast tab runs in every tab of the group

**Reference:** [Built-in commands and snippets](../guides/commands-and-snippets.md).
