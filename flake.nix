{
  description = "Flake for crust, a Harmony client written in Rust";

  inputs = {
    naersk = {
      url = "github:nmattia/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flakeUtils.url = "github:numtide/flake-utils";
    nixpkgs.url = "nixpkgs/nixpkgs-unstable";
    rustOverlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs: with inputs; with flakeUtils.lib;
    eachSystem [ "x86_64-linux" ] (system:
      let
        common = import ./nix/common.nix {
          sources = { inherit naersk nixpkgs rustOverlay; };
          inherit system;
        };

        packages = {
          crust = import ./nix/build.nix { inherit common; release = true; };
          crust-debug = import ./nix/build.nix { inherit common; };
        };
        apps = builtins.mapAttrs (n: v: mkApp { name = n; drv = v; exePath = "/bin/crust"; }) packages;
      in
      {
        inherit packages apps;

        defaultPackage = packages.crust;

        defaultApp = apps.crust;

        devShell = import ./nix/devShell.nix { inherit common; };
      }
    );
}
