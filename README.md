`rucies` is a Harmony client written in Rust using the `iced` GUI library.

It aims to be lightweight with a good out-of-the-box experience. Currently WIP

![rucies](resources/screenshot.png)

## Requirements
- Current stable Rust and Cargo.
- Make sure you have a working Vulkan setup.
- gcc, python3, pkg-config, cmake; protobuf, protoc, openssl, x11, xcb, freetype, fontconfig, expat, glib, gtk3, cairo, pango, atk, gdk_pixbuf libraries and development files.
- Above list may be incomplete, please find out what you need by looking at compiler errors.

### Nix
- `nix develop` to get a dev shell. (or `nix-shell nix/shell.nix` if you don't have flakes enabled)

## Building

- Clone the repo, and switch the working directory to it: `git clone https://github.com/harmony-development/rucies.git && cd rucies`
- To build and run the project with debug info / checks use `cargo run`. Use `cargo run --release` for an optimized release build.

### Nix
- `nix build .#rucies-debug` to compile a debug build.
- `nix build .#rucies` to compile a release build.
- If you don't have flakes enabled, `nix-build` will give you a release build.

## Installing

### Nix
- For flakes: `nix profile install github:harmony-development/rucies`
