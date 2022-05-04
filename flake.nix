{
  inputs = {
    flakeCompat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nci = {
      url = "github:yusdacra/nix-cargo-integration";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    /*
       androidPkgs = {
       url = "github:tadfisher/android-nixpkgs";
       inputs.nixpkgs.follows = "nixpkgs";
     };
     */
  };

  outputs = {nci, ...} @ inputs: let
    outputs = nci.lib.makeOutputs {
      root = ./.;
      overrides = {
        pkgsOverlays = [
          (_: prev: {
            android-sdk = inputs.androidPkgs.sdk.${prev.system} (sdkPkgs:
              with sdkPkgs; [
                cmdline-tools-latest
                build-tools-32-0-0
                platform-tools
                platforms-android-32
                emulator
                ndk-bundle
              ]);
          })
        ];
        shell = common: prev:
          with common.pkgs; {
            packages =
              prev.packages
              ++ [
                (common.internal.nci-pkgs.utils.buildCrate {
                  pname = "trunk";
                  source = builtins.fetchGit {
                    url = "https://github.com/thedodd/trunk.git";
                    rev = "5c799dc35f1f1d8f8d3d30c8723cbb761a9b6a08";
                    shallow = true;
                  };
                  packageOverrides.trunk.disable-test = {
                    doCheck = false;
                  };
                })
              ];
            env =
              prev.env
              ++ [
                {
                  name = "XDG_DATA_DIRS";
                  eval = "$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share";
                }
                /*
                   {
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
                 }
                 */
              ];
            commands =
              prev.commands
              ++ [
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
    outputs
    // {
      apps =
        outputs.apps
        // {
          x86_64-linux =
            outputs.apps.x86_64-linux
            // {
              run-latest = let
                pkgs = import inputs.nixpkgs {
                  system = "x86_64-linux";
                  config = {allowUnfree = true;};
                };
                cmd = pkgs.writeScriptBin "run-loqui-latest" ''
                  #!${pkgs.stdenv.shell}
                  mkdir -p /tmp/loqui-binary
                  cd /tmp/loqui-binary
                  ${pkgs.curl}/bin/curl -L https://github.com/harmony-development/Loqui/releases/download/continuous/loqui-linux > loqui
                  chmod +x loqui
                  ${pkgs.steam-run}/bin/steam-run ./loqui
                '';
              in {
                type = "app";
                program = "${cmd}/bin/run-loqui-latest";
              };
            };
        };
    };
}
