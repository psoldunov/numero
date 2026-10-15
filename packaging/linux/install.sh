#!/usr/bin/env bash
# Installs or removes Numero for the current user, with no Nix needed at
# runtime. `just install-linux` and `just uninstall-linux` run it.
#
#   install.sh install     copies the release binaries, the launcher entry and
#                          the icons, and registers the numero:// handler
#   install.sh uninstall   removes what install put there
#
# Binaries go to ~/.local/bin; the entry and icons to $XDG_DATA_HOME
# (~/.local/share by default). `install` expects `cargo build --release` to
# have run. Only HOME, XDG_DATA_HOME and (for xdg-mime) XDG_CONFIG_HOME are touched.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
assets="$root/crates/numero-app/assets/linux"
release="${CARGO_TARGET_DIR:-$root/target}/release"

bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
applications="$data_dir/applications"
icons="$data_dir/icons/hicolor"

binaries=(numero-app numero)
# The Wayland app_id; the launcher entry is named after it.
app_id=dev.soldunov.numero

fail() {
    echo "install: $*" >&2
    exit 1
}

# Refreshes the desktop's caches, for the tools that are installed.
refresh_caches() {
    if command -v update-desktop-database >/dev/null; then
        update-desktop-database "$applications" || true
    fi
    if command -v gtk-update-icon-cache >/dev/null; then
        gtk-update-icon-cache -f -t "$icons" || true
    fi
}

install_app() {
    local binary
    for binary in "${binaries[@]}"; do
        [ -x "$release/$binary" ] || fail "no $release/$binary; run cargo build --release first"
    done

    mkdir -p "$bin_dir"
    for binary in "${binaries[@]}"; do
        install -m 755 "$release/$binary" "$bin_dir/$binary"
    done

    # Exec names the installed binary by its path: a launcher's PATH often
    # lacks ~/.local/bin.
    "$root/packaging/linux/desktop-entry.sh" "$assets/numero.desktop" \
        "$applications/$app_id.desktop" "$bin_dir/numero-app"

    mkdir -p "$icons"
    cp -r "$assets/icons/hicolor/." "$icons/"

    refresh_caches
    if command -v xdg-mime >/dev/null; then
        xdg-mime default "$app_id.desktop" x-scheme-handler/numero
    else
        echo "install: no xdg-mime; numero:// links are not registered" >&2
    fi

    echo "install: Numero is in $bin_dir, $applications and $icons"
    case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) echo "install: $bin_dir is not on PATH; add it to run \`numero\` from a terminal" ;;
    esac
}

uninstall_app() {
    local binary icon
    for binary in "${binaries[@]}"; do
        rm -f "$bin_dir/$binary"
    done
    rm -f "$applications/$app_id.desktop"
    # Only the files the install copied; hicolor itself is the desktop's.
    while IFS= read -r icon; do
        rm -f "$icons/$icon"
    done < <(cd "$assets/icons/hicolor" && find . -type f)

    refresh_caches
    echo "install: removed Numero from $bin_dir, $applications and $icons"
    echo "install: its calculations stay in your data directory; delete them by hand to remove them too"
}

case "${1:-}" in
install) install_app ;;
uninstall) uninstall_app ;;
*) fail "usage: install.sh install|uninstall" ;;
esac
