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
    /*androidPkgs = {
      url = "github:tadfisher/android-nixpkgs";
      inputs.nixpkgs.follows = "nixpkgs";
    };*/
  };

  outputs = { nixCargoIntegration, ... }@inputs:
    let
      outputs = nixCargoIntegration.lib.makeOutputs {
        root = ./.;
        overrides = {
          pkgs = common: prev: {
            overlays = prev.overlays ++ [
              (_: prev: {
                android-sdk = inputs.androidPkgs.sdk.${prev.system} (sdkPkgs: with sdkPkgs; [
                  cmdline-tools-latest
                  build-tools-32-0-0
                  platform-tools
                  platforms-android-32
                  emulator
                  ndk-bundle
                ]);
              })
            ];
          };
          crateOverrides = common: _: {
            loqui = prev: {
              nativeBuildInputs = (prev.nativeBuildInputs or [ ]) ++ (with common.pkgs; [ makeWrapper wrapGAppsHook ]);
              postInstall = with common.pkgs; ''
                if [ -f $out/bin/loqui ]; then
                  wrapProgram $out/bin/loqui\
                    --set LD_LIBRARY_PATH ${lib.makeLibraryPath common.runtimeLibs}\
                    --set XDG_DATA_DIRS ${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share
                fi
              '';
            };
          };
          shell = common: prev: with common.pkgs; {
            env = prev.env ++ [
              {
                name = "XDG_DATA_DIRS";
                eval = "$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share";
              }
              /*{
                name = "ANDROID_HOME";
                value = "${android-sdk}/share/android-sdk";
              }
              {
                name = "ANDROID_SDK_ROOT";
                value = "${android-sdk}/share/android-sdk";
              }
              {
                name = "JAVA_HOME";
                value = jdk11.home;
              }*/
            ];
            commands = prev.commands ++ [
              {
                name = "local-dev";
                command = "SSL_CERT_FILE=~/.local/share/mkcert/rootCA.pem cargo r";
              }
              {
                name = "cargo-mobile";
                help = "Build for mobile.";
                command = "$HOME/.cargo/bin/cargo-mobile $@";
              }
            ];
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
