# Tutorial: port forwarding

Port forwards carry connections through your SSH session. You'll set up the
three common kinds, then watch and control them while connected.

The forwards are part of a host's settings and run as long as its tab is
connected. You need a saved host (see [Your first SSH host](01-your-first-ssh-host.md)).

## 1. Local: reach a database behind the server

The server `web1` can reach a PostgreSQL server at `db.internal:5432`; your
laptop can't.

1. Edit `web1` and open **Port forwarding**.
2. Click **+ Forward** and choose **Local**.
3. First field (listen here): `5432`
4. Second field (target, as the server sees it): `db.internal:5432`
5. **Save** and connect.

The tab prints a line saying the forward is up. Now, on your laptop:

```sh
psql -h localhost -p 5432 -U app
```

The connection goes to your laptop's port 5432, through the SSH session, and
from `web1` on to `db.internal`.

By default Terminaal listens on `127.0.0.1` and `::1` only. Write `*:5432` to
listen on all network interfaces (so others on your network can use it).

## 2. Remote: show your local dev server to the server

You run a web app on your laptop at `localhost:3000` and want `web1` to reach it.

1. Add a forward of kind **Remote**.
2. Listen (on the server): `8080`
3. Target (as your laptop sees it): `localhost:3000`

On the server, `curl localhost:8080` now reaches your laptop's app.

## 3. SOCKS: browse as if you were on the server

1. Add a forward of kind **SOCKS**, listen on `1080`.
2. Connect, then point a browser or tool at the SOCKS proxy `localhost:1080`:

   ```sh
   curl --socks5-hostname localhost:1080 http://intranet.internal/
   ```

Every connection continues from the server, to wherever the program asks.
SOCKS 4, 4a and 5 are supported (without authentication, like OpenSSH).

**SOCKS on server** does the reverse: a proxy on the server whose connections
continue from your machine.

## 4. Unix sockets

Anything containing a `/` is a Unix socket path, on either side of a local
forward, and as the target of a remote one:

| Listen | Target | Use |
| --- | --- | --- |
| `~/docker.sock` | `/var/run/docker.sock` | Talk to the server's Docker daemon: `DOCKER_HOST=unix://$HOME/docker.sock docker ps` |
| `5433` | `/run/postgresql/.s.PGSQL.5432` | A database that only listens on a socket |

Local socket files are created with permissions `0600` and removed when the
forward ends. (The server itself can't listen on a socket – a libssh2
limitation.)

## 5. Watch and control forwards live

While a host with forwards is the active tab, the top of the **SSH** section
lists them, each with its state:

| State | Meaning | Button |
| --- | --- | --- |
| active | Listening | **Pause** |
| setting up … | Waiting for the server to start listening | **Pause** |
| paused | You paused it | **Start** |
| failed: *reason* | e.g. "Address already in use" | **Try again** |

Typical uses:

- **Port taken?** Stop whatever uses it, then **Try again** – no reconnect needed.
- **Need the port for something else for a moment?** **Pause** a local forward
  frees it; **Start** takes it back.

A paused *remote* forward keeps its listener on the server (libssh2 can't
cancel one cleanly mid-session) and turns new connections away.

## 6. From `~/.ssh/config`

Forwards in your SSH config work too, and show up in the form when you adopt the
host:

```
Host web1
    HostName server.example.com
    LocalForward 5432 db.internal:5432
    RemoteForward 8080 localhost:3000
    DynamicForward 1080
```

## What you learned

- Local, remote and SOCKS forwards, on ports or Unix sockets
- A failing forward doesn't stop the connection – retry it from the sidebar
- Paused local forwards free their port

**Next:** [One command on many servers](04-one-command-many-servers.md).
