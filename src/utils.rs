use std::{
    borrow::Cow,
    cmp::Ordering,
    ops::Deref,
    sync::{atomic::AtomicBool, Arc},
};

use client::{
    guild::Guild,
    harmony_rust_sdk::api::{profile::UserStatus, rest::FileId},
    member::Member,
    tracing, Client,
};
use eframe::egui::{self, Align, Color32, Key, Layout, Response, RichText, Ui, Widget, WidgetText};

use crate::app::State;
pub(crate) use crate::futures::{handle_future, spawn_client_fut, spawn_evs, spawn_future};
pub use anyhow::{anyhow, bail, ensure, Error};
pub use client::error::{ClientError, ClientResult};
pub use guard::guard;

#[derive(Default, Clone)]
pub struct AtomBool {
    inner: Arc<AtomicBool>,
}

impl AtomBool {
    pub fn new(val: bool) -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(val)),
        }
    }

    pub fn get(&self) -> bool {
        self.inner.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set(&self, val: bool) {
        self.inner.store(val, std::sync::atomic::Ordering::Relaxed);
    }
}

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

pub fn sort_members<'a, 'b>(state: &'a State, guild: &'b Guild) -> Vec<(&'b u64, &'a Member)> {
    let mut sorted_members = guild
        .members
        .keys()
        .flat_map(|id| state.cache.get_user(*id).map(|m| (id, m)))
        .collect::<Vec<_>>();
    sorted_members.sort_unstable_by(|(_, member), (_, other_member)| {
        let name = member.username.as_str().cmp(other_member.username.as_str());
        let offline = matches!(member.status, UserStatus::OfflineUnspecified);
        let other_offline = matches!(other_member.status, UserStatus::OfflineUnspecified);

        match (offline, other_offline) {
            (false, true) => Ordering::Less,
            (true, false) => Ordering::Greater,
            _ => name,
        }
    });
    sorted_members
}

// scale down resolution while preserving ratio
pub fn scale_down(w: f32, h: f32, max_size: f32) -> (f32, f32) {
    let ratio = w / h;
    let new_w = max_size;
    let new_h = max_size / ratio;
    (new_w, new_h)
}

pub trait ResponseExt {
    fn did_submit(&self, ui: &Ui) -> bool;
    fn on_hover_text_at_pointer(self, text: &str) -> Self;
}

impl ResponseExt for Response {
    fn did_submit(&self, ui: &Ui) -> bool {
        self.lost_focus() && ui.input().key_pressed(Key::Enter)
    }
    fn on_hover_text_at_pointer(self, text: &str) -> Self {
        self.on_hover_ui_at_pointer(|ui| {
            ui.label(text);
        })
    }
}

pub trait UiExt {
    fn text_button(&mut self, text: &str) -> Response;
    fn animate_bool_with_time_alternate(&mut self, id: &str, b: &mut bool, time: f32) -> f32;
    fn add_hovered<W: Widget>(&mut self, widget: W) -> Response;
}

impl UiExt for Ui {
    fn text_button(&mut self, text: &str) -> Response {
        self.add(egui::Button::new(text).frame(false))
    }

    fn animate_bool_with_time_alternate(&mut self, id: &str, b: &mut bool, time: f32) -> f32 {
        let anim_val = self.ctx().animate_bool_with_time(egui::Id::new(id), *b, time);
        if anim_val == 1.0 {
            *b = false;
        } else if anim_val == 0.0 {
            *b = true;
        }
        anim_val
    }

    fn add_hovered<W: Widget>(&mut self, widget: W) -> Response {
        self.add_visible(self.ui_contains_pointer(), widget)
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

pub fn dangerous_text(text: impl Into<String>) -> RichText {
    RichText::new(text).color(Color32::RED)
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
