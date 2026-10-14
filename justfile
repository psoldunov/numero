set shell := ["bash", "-euo", "pipefail", "-c"]

mac_host := env("NUMERO_MAC_HOST", "bigmac")
mac_dir := "/tmp/numero"

default:
    @just --list

# Allow .envrc and fetch crates.
setup:
    if command -v direnv >/dev/null; then direnv allow; fi
    cargo fetch

# Every gate: formatting, clippy, tests.
check: fmt-check clippy test

fmt:
    cargo fmt --all
    nixfmt flake.nix nix/*.nix

fmt-check:
    cargo fmt --all --check
    nixfmt --check flake.nix nix/*.nix

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo nextest run --workspace --no-tests=pass
    cargo test --workspace --doc

# Engine tests only; fast, no GPUI build.
test-engine:
    cargo nextest run -p numero-engine

# The GPUI app.
app:
    cargo run -p numero-app

# Evaluate a file (or stdin) line by line in the terminal.
cli *args:
    cargo run -q -p numero-cli -- {{ args }}

# Render the app icon's SVG into its PNGs and icns (needs Chrome and ImageMagick).
icon:
    crates/numero-app/assets/app-icon/render.sh

# Linux only: copy the tree to the Mac and run `just check` there over SSH.
[linux]
mac-check:
    rsync -a --delete --filter=':- .gitignore' --exclude=.git ./ {{ mac_host }}:{{ mac_dir }}/
    ssh {{ mac_host }} "bash -lc 'cd {{ mac_dir }} && /nix/var/nix/profiles/default/bin/nix develop -c just check'"
