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
        overrides = {
          pkgs = common: prev: {
            overlays = prev.overlays ++ [
              (final: prev: {
                mold = prev.stdenv.mkDerivation {
                  pname = "mold";
                  version = "master";

                  stdenv = prev.llvmPackages_12.stdenv;

                  dontUseCmakeConfigure = true;

                  buildInputs = with prev; [ openssl.dev zlib.dev xxHash.dev tbb ];
                  nativeBuildInputs = [ prev.cmake prev.clang_12 ];

                  src = prev.fetchgit {
                    url = "https://github.com/rui314/mold.git";
                    rev = "72cea9a0bfcdee7cb17cc34bed9aacdea2f80adf";
                    fetchSubmodules = true;
                    sha256 = "sha256-ocug5DAPq7LU8HH6yHQI3FhW8XF4H31krmr6ttJ9V9k=";
                  };

                  buildPhase = "make";
                  installPhase = ''
                    mkdir -p $out/bin
                    install -m 755 mold-wrapper.so mold $out/bin
                  '';
                };
              })
            ];
          };
          shell = common: prev: {
            packages = prev.packages ++ [ common.pkgs.mold ];
            env = prev.env ++ [
              {
                name = "XDG_DATA_DIRS";
                eval = with common.pkgs; "$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS:${hicolor-icon-theme}/share:${gnome3.adwaita-icon-theme}/share";
              }
              {
                name = "RUSTFLAGS";
                value = "-Clink-arg=-fuse-ld=${common.pkgs.mold}/bin/mold";
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
