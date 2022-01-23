# Architecture

This file outlines the high-level architecture of `loqui`.

Since `loqui` is an `egui` application, it is recommended to get familiar with
`egui` first before delving in the codebase.

## `client`

This crate handles all communication with Harmony servers. This is where all
endpoint methods and socket event handlers should go.

## `src/app`

This module contains the main app loop and app initialization for loqui.

It also implements "general" UI seperate from screens, such as the status bar
and the global menu.

## `src/state`

This module contains the monolithic state structure. It does everything ranging
from managing sockets, handling events and storing important data. Since it is
kept over the course of the lifetime of the app, this is where anything that should
live beyond a "screen" should go.

## `src/screen`

This module contains the "screen"s for loqui, aka the main content we will be
displaying at once. Any major UI should go here, more minor UIs can be made a
window.

## `src/config`

This module contains configuration structures for loqui. This is where
all configuration related stuff should go (new config options, config parsing,
anyting configuration related).

## `src/widgets`

This module contains widgets used by loqui.