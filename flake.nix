{
  inputs = {
    utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };
  outputs =
    inputs: inputs.utils.lib.eachDefaultSystem (
      system: let
        pkgs = inputs.nixpkgs.legacyPackages.${system}.extend inputs.rust-overlay.overlays.default;
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in {
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rust
          ] ++ lib.optionals pkgs.stdenv.isLinux [
            pkg-config
          ];

          buildInputs = with pkgs; [] ++ lib.optionals pkgs.stdenv.isLinux [
            alsa-lib
          ];
        };
      }
    );
}
