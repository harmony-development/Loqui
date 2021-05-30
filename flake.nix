{
  inputs = {
    flakeCompat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixCargoIntegration = {
      url = "github:yusdacra/nix-cargo-integration";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs: inputs.nixCargoIntegration.lib.makeOutputs {
    root = ./.;
    overrides = {
      pkgs = common: prev: {
        overlays = [
          (final: prev: {
            llvmPackages_12 = prev.llvmPackages_12 // {
              clang = prev.lib.hiPrio prev.llvmPackages_12.clang;
              bintools = prev.lib.setPrio (-20) prev.llvmPackages_12.bintools;
            };
          })
        ] ++ prev.overlays;
      };
      shell = common: prev: {
        env = prev.env ++ [
          {
            name = "XDG_DATA_DIRS";
            eval = with common.pkgs; "$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share";
          }
        ];
        commands = prev.commands ++ [
          {
            name = "local-dev";
            command = "SSL_CERT_FILE=~/.local/share/mkcert/rootCA.pem cargo r";
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
