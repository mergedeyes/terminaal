# Updates

Terminaal can update itself from its [GitHub releases](https://github.com/mergedeyes/terminaal/releases).

## How it works

1. **At start**, Terminaal asks GitHub for the latest release, in the
   background – start-up never waits for it. Nothing else is sent.
2. **If there's a newer version**, a dialog at the top of the window shows what's
   new: **Update now**, **Later** or **Skip this version**. **What's new?** opens
   the release page. You can keep typing meanwhile; the dialog doesn't take the
   keyboard.
3. **Update now** downloads the new program and checks it against the SHA-256
   checksum published with the release. Only a download that matches replaces
   the installed program, in one step (it's written next to it and then renamed),
   so a broken or cut-off download never leaves a broken Terminaal behind.
4. **Restart now** starts the new version. The programs running in your tabs end;
   with **Restore last session** (Settings → General, on by default) the tabs
   themselves come back, in the same folders and connected to the same hosts.
   **Later** keeps the old version running – the new one starts next time.

**Later** offers the update again at the next start; **Skip this version** doesn't,
until an even newer one comes out (`update_skipped` in `config.toml` remembers it).

## Settings

**Settings → General → Updates**:

- **Installed version**
- **Check for updates at start** (`update_check`, on by default)
- **Check now** – asks right away and says whether Terminaal is up to date.
  Skipped versions are offered again when you ask.

## Where it can't update itself

- **Installed by a package manager** (in `/usr/bin`, for example): Terminaal may
  not write there, says so and points to the release page. Update it the way you
  installed it.
- **No ready-made program for your machine**: releases carry a build for x86_64
  Linux. Elsewhere the dialog offers the release page instead.
- **Built from source** with `./install.sh`: updating replaces
  `~/.cargo/bin/terminaal` with the release build, which works the same. To stay
  on your own builds, turn off **Check for updates at start** and run
  `./install.sh` after pulling.

Development builds (`cargo run`) don't check at start.

## What's downloaded

| File | Size | What for |
| --- | --- | --- |
| `api.github.com/repos/mergedeyes/terminaal/releases/latest` | a few KB | Which version is the newest, its notes and files |
| `terminaal-x86_64-linux.sha256` | 90 bytes | The checksum, on **Update now** |
| `terminaal-x86_64-linux` | the program | On **Update now** |

All over HTTPS, checked with your system's certificates through OpenSSL. The
desktop entry, icons and man page aren't updated – they rarely change; unpack the
release archive and run its `install.sh` to refresh them.
