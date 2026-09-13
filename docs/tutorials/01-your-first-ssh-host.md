# Tutorial: your first SSH host

In this tutorial you'll save a server in Terminaal, connect to it, give it a
second login, and reach a server behind it through a jump host.

You need a server you can already reach with `ssh` – or at least its address
and a user name.

## 1. Add the host

1. Open the sidebar (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> if it's hidden)
   and switch to the **SSH** section.
2. Click **+ Add host**.
3. Fill in:
   - **Host**: the address, e.g. `server.example.com` or `192.168.1.10`
   - **Name**: how it shows in the list, e.g. `web1`. Leave it empty to use
     the address
   - **Port**: `22` unless your server uses another one
   - **Login**: the user name. Leave the key on **Automatic** for now
4. Click **Save**.

The host is now in `~/.config/terminaal/hosts.toml`.

> **Already have hosts in `~/.ssh/config`?** They're listed under
> **From ~/.ssh/config**, and you can connect to them right away. Click
> **Adopt** to copy one into your saved hosts and edit it.

## 2. Connect

Double-click the host, or select it and click **▶ Connect**. A new tab opens and
the connection plays out inside it, just like `ssh`:

- **First connection:** Terminaal shows the server's host key fingerprint and
  asks whether to trust it. Compare it with what your provider or admin gave
  you, then type `yes`. The key is appended to `~/.ssh/known_hosts`.
- **Authentication:** Terminaal tries your SSH agent's keys, then key files,
  then `~/.ssh/id_*`, then keyboard-interactive or a password. Passphrases and
  passwords are asked in the tab and never saved.

Once you're in, the tab's title changes to `user@host` – or to the remote
working directory if the server's shell reports it.

Close the connection like any tab (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd>),
or type `exit`.

## 3. Pin a key to the host

"Automatic" is convenient, but a server that allows only a few attempts
(`MaxAuthTries`) can refuse you if your agent holds many keys. Give the host its
own key:

1. Make sure the key is in the **Keys** section (see
   [Keys and the SSH agent](02-keys-and-the-agent.md)).
2. Select the host, click **✏**, and choose the key in the login's key dropdown.
3. **Save**.

Now only that key is offered – also when it comes from the agent.

## 4. A second login

Say you log in as `deploy` day to day, and sometimes as `root`.

1. Edit the host and click **+ Another login**.
2. Enter `root` and pick its key.
3. **Save**.

The list now shows a **▶** button per login. Double-clicking the host still
uses the first login (the default); **★** on another login makes it the
default.

From the command line, `terminaal --connect root@web1` picks that login.

## 5. Through a jump host

Suppose `db1` is only reachable from `web1`.

1. Add `db1` like before, with its internal address as **Host**.
2. Under **Jump host (ProxyJump)**, choose `web1`.
3. **Save** and connect.

Terminaal connects to `web1` first and tunnels through it to `db1`. Each hop
checks its own host key and uses its own login. Chains work too: a jump host can
have a jump host of its own.

## 6. Keep the connection alive

Terminaal sends a keepalive every 30 seconds and gives up after three
unanswered ones, so a dead connection shows up as an error instead of a frozen
tab. To change that, open **Advanced → Connection** in the host form
(`ServerAliveInterval`, `ServerAliveCountMax`).

## What you learned

- Hosts live in the SSH section, saved in `hosts.toml`
- Connecting happens inside the tab, secrets are asked and never stored
- A host can have several logins, each with its own key
- Jump hosts are a dropdown away

**Next:** [Keys and the SSH agent](02-keys-and-the-agent.md), or the full list of
per-host options in [SSH hosts and `~/.ssh/config`](../guides/ssh-hosts.md).
