{ sources ? import ./sources.nix { }
, nixpkgs ? sources.nixpkgs
, system ? builtins.currentSystem
}:
with (import ./common.nix { inherit sources system nixpkgs; });
with pkgs;
mkShell {
  name = "icy_matrix-dev-shell";
  nativeBuildInputs =
    ([ git niv nixpkgs-fmt crate2nix cargo clippy rustc rustfmt ])
    ++ crateNativeBuildInputs;
  buildInputs = crateBuildInputs;
  shellHook = ''
    export LD_LIBRARY_PATH=${lib.makeLibraryPath neededLibs}
    export XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
  '';
}
