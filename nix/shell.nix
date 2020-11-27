{ nixpkgs ? <nixpkgs>, sources ? import ./sources.nix { }
, system ? builtins.currentSystem }:
let
  common = import ./common.nix { inherit sources system; };
  inherit (common) pkgs;
  crate2nix = pkgs.callPackage sources.crate2nix { inherit pkgs; };
in with pkgs;
mkShell {
  name = "icy_matrix-dev-shell";
  nativeBuildInputs = [ git niv nixfmt crate2nix cargo rustc rustfmt ];
  buildInputs = (lib.concatLists (lib.attrValues common.crateDeps)) ++ [ gtk3 ];
  shellHook = ''
    export LD_LIBRARY_PATH=${common.neededLibPaths}
    export XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
  '';
}
