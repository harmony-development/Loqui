{
  inputs = {
    flakeCompat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nixCargoIntegration = {
      url = "github:yusdacra/nix-cargo-integration";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs: inputs.nixCargoIntegration.lib.makeOutputs {
    root = ./.;
    overrides = {
      shell = common: prev: {
        env = prev.env ++ [
          {
            name = "XDG_DATA_DIRS";
            eval = with common.pkgs; "$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share";
          }
        ];
      };
      mainBuild = common: prev: {
        nativeBuildInputs = prev.nativeBuildInputs ++ (with common.pkgs; [ makeWrapper wrapGAppsHook ]);
        postInstall = with common.pkgs; ''
          if [ -f $out/bin/crust ]; then
            wrapProgram $out/bin/crust\
              --set LD_LIBRARY_PATH ${lib.makeLibraryPath common.runtimeLibs}\
              --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
          fi
        '';
      };
    };
  };
}
