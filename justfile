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

# --- Packaging ---------------------------------------------------------------

mac_pkg_dir := "/tmp/numero-pkg"

# Linux only: build the release binaries and install them for this user
# (~/.local/bin, $XDG_DATA_HOME); no Nix needed at runtime.
[linux]
install-linux:
    cargo build --release --locked -p numero-app -p numero-cli
    packaging/linux/install.sh install

# Linux only: remove what `just install-linux` installed.
[linux]
uninstall-linux:
    packaging/linux/install.sh uninstall

# macOS only: build target/bundle/Numero.app from a release build, signed ad hoc.
[macos]
bundle:
    packaging/macos/bundle.sh

# Linux only: copy the tree to the Mac and run `just bundle` there over SSH.
[linux]
mac-bundle:
    rsync -a --delete --filter=':- .gitignore' --exclude=.git ./ {{ mac_host }}:{{ mac_pkg_dir }}/
    ssh {{ mac_host }} "bash -lc 'cd {{ mac_pkg_dir }} && /nix/var/nix/profiles/default/bin/nix develop -c just bundle'"
