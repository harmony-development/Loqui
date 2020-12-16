`icy_matrix` is a Matrix client written in Rust using the `iced` GUI library. It uses `ruma` and `ruma-client` to interact with the Matrix network.

It aims to be lightweight with a good out-of-the-box experience. Currently WIP

## Requirements
- If you have the Nix package manager:
    - `nix develop` to get a dev shell. (or `nix-shell nix/shell.nix` if you don't have flakes enabled)
- If not:
    - Current stable Rust and Cargo.
    - Make sure you have a working Vulkan setup.
    - gcc, python3, pkg-config, cmake; openssl, x11, xcb, freetype, fontconfig, expat, glib, gtk3, cairo, pango, atk, gdk_pixbuf libraries and development files.
    - Above list may be incomplete, please find out what you need by looking at compiler errors.

## Building
- Clone the repo, and switch the working directory to it: `git clone https://gitlab.com/yusdacra/icy_matrix.git && cd icy_matrix`
- To build and run the project with debug info / checks use `cargo run`. Use `cargo run --release` for an optimized release build.

## Current features
- Plain-text message and file sending
- Receiving messages and media, can show thumbnails (does not support location messages)
- Show state changes (kicks / bans, room title changes etc.)
- Change rooms (the ones you joined to)
- Room search (powered by [`fuzzy-matcher`](https://lib.rs/crates/fuzzy-matcher) using the `SkimMatcherV2` implementation)
- Remembers login

## Planned features (not ordered)
- Multiline message composer
- Play audio / video and show images in app
- HTML tag rendering of messages (need widget for iced)
- Embedding URLs (pictures / video thumbnails)
- Read markers
- Settings screen
- User list for rooms
- User presence
- Showing invites, leaving rooms 
- Public room explorer
- Room settings
- Moderator actions (kick, ban, delete message etc.)
- Encryption (via [pantalaimon](https://github.com/matrix-org/pantalaimon)?)
- Animations for better UX (whenever iced supports this)
- Custom emotes (depends on HTML rendering of messages)

## Not planned features
- Video / audio calls
