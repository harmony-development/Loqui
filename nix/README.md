### How to use

To build and install icy_matrix into user profile, run:
```shell
nix-env -f nix/default.nix -i
```

To enter the development shell (which includes all tools mentioned in this readme + tools you'll need to develop icy_matrix), run:
```shell
nix-shell nix/shell.nix
```

If you have [direnv](https://direnv.net), copy `nix/envrc` file to repository root as `.envrc` to get your dev env automatically setup:
```shell
cp nix/envrc .envrc
```

### Managing Cargo.nix

Enter the development shell, switch your working directory to `nix`.

To update `Cargo.nix` (and `crate-hashes.json`) using latest `Cargo.lock`, run:
```shell
crate2nix generate -f ../Cargo.toml
```

### Managing dependencies

We use [niv](https://github.com/nmattia/niv) to manage dependencies.

To update the dependencies, run (from repository root):
```shell
niv update
```

### Formatting

Use [nixfmt](https://github.com/serokell/nixfmt) to format files.

To format every Nix file in current working directory:
```shell
nixfmt *.nix
```
