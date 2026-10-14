#!/usr/bin/env bash
# Builds target/bundle/Numero.app from a release build of numero-app and
# signs it ad hoc. `just bundle` runs it.
#
# Runs on macOS, in the dev shell or with cargo on PATH. It builds for the Mac
# it runs on, arm64 on Apple silicon.
#
# The bundle:
#   Contents/Info.plist              packaging/macos/Info.plist
#   Contents/MacOS/numero-app        the app, CFBundleExecutable
#   Contents/Resources/numero.icns   crates/numero-app/assets/app-icon
#
# The signature is ad hoc: the app runs on this Mac, but Gatekeeper refuses it
# on another until it is signed with a Developer ID and notarized.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/packaging/macos"
app="$root/target/bundle/Numero.app"
release="${CARGO_TARGET_DIR:-$root/target}/release"
icns="$root/crates/numero-app/assets/app-icon/numero.icns"

fail() {
    echo "bundle: $*" >&2
    exit 1
}

check_host() {
    [ "$(uname -s)" = Darwin ] || fail "the Mac app builds on macOS only"
    command -v cargo >/dev/null || fail "no cargo; run this inside the dev shell"
    [ -f "$icns" ] || fail "no icon at $icns"
}

# `version` under [workspace.package] in the root Cargo.toml.
workspace_version() {
    awk '
        /^\[/ { in_package = ($0 == "[workspace.package]") }
        in_package && /^version[[:space:]]*=/ {
            gsub(/^[^"]*"|"[^"]*$/, "")
            print
            exit
        }
    ' "$root/Cargo.toml"
}

make_app() {
    local version
    version=$(workspace_version)
    [ -n "$version" ] || fail "no workspace version in Cargo.toml"

    rm -rf "$app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    sed "s|@version@|$version|g" "$here/Info.plist" >"$app/Contents/Info.plist"
    plutil -lint -s "$app/Contents/Info.plist"
    printf 'APPL????' >"$app/Contents/PkgInfo"
    cp "$release/numero-app" "$app/Contents/MacOS/numero-app"
    cp "$icns" "$app/Contents/Resources/numero.icns"
}

# Refuses a binary that loads a library from /nix/store, which no other Mac
# has.
check_app() {
    if otool -L "$app/Contents/MacOS/numero-app" | grep -q /nix/store; then
        fail "numero-app links a library from /nix/store: $(otool -L "$app/Contents/MacOS/numero-app" | grep /nix/store)"
    fi
}

sign_app() {
    codesign --force --deep -s - "$app"
    codesign --verify --deep --strict "$app"
}

main() {
    check_host
    cargo build --manifest-path "$root/Cargo.toml" --release --locked -p numero-app
    make_app
    check_app
    sign_app
    echo "bundle: built $app ($(du -sh "$app" | cut -f1)), signed ad hoc"
}

main "$@"
