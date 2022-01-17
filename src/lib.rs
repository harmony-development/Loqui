#![feature(let_else, once_cell)]
#![deny(unsafe_code)]

pub(crate) mod app;
pub(crate) mod futures;
pub(crate) mod image_cache;
pub(crate) mod screen;
pub(crate) mod style;
pub(crate) mod utils;
pub(crate) mod widgets;
pub use app::App;
