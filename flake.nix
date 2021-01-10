{
  description = "Flake for icy_matrix, a Matrix client written in Rust";

  inputs = rec {
    naersk = {
      url = "github:yusdacra/naersk/extract-rev-cargolock";
      inputs.nixpkgs = nixpkgs;
    };
    flakeUtils.url = "github:numtide/flake-utils";
    nixpkgs.url = "nixpkgs/nixpkgs-unstable";
    nixpkgsMoz = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = inputs: with inputs; with flakeUtils.lib;
    eachSystem [ "x86_64-linux" ] (system:
      let
        common = import ./nix/common.nix {
          sources = { inherit naersk nixpkgs nixpkgsMoz; };
          inherit system;
        };

        packages = {
          icy_matrix = import ./nix/build.nix { inherit common; release = true; };
          icy_matrix-debug = import ./nix/build.nix { inherit common; };
        };
        apps = builtins.mapAttrs (n: v: mkApp { name = n; drv = v; exePath = "/bin/icy_matrix"; }) packages;
      in
      {
        inherit packages apps;

        defaultPackage = packages.icy_matrix-debug;

        defaultApp = apps.icy_matrix-debug;

        devShell = import ./nix/devShell.nix { inherit common; };
      }
    );
}
