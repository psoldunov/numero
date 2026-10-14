# Installing Numero

Numero builds from source on Linux and macOS. There are no prebuilt releases.

It installs two programs: `numero-app`, the GPUI window, and `numero`, a terminal calculator that reads a file or stdin.

## Linux with Nix

The flake has a `numero` package (also `default`) for `x86_64-linux` and `aarch64-linux`. It holds both programs, the launcher entry `dev.soldunov.numero.desktop` and the icons. `numero-app` is wrapped so GPUI finds Vulkan, Wayland, X11 and xkbcommon at runtime.

Try it without installing:

```sh
nix run github:psoldunov/numero
```

Install it into your profile:

```sh
nix profile install github:psoldunov/numero
```

From a checkout, use `.` instead of the GitHub reference: `nix run .` and `nix profile install .`. The first build compiles the whole workspace and takes a few minutes.

### NixOS and home-manager

Add the flake as an input, then put the package in the system or user package list:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    numero.url = "github:psoldunov/numero";
    numero.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, numero, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ({ pkgs, ... }: {
          environment.systemPackages = [ numero.packages.${pkgs.system}.default ];
        })
      ];
    };
  };
}
```

With home-manager, use `home.packages = [ numero.packages.${pkgs.system}.default ];` instead.

The launcher entry names `numero-app` by its store path, so the app starts from the menu whatever `PATH` the launcher has.

## Linux without Nix

You need Rust 1.99 (`rust-toolchain.toml` pins it, and rustup reads it), a C compiler, `pkg-config`, and the development packages for fontconfig and freetype. At runtime GPUI loads Vulkan, Wayland or X11, and xkbcommon libraries, so install your distribution's packages for those too.

```sh
just install-linux
```

This builds with `cargo build --release` and copies:

| What | Where |
| --- | --- |
| `numero-app`, `numero` | `~/.local/bin` |
| `dev.soldunov.numero.desktop` | `$XDG_DATA_HOME/applications` (default `~/.local/share/applications`) |
| Icons, 16 to 512 px and a symbolic one | `$XDG_DATA_HOME/icons/hicolor` |

It then runs `update-desktop-database` and `gtk-update-icon-cache -f -t` if they are installed, and registers Numero as the handler for `numero://` links with `xdg-mime default dev.soldunov.numero.desktop x-scheme-handler/numero`. If `~/.local/bin` is not on your `PATH`, the script says so. The launcher entry uses the full path to `numero-app`, so the menu works either way.

To remove it:

```sh
just uninstall-linux
```

That deletes the files above and refreshes the caches. It leaves the `x-scheme-handler/numero` line in `~/.config/mimeapps.list`, which is harmless without the desktop file.

## macOS

macOS builds on the Mac itself. You need Xcode's command line tools and either Nix (the repository's dev shell sets up Rust) or a Rust 1.99 toolchain.

```sh
nix develop -c just bundle
```

Without Nix, run `just bundle` with `cargo` on your `PATH`.

This release-builds `numero-app` and assembles `target/bundle/Numero.app`:

```
Numero.app/Contents/Info.plist
Numero.app/Contents/MacOS/numero-app
Numero.app/Contents/Resources/numero.icns
```

The bundle id is `dev.soldunov.numero`. The minimum macOS version is 13.0. The `Info.plist` registers the `numero://` scheme and `.numi` and `.txt` files.

The app is signed ad hoc (`codesign -s -`). It runs on the Mac that built it. Gatekeeper blocks it on other Macs, and macOS asks again for permissions after each rebuild, because an ad hoc signature changes every time. Signing with a Developer ID and notarizing are not set up.

Nothing installs for you. Copy it where you want it:

```sh
cp -R target/bundle/Numero.app /Applications/
```

From a Linux machine, `just mac-bundle` copies the tree to the Mac named by `NUMERO_MAC_HOST` (default `bigmac`) and runs `just bundle` there over SSH, leaving the app in `/tmp/numero-pkg/target/bundle/Numero.app`.

To remove it, quit Numero and delete `Numero.app`.

## Your data

Uninstalling does not touch your calculations or settings. On Linux they live in `~/.local/share/numero` (calculations), `~/.config` (settings) and `~/.cache/numero` (exchange rates), following the XDG variables. On macOS they live under `~/Library/Application Support/numero`, `~/Library/Preferences` and `~/Library/Caches/numero`. Delete them by hand to start clean.
