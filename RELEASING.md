# Making a release

A release is a version tag. Pushing it starts the
[release workflow](.github/workflows/release.yml), which builds Terminaal on
GitHub and publishes the release; every Terminaal out there then offers the
update at its next start ([Updates](docs/guides/updates.md)).

## Steps

1. **Pick the version.** `MAJOR.MINOR.PATCH`, higher than the last one –
   Terminaal compares the numbers (`0.10.0` is newer than `0.9.0`). Tags with a
   suffix (`1.0.0-rc1`) are never offered as updates.
2. **Bump it** in `Cargo.toml` (`version = "0.2.0"`), then `cargo build` so
   `Cargo.lock` follows, and run `cargo test` and `cargo clippy --all-targets`.
   The workflow doesn't run the tests (some need tools only your machine has,
   like `/usr/lib/ssh/sftp-server`).
3. **Commit and merge** that into `main` as usual (a PR "Release 0.2.0").
4. **Tag and push the tag** from an up-to-date `main`:

   ```sh
   git switch main && git pull
   git tag v0.2.0
   git push terminaal v0.2.0
   ```

   The tag must be `v` plus exactly the version in `Cargo.toml`; otherwise the
   workflow stops before building.
5. **Watch it** under *Actions* on GitHub (about ten minutes). It publishes the
   release with notes generated from the merged pull requests – their titles are
   what users see in the update dialog, so give PRs readable titles. Edit the
   notes on the release page afterwards if you like.

## What a release contains

| File | For |
| --- | --- |
| `terminaal-x86_64-linux` | The program. Terminaal's updater downloads exactly this name |
| `terminaal-x86_64-linux.sha256` | Its checksum; without it, no update is offered |
| `terminaal-x86_64-linux.tar.gz` | First installs: program, `install.sh`, desktop entry, icons, man page |
| `terminaal-x86_64-linux.tar.gz.sha256` | Its checksum |

Built on Ubuntu 22.04, so the program runs with glibc 2.35 or newer – practically
every current distribution. It uses OpenSSL 3, zlib and libxkbcommon from the
system.

## If something goes wrong

- **The workflow failed**: fix it on `main`, then delete the tag and push it again
  (`git tag -d v0.2.0 && git push terminaal :v0.2.0`, then step 4). Delete a
  half-made release on GitHub first.
- **A broken release went out**: publish a fixed one with a higher version.
  Deleting the release only stops new updates to it.

## Trying an update before releasing

`TERMINAAL_UPDATE_URL` makes Terminaal ask another address instead of GitHub,
and makes development builds check at start too. Serve a file in the format of
GitHub's API (`tag_name`, `html_url`, `body`, `assets` with `name` and
`browser_download_url`) next to a binary and its checksum:

```sh
mkdir -p /tmp/rel && cp target/release/terminaal /tmp/rel/terminaal-x86_64-linux
(cd /tmp/rel && sha256sum terminaal-x86_64-linux > terminaal-x86_64-linux.sha256)
cat > /tmp/rel/latest.json <<'JSON'
{"tag_name": "v99.0.0", "html_url": "http://127.0.0.1:8765/", "body": "* Test",
 "assets": [
  {"name": "terminaal-x86_64-linux", "browser_download_url": "http://127.0.0.1:8765/terminaal-x86_64-linux"},
  {"name": "terminaal-x86_64-linux.sha256", "browser_download_url": "http://127.0.0.1:8765/terminaal-x86_64-linux.sha256"}]}
JSON
python3 -m http.server 8765 --bind 127.0.0.1 --directory /tmp/rel &
cp target/debug/terminaal /tmp/terminaal-old
TERMINAAL_UPDATE_URL=http://127.0.0.1:8765/latest.json /tmp/terminaal-old
```

**Update now** then replaces `/tmp/terminaal-old`, not your installed Terminaal –
the updater always replaces the program that's running.
