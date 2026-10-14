# The development shell: `nix develop`, or `direnv allow` with `.envrc`.
#
# Linux gets a C toolchain and the native libraries GPUI links or dlopens.
# macOS gets no C toolchain from Nix: the linker and SDK come from Xcode.
{
  lib,
  stdenv,
  mkShell,
  mkShellNoCC,
  rustToolchain,
  cargo-nextest,
  just,
  nixfmt,
  # Linux
  pkg-config,
  fontconfig,
  freetype,
  libglvnd,
  libx11,
  libxcb,
  libxkbcommon,
  vulkan-loader,
  wayland,
}:

let
  tools = [
    cargo-nextest
    just
    nixfmt
  ];

  # GPUI opens these with dlopen at runtime, so they also have to be on the
  # library path.
  gpuiRuntimeLibraries = [
    libglvnd
    libx11
    libxcb
    libxkbcommon
    vulkan-loader
    wayland
  ];

  linux = mkShell {
    packages = tools ++ [
      rustToolchain
      pkg-config
    ];
    buildInputs = [
      fontconfig
      freetype
    ]
    ++ gpuiRuntimeLibraries;
    LD_LIBRARY_PATH = lib.makeLibraryPath gpuiRuntimeLibraries;
  };

  darwin = mkShellNoCC {
    packages = tools;
    # The toolchain goes on PATH rather than in `packages`, so rust-overlay
    # does not propagate Nix's clang wrapper and binaries link Xcode's SDK.
    shellHook = ''
      export PATH="${rustToolchain}/bin:$PATH"
      case "''${DEVELOPER_DIR:-}" in
        "" | /nix/store/*) DEVELOPER_DIR="$(/usr/bin/xcode-select -p 2>/dev/null || true)" ;;
      esac
      case "$DEVELOPER_DIR" in
        *.app/Contents/Developer) ;;
        *) DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer ;;
      esac
      export DEVELOPER_DIR
      unset SDKROOT MACOSX_DEPLOYMENT_TARGET
    '';
  };
in
if stdenv.hostPlatform.isDarwin then darwin else linux
