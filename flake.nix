{
  description = "Numero: a Numi-style text calculator for Linux and macOS, in Rust and GPUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
      toolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    in
    {
      # The macOS app is built with cargo on a Mac (`just bundle`), not here.
      packages = forAllSystems (
        pkgs:
        let
          toolchain = toolchainFor pkgs;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
        in
        nixpkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux rec {
          numero = pkgs.callPackage ./nix/package.nix { inherit rustPlatform; };
          default = numero;
        }
      );

      devShells = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/devshell.nix {
          rustToolchain = toolchainFor pkgs;
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
