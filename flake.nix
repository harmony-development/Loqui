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
                rev = "f127281d050d25032b44790a865b8a128a8145e8";
                fetchSubmodules = true;
                sha256 = "sha256-3cYE9/hZtnbCx4Y4I1hbGhUtFRjB/X+uUiJZjxA6Qw4=";
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
            value = "-Clink-arg=-fuse-ld=${common.pkgs.mold}/bin/mold -Cprefer-dynamic=yes";
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
