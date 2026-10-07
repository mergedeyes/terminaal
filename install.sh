#!/usr/bin/env bash
# Installs Terminaal for the current user, plus desktop entry, icons and the
# manual page under $XDG_DATA_HOME (default ~/.local/share).
#
# Two ways in:
#   - From the source code (Cargo.toml next to this script): the binary is
#     built with `cargo install` (~/.cargo/bin); ImageMagick scales the icon.
#   - From a release download (terminaal-x86_64-linux.tar.gz, unpacked): the
#     ready-made binary goes to ~/.local/bin, the icons are already scaled.
#     Terminaal updates itself from there (Settings → General → Updates).
#
# The desktop entry names the binary by its absolute path, since launchers
# don't necessarily have ~/.cargo/bin or ~/.local/bin in their PATH. The
# compositor matches windows to the entry via their app_id / WM_CLASS
# `terminaal`, so `cargo run` builds get the icon too.
#
#   ./install.sh              binary + desktop entry + icons + man page
#   ./install.sh --no-binary  desktop entry + icons + man page only
#   ./install.sh --uninstall  remove all of it again
set -euo pipefail
cd "$(dirname "$0")"

data="${XDG_DATA_HOME:-$HOME/.local/share}"
icons="$data/icons/hicolor"
apps="$data/applications"
man="$data/man/man1"
cargo_bin="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/terminaal"
local_bin="$HOME/.local/bin/terminaal"
sizes=(16 24 32 48 64 128 256 512)

# A release download carries the binary itself; the source code doesn't.
if [[ -f Cargo.toml ]]; then
    prebuilt=false
    bin="$cargo_bin"
else
    prebuilt=true
    bin="$local_bin"
fi

refresh() {
    gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
    update-desktop-database -q "$apps" 2>/dev/null || true
}

case "${1:-}" in
    "" | --no-binary) ;;
    --uninstall)
        for size in "${sizes[@]}"; do
            rm -f "$icons/${size}x${size}/apps/terminaal.png"
        done
        rm -f "$apps/terminaal.desktop"
        rm -f "$man/terminaal.1"
        refresh
        if [[ -e "$cargo_bin" ]] && command -v cargo >/dev/null; then
            cargo uninstall terminaal
        fi
        rm -f "$local_bin"
        echo "Terminaal, Desktop-Eintrag, Symbole und Handbuchseite entfernt."
        exit 0
        ;;
    *)
        echo "Aufruf: ./install.sh [--no-binary | --uninstall]" >&2
        exit 2
        ;;
esac

if ! $prebuilt && ! command -v magick >/dev/null; then
    echo "ImageMagick (magick) wird zum Skalieren des Symbols benötigt." >&2
    exit 1
fi

if [[ "${1:-}" != --no-binary ]]; then
    if $prebuilt; then
        mkdir -p "$(dirname "$local_bin")"
        install -m 755 terminaal "$local_bin"
    else
        cargo install --path . --locked
    fi
fi

for size in "${sizes[@]}"; do
    dir="$icons/${size}x${size}/apps"
    mkdir -p "$dir"
    if $prebuilt; then
        install -m 644 "icons/$size.png" "$dir/terminaal.png"
    else
        magick assets/terminaal_logo.png -resize "${size}x${size}" "PNG32:$dir/terminaal.png"
    fi
done
mkdir -p "$apps"
sed "s|^Exec=.*|Exec=$bin|" assets/terminaal.desktop >"$apps/terminaal.desktop"
chmod 644 "$apps/terminaal.desktop"
mkdir -p "$man"
install -m 644 assets/terminaal.1 "$man/terminaal.1"
refresh
echo "Desktop-Eintrag, Symbole und Handbuchseite installiert ($data), Programm: $bin"

if [[ ! -x "$bin" ]]; then
    echo "Hinweis: $bin fehlt noch -- ohne --no-binary ausführen, sonst kann der Launcher Terminaal nicht starten." >&2
fi
