let
  flake = builtins.getFlake "github:nixos/nixpkgs/nixpkgs-unstable";
  pkgs = import flake {
    system = "x86_64-linux";
    overlays = [
      (final: prev: {
        libadwaita = prev.libadwaita.overrideAttrs (oldAttrs: {
          pname = "${oldAttrs.pname}-without-adwaita";
          doCheck = false;
          patches = (oldAttrs.patches or []) ++ [
            /home/vfuchedzhy/.config/home-manager/theming_patch.diff
          ];
        });
      })
    ];
  };
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
  ];

  buildInputs = with pkgs; [
    gtk4
    libadwaita
    gtk4-layer-shell
  ];
}
