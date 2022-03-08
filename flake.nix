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

  outputs = inputs:
    let
      outputs = inputs.nixCargoIntegration.lib.makeOutputs {
        root = ./.;
        buildPlatform = "crate2nix";
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
              (_: prev: {
                trunk = prev.nciUtils.buildCrate {
                  root = builtins.fetchGit {
                    url = "https://github.com/thedodd/trunk.git";
                    ref = "master";
                    rev = "b989bc9bfd568bc3b3bba7ac804f797a41f12a82";
                  };
                  release = true;
                };
                twiggy = prev.nciUtils.buildCrate {
                  root = builtins.fetchGit {
                    url = "https://github.com/rustwasm/twiggy.git";
                    ref = "master";
                    rev = "195feee4045f0b89d7cba7492900131ac89803dd";
                  };
                  memberName = "twiggy";
                  release = true;
                };
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
            #packages = [ android-sdk ];
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
                help = "Build for the web.";
                package = trunk;
              }
              /*{
                help = "Profile binary size.";
                package = twiggy;
              }*/
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
