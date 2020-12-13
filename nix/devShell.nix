common:
with common; with pkgs;
let
  getCratesDeps = name:
    lib.concatLists (map (attr: attr."${name}") (lib.attrValues crateDeps));
in
mkShell {
  name = "icy_matrix-dev-shell";
  nativeBuildInputs =
    [ git nixpkgs-fmt crate2nix cargo clippy rustc rustfmt ]
    ++ getCratesDeps "nativeBuildInputs";
  buildInputs = getCratesDeps "buildInputs";
  shellHook = ''
    export LD_LIBRARY_PATH=${lib.makeLibraryPath neededLibs}
    export XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
  '';
}
