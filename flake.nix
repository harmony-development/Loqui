{
  description = "Flake for icy_matrix, a Matrix client written in Rust";

  inputs = {
    crate2nix = {
      url = "github:kolloch/crate2nix";
      flake = false;
    };
    flakeUtils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs";
  };

  outputs = { self, crate2nix, flakeUtils, nixpkgs }:
    with flakeUtils.lib;
    eachSystem [ "x86_64-linux" ] (system:
      let
        common = import ./nix/common.nix { inherit crate2nix nixpkgs system; };

        icy_matrix = import ./nix/build.nix { inherit common; };
        icy_matrix-app = mkApp { drv = icy_matrix; };
      in
      rec {
        packages = {
          inherit icy_matrix;
        };
        defaultPackage = packages.icy_matrix;

        apps = { icy_matrix = icy_matrix-app; };
        defaultApp = apps.icy_matrix;

        devShell = (import ./nix/devShell.nix) common;
      }
    );
}
