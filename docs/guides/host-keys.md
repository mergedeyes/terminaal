# Host keys and `known_hosts`

A host key proves you're talking to the server you think you are. Terminaal checks
it against `~/.ssh/known_hosts` – the same file OpenSSH uses – or the host's
`UserKnownHostsFile`.

## A new host

The first time you connect, the key isn't known yet. What happens depends on
**Advanced → Host key → Unknown host keys** (`StrictHostKeyChecking`):

| Setting | What happens |
| --- | --- |
| **Ask** (default) | The tab shows the key type and SHA256 fingerprint and asks `(yes/no)`. On yes, the key is saved |
| **Save new ones without asking** (`accept-new`) | Saved right away; the tab says so |
| **Known hosts only** (`yes`) | The connection is refused |

Compare the fingerprint with one you got another way – from your hosting
provider's console, or by running this on the server:

```sh
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

New entries are **appended** to the file as one line each; nothing else in it is
changed. Comments and lines Terminaal doesn't understand stay as they are.

## A changed key

If the server presents a different key than the one stored, Terminaal always
aborts, whatever the settings say:

```
WARNING: The host key of [10.0.0.5]:2222 has changed!
This may be an attack (man-in-the-middle) – or the server was set up anew.
Stored in ~/.ssh/known_hosts:
  ssh-ed25519 SHA256:Gwkz… (line 2)
New from the server: ssh-ed25519 SHA256:Oiqu…
```

**Stop and think.** Did the server get reinstalled, or its SSH keys
regenerated? Did the IP address move to another machine? If you can't explain the
change, don't connect.

## Removing the old key

When the change is expected:

1. In the SSH section, select the host and click **🔑 Host key**.
2. You see the stored entries: key type, fingerprint, line number, and which
   names each line applies to.
3. Click **Remove entry** (or **Remove N entries**) and confirm.
4. Connect again. The new key is treated like a new host – you're asked to confirm
   its fingerprint.

The removal is careful:

- Only the lines shown are removed; every other byte of the file stays the same.
- The file is written next to the original and swapped in when complete, keeping
  its permissions. A symlinked `known_hosts` stays a symlink.
- If the file changed since you opened the view, nothing is removed – reload and
  try again.
- A line may list several names (`web1,10.0.0.5 ssh-ed25519 …`). The whole line
  goes, so the confirmation tells you which names it covers.

The command-line way works too: `ssh-keygen -R '[10.0.0.5]:2222'`.

## Details

- **Hashed entries** (`|1|…`) are found and removed like plain ones; the view
  shows "name hashed" instead of the names.
- **Ports:** OpenSSH stores non-standard ports as `[host]:port`. libssh2, which
  Terminaal uses, also accepts a plain `host` line for *any* port. That's why the
  view can list a `host` line for a host on port 2222 – removing only the
  `[host]:2222` line would leave the old key in force.
- **`@cert-authority` and `@revoked`** lines aren't understood by libssh2; they're
  left alone and not shown.
- **Jump hosts** are checked hop by hop. Open the view on the jump host itself to
  manage its key.
