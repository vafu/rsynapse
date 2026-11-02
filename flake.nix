{
  description = "Rsynapse development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          nativePackages = with pkgs; [ pkg-config gtk4 libadwaita gtk4-layer-shell gtksourceview5 ];
        in {
          # Use an already installed Rust toolchain with the pinned native GTK
          # dependencies, without downloading a second compiler and source tree.
          native = pkgs.mkShell { packages = nativePackages; };
          default = pkgs.mkShell {
            packages = nativePackages ++ (with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
            ]);
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        });
    };
}
