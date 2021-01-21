{
  description = "Flake for rucies, a Harmony client written in Rust";

  inputs = {
    naersk = {
      url = "github:nmattia/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
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
          rucies = import ./nix/build.nix { inherit common; release = true; };
          rucies-debug = import ./nix/build.nix { inherit common; };
        };
        apps = builtins.mapAttrs (n: v: mkApp { name = n; drv = v; exePath = "/bin/rucies"; }) packages;
      in
      {
        inherit packages apps;

        defaultPackage = packages.rucies;

        defaultApp = apps.rucies;

        devShell = import ./nix/devShell.nix { inherit common; };
      }
    );
}
