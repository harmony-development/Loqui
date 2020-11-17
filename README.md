`icy_matrix` is a Matrix client written in Rust using the `iced` GUI library. It uses `ruma` and `ruma-client` to interact with the Matrix network.

It aims to be lightweight with a good out-of-the-box experience and a small amount of customization. Currently very WIP

## Requirements
- Current stable Rust and Cargo.

## Building
- Make sure you have current stable Rust and Cargo.
- Clone the repo, and switch the working directory to it: `git clone https://github.com/yusdacra/icy_matrix.git && cd icy_matrix` 
- To build and run the project use `cargo run`.

## Current features
- Plain-text message and file sending
- Receiving messages and media, can show thumbnails (does not support location messages)
- Show state changes (kicks / bans, room title changes etc.)
- Change rooms (the ones you joined to)
- Remembers login

## Planned features (not ordered)
- Multiline message composer
- Play audio / video and show images in app 
- HTML rendering for messages (need widget for iced)
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

## Not planned features
- Video / audio calls
