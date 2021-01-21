{ common }:
with common; with pkgs;
mkShell {
  name = "crust-devShell";
  nativeBuildInputs =
    [ git nixpkgs-fmt rustc cachix ]
    ++ crateDeps.nativeBuildInputs;
  buildInputs = crateDeps.buildInputs;
  shellHook =
    let
      varList = lib.mapAttrsToList (name: value: ''export ${name}="${value}"'') env;
      varConcatenated = lib.concatStringsSep "\n" varList;
    in
    ''
      export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${lib.makeLibraryPath neededLibs}"
      export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share"

      ${varConcatenated}
    '';
}
