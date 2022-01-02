use std::{borrow::Cow, ops::Deref};

use client::{harmony_rust_sdk::api::rest::FileId, tracing, Client};
use eframe::egui::{self, Align, Color32, Key, Layout, Response, Ui};

pub(crate) use crate::futures::{handle_future, spawn_client_fut, spawn_evs, spawn_future};
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

pub fn horizontal_centered_justified() -> Layout {
    Layout::left_to_right()
        .with_cross_align(Align::Center)
        .with_cross_justify(true)
}

pub fn make_url_from_file_id(client: &Client, id: &FileId) -> String {
    match id {
        FileId::Hmc(hmc) => format!(
            "https://{}:{}/_harmony/media/download/{}",
            hmc.server(),
            hmc.port(),
            hmc.id(),
        ),
        FileId::Id(id) => {
            let homeserver = client.inner().homeserver_url();
            format!("{}_harmony/media/download/{}", homeserver, id)
        }
        FileId::External(ext) => {
            let homeserver = client.inner().homeserver_url();
            format!(
                "{}_harmony/media/download/{}",
                homeserver,
                urlencoding::encode(&ext.to_string())
            )
        }
    }
}

// opens a URL in background
pub fn open_url(url: impl Deref<Target = str> + Send + 'static) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::spawn(move || {
            let url = url.deref();

            if let Err(err) = open::that(url) {
                tracing::error!("error opening URL, falling back to browser: {}", err);
                if let Err(err) = webbrowser::open(url) {
                    tracing::error!("error opening URL: {}", err);
                }
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        let url = url.deref();
        if let Err(err) = webbrowser::open(url) {
            tracing::error!("error opening URL: {}", err);
        }
    }
}
