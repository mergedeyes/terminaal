#!/usr/bin/env bash
# Installs Terminaal for the current user: the binary via `cargo install`
# (~/.cargo/bin) plus desktop entry and icons under $XDG_DATA_HOME (default
# ~/.local/share). The desktop entry names the binary by its absolute path,
# since launchers don't necessarily have ~/.cargo/bin in their PATH. The
# compositor matches windows to the entry via their app_id / WM_CLASS
# `terminaal`, so `cargo run` builds get the icon too.
#
#   ./install.sh              binary + desktop entry + icons
#   ./install.sh --no-binary  desktop entry + icons only
#   ./install.sh --uninstall  remove all of it again
set -euo pipefail
cd "$(dirname "$0")"

data="${XDG_DATA_HOME:-$HOME/.local/share}"
icons="$data/icons/hicolor"
apps="$data/applications"
bin="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/terminaal"
sizes=(16 24 32 48 64 128 256 512)

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
        refresh
        if [[ -e "$bin" ]]; then
            cargo uninstall terminaal
        fi
        echo "Terminaal, Desktop-Eintrag und Symbole entfernt."
        exit 0
        ;;
    *)
        echo "Aufruf: ./install.sh [--no-binary | --uninstall]" >&2
        exit 2
        ;;
esac

if ! command -v magick >/dev/null; then
    echo "ImageMagick (magick) wird zum Skalieren des Symbols benötigt." >&2
    exit 1
fi

if [[ "${1:-}" != --no-binary ]]; then
    cargo install --path . --locked
fi

for size in "${sizes[@]}"; do
    dir="$icons/${size}x${size}/apps"
    mkdir -p "$dir"
    magick assets/terminaal_logo.png -resize "${size}x${size}" "PNG32:$dir/terminaal.png"
done
mkdir -p "$apps"
sed "s|^Exec=.*|Exec=$bin|" assets/terminaal.desktop >"$apps/terminaal.desktop"
chmod 644 "$apps/terminaal.desktop"
refresh
echo "Desktop-Eintrag und Symbole installiert ($data), Programm: $bin"

if [[ ! -x "$bin" ]]; then
    echo "Hinweis: $bin fehlt noch -- ohne --no-binary ausführen, sonst kann der Launcher Terminaal nicht starten." >&2
fi
