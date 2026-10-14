# Numero for Linux, built from this checkout: the GPUI app `numero-app`, the
# terminal calculator `numero`, the launcher entry and the icons.
#
# It builds the revision the flake is at. The macOS app is built with cargo
# on a Mac (`just bundle`), not here.
{
  lib,
  rustPlatform,
  pkg-config,
  makeBinaryWrapper,
  desktop-file-utils,
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
  # GPUI opens these with dlopen at runtime, so linking alone does not put
  # them where the loader looks.
  gpuiRuntimeLibraries = [
    libglvnd
    libx11
    libxcb
    libxkbcommon
    vulkan-loader
    wayland
  ];

  assets = "crates/numero-app/assets/linux";
in
rustPlatform.buildRustPackage {
  pname = "numero";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  # Only what the build and the install read, so a change to the docs or the
  # justfile does not rebuild it.
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../packaging/linux
    ];
  };

  # No git dependencies, so no output hashes.
  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = [
    "-p"
    "numero-app"
    "-p"
    "numero-cli"
  ];

  # `just check` runs the tests; the app's need a display and a GPU, which
  # the build sandbox has none of.
  doCheck = false;

  strictDeps = true;

  nativeBuildInputs = [
    pkg-config
    makeBinaryWrapper
    desktop-file-utils
    # QuickJS, behind the extensions, is C built by rquickjs's build script,
    # which may generate bindings with bindgen.
    rustPlatform.bindgenHook
  ];

  buildInputs = [
    fontconfig
    freetype
  ]
  ++ gpuiRuntimeLibraries;

  postInstall = ''
    bash packaging/linux/desktop-entry.sh ${assets}/numero.desktop \
      $out/share/applications/numero.desktop $out/bin/numero-app

    mkdir -p $out/share/icons
    cp -r ${assets}/icons/hicolor $out/share/icons/hicolor
    chmod -R u+w $out/share/icons

    wrapProgram $out/bin/numero-app \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath gpuiRuntimeLibraries}
  '';

  doInstallCheck = true;

  installCheckPhase = ''
    runHook preInstallCheck

    test -x $out/bin/numero-app
    test -x $out/bin/numero
    echo '1 + 1' | $out/bin/numero --no-extensions

    desktop-file-validate $out/share/applications/numero.desktop
    grep -qx "Exec=$out/bin/numero-app %U" $out/share/applications/numero.desktop
    grep -qx 'MimeType=x-scheme-handler/numero;text/plain;' \
      $out/share/applications/numero.desktop
    test -f $out/share/icons/hicolor/256x256/apps/numero.png
    test -f $out/share/icons/hicolor/symbolic/apps/numero-symbolic.svg

    runHook postInstallCheck
  '';

  meta = {
    description = "A Numi-style text calculator in Rust and GPUI";
    platforms = lib.platforms.linux;
    mainProgram = "numero-app";
  };
}
