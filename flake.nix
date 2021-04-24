{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    naersk = {
      url = "github:yusdacra/naersk/feat/cargolock-git-deps";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rustOverlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixCargoIntegration = {
      url = "github:yusdacra/nix-cargo-integration";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.naersk.follows = "naersk";
      inputs.rustOverlay.follows = "rustOverlay";
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
      build = common: prevb: {
        gitSubmodules = true;
        allRefs = true;
        overrideMain = prev:
          let o = prevb.overrideMain prev; in
          o // {
            nativeBuildInputs = o.nativeBuildInputs ++ (with common.pkgs; [ makeWrapper wrapGAppsHook rustfmt ]);
            fixupPhase = with common.pkgs; ''
              wrapProgram $out/bin/crust\
                --set LD_LIBRARY_PATH ${lib.makeLibraryPath common.runtimeLibs}\
                --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
            '';
          };
      };
    };
  };
}
