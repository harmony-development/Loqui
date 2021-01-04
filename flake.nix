{
  description = "Flake for icy_matrix, a Matrix client written in Rust";

  inputs = rec {
    naersk = {
      url = "github:yusdacra/naersk/extract-rev-cargolock";
      inputs.nixpkgs = nixpkgs;
    };
    flakeUtils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs";
    nixpkgsMoz = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = { self, naersk, flakeUtils, nixpkgs, nixpkgsMoz }:
    with flakeUtils.lib;
    eachSystem [ "x86_64-linux" ] (system:
      let
        common = import ./nix/common.nix {
          sources = { inherit naersk nixpkgs nixpkgsMoz; };
          inherit system;
        };
      in
      rec {
        packages = {
          icy_matrix = import ./nix/build.nix { inherit common; };
        };
        defaultPackage = packages.icy_matrix;

        apps = builtins.mapAttrs (n: v: mkApp { name = n; drv = v; }) packages;
        defaultApp = apps.icy_matrix;

        devShell = (import ./nix/devShell.nix) common;
      }
    );
}
