#!/usr/bin/env bash
# Writes Numero's launcher entry for an install: the asset in
# crates/numero-app/assets/linux, with the lines an install adds.
#
#   desktop-entry.sh <source .desktop> <destination .desktop> [program]
#
# `program` is what Exec starts; it defaults to the bare name, found on PATH.
# Nix passes the store path, so the entry works whatever PATH the launcher has.
#
# The asset stays the plain launcher entry. Here it gains:
#   - `%U` on Exec, so the file manager and `numero://` links reach the app;
#   - MimeType, which makes Numero a handler for its scheme and for text files.
set -euo pipefail

source=${1:?usage: desktop-entry.sh <source> <destination> [program]}
destination=${2:?usage: desktop-entry.sh <source> <destination> [program]}
program=${3:-numero-app}

mime='x-scheme-handler/numero;text/plain;'

mkdir -p "$(dirname "$destination")"
{
    while IFS= read -r line; do
        case $line in
        Exec=*) printf 'Exec=%s %%U\n' "$program" ;;
        MimeType=*) ;; # replaced below; the asset has none today
        *) printf '%s\n' "$line" ;;
        esac
    done <"$source"
    printf 'MimeType=%s\n' "$mime"
} >"$destination"
chmod 644 "$destination"
