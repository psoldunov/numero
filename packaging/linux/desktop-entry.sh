#!/usr/bin/env bash
# Writes Numero's launcher entry for an install: the asset in
# crates/numero-app/assets/linux, with the lines an install changes.
#
#   desktop-entry.sh <source .desktop> <destination .desktop> [program]
#
# `program` is what Exec starts; it defaults to the bare name, found on PATH.
# Nix passes the store path, so the entry works whatever PATH the launcher has.
#
# The destination's file name is the app id, dev.soldunov.numero: the
# GlobalShortcuts portal and the compositor find the entry by it. The asset
# keeps its short name; the installs rename it. Here the entry gains:
#   - `%U` on Exec, so `numero://` links reach the app;
#   - MimeType, which makes Numero the handler for its scheme;
#   - StartupWMClass, the Wayland app_id and X11 class the window carries.
set -euo pipefail

source=${1:?usage: desktop-entry.sh <source> <destination> [program]}
destination=${2:?usage: desktop-entry.sh <source> <destination> [program]}
program=${3:-numero-app}

app_id=dev.soldunov.numero
mime='x-scheme-handler/numero;'

mkdir -p "$(dirname "$destination")"
{
    while IFS= read -r line; do
        case $line in
        Exec=*) printf 'Exec=%s %%U\n' "$program" ;;
        # Set once, below, whatever the asset says.
        MimeType=* | StartupWMClass=*) ;;
        *) printf '%s\n' "$line" ;;
        esac
    done <"$source"
    printf 'StartupWMClass=%s\n' "$app_id"
    printf 'MimeType=%s\n' "$mime"
} >"$destination"
chmod 644 "$destination"
