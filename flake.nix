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

  outputs = inputs:
    let
      outputs = inputs.nixCargoIntegration.lib.makeOutputs {
        root = ./.;
        buildPlatform = "crate2nix";
        overrides = {
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
              if [ -f $out/bin/loqui ]; then
                wrapProgram $out/bin/loqui\
                  --set LD_LIBRARY_PATH ${lib.makeLibraryPath common.runtimeLibs}\
                  --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
              fi
            '';
          };
        };
      };
    in
    outputs // {
      apps = outputs.apps // {
        x86_64-linux = outputs.apps.x86_64-linux // {
          run-latest =
            let
              pkgs = import inputs.nixpkgs { system = "x86_64-linux"; config = { allowUnfree = true; }; };
              cmd =
                pkgs.writeScriptBin "run-loqui-latest" ''
                  #!${pkgs.stdenv.shell}
                  mkdir -p /tmp/loqui-binary
                  cd /tmp/loqui-binary
                  ${pkgs.curl}/bin/curl -L https://github.com/harmony-development/Loqui/releases/download/continuous/loqui-linux > loqui
                  chmod +x loqui
                  ${pkgs.steam-run}/bin/steam-run ./loqui
                '';
            in
            {
              type = "app";
              program = "${cmd}/bin/run-loqui-latest";
            };
        };
      };
    };
}
