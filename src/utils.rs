use std::borrow::Cow;

use eframe::egui::{self, Color32, Key, Response, Ui};

pub(crate) use crate::futures::{handle_future, spawn_evs, spawn_future};
pub use anyhow::{anyhow, bail, ensure, Error};
pub use client::error::{ClientError, ClientResult};
pub use guard::guard;

#[allow(dead_code)]
pub fn truncate_string(value: &str, new_len: usize) -> Cow<'_, str> {
    if value.chars().count() > new_len {
        let mut value = value.to_string();
        value.truncate(value.chars().take(new_len).map(char::len_utf8).sum());
        value.push('…');
        Cow::Owned(value)
    } else {
        Cow::Borrowed(value)
    }
}

// scale down resolution while preserving ratio
pub fn scale_down(w: f32, h: f32, max_size: f32) -> (f32, f32) {
    let ratio = w / h;
    let new_w = max_size;
    let new_h = max_size / ratio;
    (new_w, new_h)
}

pub trait TextInputExt {
    fn did_submit(&self, ui: &Ui) -> bool;
}

impl TextInputExt for Response {
    fn did_submit(&self, ui: &Ui) -> bool {
        self.lost_focus() && ui.input().key_pressed(Key::Enter)
    }
}

pub trait UiExt {
    fn text_button(&mut self, text: &str) -> Response;
}

impl UiExt for Ui {
    fn text_button(&mut self, text: &str) -> Response {
        self.add(egui::Button::new(text).frame(false))
    }
}

pub fn rgb_color(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}
