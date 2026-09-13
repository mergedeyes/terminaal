# Tutorial: keys and the SSH agent

You'll create an SSH key in Terminaal, install it on a server, and learn how to
use keys that live in an agent such as 1Password.

## 1. Generate a key

1. In the sidebar, open the **Keys** section and click **+ Generate**.
2. **Name**: something that tells you where it's used, e.g. `Work`.
3. **Location** follows the name (`~/.ssh/id_ed25519_work`). Change it if you
   like – Terminaal never overwrites an existing file.
4. **Passphrase**: strongly recommended. Without one, the key sits unencrypted on
   disk. You'll be asked for it in the tab when you connect; it's never saved.
5. Confirm.

You get an Ed25519 key in OpenSSH format: the private key with permissions
`0600`, and the public key next to it as `.pub`.

## 2. Put the public key on the server

1. Select the key and click **📋 Copy public key**.
2. On the server, add it to `~/.ssh/authorized_keys`:

   ```sh
   mkdir -p ~/.ssh && chmod 700 ~/.ssh
   cat >> ~/.ssh/authorized_keys   # paste, then Ctrl+D
   chmod 600 ~/.ssh/authorized_keys
   ```

   You can do this in a Terminaal tab that's logged in with a password.
3. Edit the host in the SSH section and choose `Work` as its key.

Connect again: if the key has a passphrase, the tab asks for it, and you're in
without the server's password.

## 3. Use an existing key file

Already have `~/.ssh/id_rsa` or a key from another tool? Click **+ File** and
pick the private key file. Terminaal only records the path; the file stays
where it is, and removing the entry later never deletes it.

## 4. Keys from an agent (1Password, ssh-agent, KeePassXC)

With an agent, the private key never leaves it – Terminaal only asks the agent
to sign.

1. Make sure the agent runs and `SSH_AUTH_SOCK` points at it (for 1Password:
   enable its SSH agent; the socket is usually `~/.1password/agent.sock`).
2. Click **+ From agent**. The first key you haven't taken over yet is
   preselected, and the name follows your choice until you type one yourself.
3. Confirm.

`keys.toml` now holds the key's *public* half, so Terminaal can tell the agent
which key to use. Assign it to a host like any other key: only that key is
offered, which avoids "Too many authentication failures" on servers with a low
`MaxAuthTries`.

> **A host that should use a different agent** than `SSH_AUTH_SOCK` – set
> **Advanced → Authentication → Agent socket** (`IdentityAgent`).

## 5. Rename and remove

- **Rename**: hosts using the key follow along.
- **Remove**: only possible while no host uses the key. For keys generated in
  Terminaal, a second question offers to delete the files too – and only if no
  other entry points at the same file. Key files you added, and agent keys, are
  never deleted.

## 6. Agent forwarding (optional)

If you need your keys *on* the server – say, to `git pull` from there – enable
**Advanced → Authentication → Forward the agent to the server**. Only do this for
servers you trust: anyone with root there can use your agent while you're
connected.

## What you learned

- Generated keys are Ed25519, passphrase-protected, never overwrite anything
- Agent keys stay in the agent; Terminaal stores only their public half
- Pinning one key per host avoids authentication failures

**Next:** [Port forwarding](03-port-forwarding.md).
