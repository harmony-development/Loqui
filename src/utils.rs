use std::{
    borrow::Cow,
    cmp::Ordering,
    future::Future,
    ops::{Deref, Not},
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
};

use client::{
    guild::Guild,
    harmony_rust_sdk::api::{
        chat::overrides::Reason,
        mediaproxy::fetch_link_metadata_response,
        profile::{profile_override, ProfileOverride, UserStatus},
        rest::FileId,
    },
    member::Member,
    message::{is_raster_image, Override},
    role::Role,
    tracing, Cache, Client, Uri,
};
use eframe::egui::{
    self, Color32, Context, Frame, Key, Pos2, Response, Rgba, RichText, TextureHandle, Ui, Vec2, Widget, WidgetText,
};

pub(crate) use crate::futures::{handle_future, spawn_client_fut, spawn_evs};
use crate::{state::State, style, widgets::TextButton};
pub use anyhow::{anyhow, bail, ensure, Error};
pub use client::error::{ClientError, ClientResult};

/// A wrapper around an `Arc<AtomicBool>`.
///
/// Mostly useful for keeping track of whether a future has finished or not.
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

    #[inline(always)]
    pub fn get(&self) -> bool {
        self.inner.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn set(&self, val: bool) {
        self.inner.store(val, std::sync::atomic::Ordering::Relaxed);
    }

    /// Runs a future under this atom's scope. Before running the future,
    /// it is set to `true`, after the future is finished it is set to `false`.
    pub async fn scope<Fut>(&self, fut: Fut) -> <Fut as Future>::Output
    where
        Fut: Future,
    {
        self.set(true);
        let res = fut.await;
        self.set(false);
        res
    }
}

pub trait RoleExt {
    fn color(&self) -> Color32;
}

impl RoleExt for Role {
    #[inline(always)]
    fn color(&self) -> Color32 {
        rgb_color(self.color)
    }
}

pub trait VecExt<T> {
    fn remove_item(&mut self, item: &T) -> Option<T>;
}

impl<T: PartialEq> VecExt<T> for Vec<T> {
    fn remove_item(&mut self, item: &T) -> Option<T> {
        self.iter().position(|i| i == item).map(|pos| self.remove(pos))
    }
}

pub trait ClientExt {
    /// Returns the current logged-in user.
    fn this_user<'a>(&self, cache: &'a Cache) -> Option<&'a Member>;
}

impl ClientExt for Client {
    fn this_user<'a>(&self, cache: &'a Cache) -> Option<&'a Member> {
        let user_id = self.user_id();
        cache.get_user(user_id)
    }
}

pub trait ResponseExt {
    /// Did the user submit the text input? Intended for sigleline text inputs.
    fn did_submit(&self, ui: &Ui) -> bool;
    /// Shows some text at the pointer on hover.
    fn on_hover_text_at_pointer(self, text: &str) -> Self;
    /// Shows a stylized context menu. This should be used instead of the
    /// normal `context_menu` function, because it will style it correctly.
    fn context_menu_styled(self, add_contents: impl FnOnce(&mut Ui)) -> Self;
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

    fn context_menu_styled(self, add_contents: impl FnOnce(&mut Ui)) -> Self {
        self.context_menu(move |ui| {
            ui.style_mut().visuals.widgets.hovered.fg_stroke.color = style::HARMONY_LOTUS_ORANGE;
            //ui.style_mut().visuals.widgets.hovered.bg_fill = Color32::TRANSPARENT;
            add_contents(ui);
        })
    }
}

pub trait UiExt {
    /// Shows a text button
    fn text_button(&mut self, text: impl Into<WidgetText>) -> Response;
    /// Animates via a tracking bool and returns the anim value. Automatically
    /// resets the animation if the anim value reaches 0.0 / 1.0
    ///
    /// mostly useful for animations that loop, eg. typing anim
    fn animate_bool_with_time_alternate(&mut self, id: &str, b: &mut bool, time: f32) -> f32;
    /// Adds a widget if the current ui is hovered.
    fn add_hovered(&mut self, widget: impl Widget) -> Response;
    /// Creates a group frame filled with the passed color.
    fn group_filled_with(&self, color: Color32) -> Frame;
    /// Creates a group frame filled with the window fill color.
    fn group_filled(&self) -> Frame;
    /// Fills all the available width, except the passed offset.
    fn offsetw(&mut self, offset: f32);
    /// Downscale some size to fit the available width
    fn downscale(&self, size: [f32; 2]) -> [f32; 2];
    /// Downscale some size to fit the available width
    fn downscale_to(&self, size: [f32; 2], factor: f32) -> [f32; 2];
    /// Add an image button without a frame
    fn frameless_image_button(&mut self, texture_id: egui::TextureId, size: impl Into<Vec2>) -> Response;
    fn role_color(&self, role: &Role) -> Color32;
}

impl UiExt for Ui {
    fn role_color(&self, role: &Role) -> Color32 {
        let color = Rgba::from(role.color());
        let bg_color = Rgba::from(self.visuals().window_fill());

        let colorb = color.intensity();
        let bg_colorb = bg_color.intensity();

        let new_color = if colorb < bg_colorb {
            Rgba::from_rgb(1.0 - color.r(), 1.0 - color.g(), 1.0 - color.b())
        } else {
            color
        };

        Color32::from(new_color)
    }

    #[inline(always)]
    fn text_button(&mut self, text: impl Into<WidgetText>) -> Response {
        self.add(TextButton::text(text))
    }

    fn frameless_image_button(&mut self, texture_id: egui::TextureId, size: impl Into<Vec2>) -> Response {
        self.add(egui::ImageButton::new(texture_id, size).frame(false))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
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

    #[inline(always)]
    fn add_hovered(&mut self, widget: impl Widget) -> Response {
        self.add_visible(self.ui_contains_pointer(), widget)
    }

    #[inline(always)]
    fn group_filled(&self) -> Frame {
        self.group_filled_with(self.style().visuals.window_fill())
    }

    #[inline(always)]
    fn group_filled_with(&self, color: Color32) -> Frame {
        egui::Frame::group(self.style()).fill(color)
    }

    fn offsetw(&mut self, offset: f32) {
        self.add_space(self.available_width() - offset);
    }

    fn downscale(&self, size: [f32; 2]) -> [f32; 2] {
        self.downscale_to(
            size,
            self.is_mobile().then(|| 0.95).unwrap_or_else(|| {
                let img_factor = size[0] / size[1];
                (img_factor > 1.3)
                    .then(|| 550.0 / self.input().screen_rect.width())
                    .unwrap_or_else(|| {
                        (img_factor < 0.7)
                            .then(|| 250.0 / self.input().screen_rect.width())
                            .unwrap_or_else(|| 400.0 / self.input().screen_rect.width())
                    })
            }),
        )
    }

    fn downscale_to(&self, size: [f32; 2], factor: f32) -> [f32; 2] {
        let available_width = self.available_width() * factor;
        let [w, h] = size;
        let max_size = (w < available_width).then(|| w).unwrap_or(available_width);
        let (w, h) = scale_down(w, h, max_size);
        [w as f32, h as f32]
    }
}

pub trait CtxExt {
    /// Returns the center of the available rect currently.
    fn available_center_pos(&self, offset_size: Vec2) -> Pos2;
    /// Are we on mobile or not?
    fn is_mobile(&self) -> bool;
}

impl CtxExt for Context {
    #[inline(always)]
    fn available_center_pos(&self, offset_size: Vec2) -> Pos2 {
        let center = self.available_rect().center();
        center - (offset_size * 0.5)
    }

    fn is_mobile(&self) -> bool {
        let input = self.input();
        // HACK: we should check whether or not we are on hidpi here...
        // otherwise `input.pixels_per_point() > 2.0` will break!
        input.screen_rect().aspect_ratio() < 1.1 || input.pixels_per_point() > 2.0
    }
}

impl CtxExt for Ui {
    #[inline(always)]
    fn available_center_pos(&self, offset_size: Vec2) -> Pos2 {
        self.ctx().available_center_pos(offset_size)
    }

    #[inline(always)]
    fn is_mobile(&self) -> bool {
        self.ctx().is_mobile()
    }
}

/// Truncate some string.
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

/// Sorts members by alphabet and offline / online.
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
    if new_w > w {
        (w, h)
    } else {
        (new_w, new_h)
    }
}

/// Converts u8 array to egui color.
#[inline(always)]
pub const fn rgb_color(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

/// Returns a text that represents a "dangerous" action (ie. red color).
#[inline(always)]
pub fn dangerous_text(text: impl Into<String>) -> RichText {
    RichText::new(text).color(Color32::RED)
}

/// Construct a URL from a harmony file ID.
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

/// Parse URLs from some text. Treats whitespace as seperators.
pub fn parse_urls<'a>(text: &'a str, state: &State) -> (Vec<(&'a str, Uri)>, bool) {
    text.split_whitespace()
        .fold((Vec::new(), true), |(mut tot, mut has_only_image), maybe_url| {
            let parsed = maybe_url
                .parse::<Uri>()
                .ok()
                .filter(|u| matches!(u.scheme_str(), Some("http" | "https")));
            if let Some(url) = parsed {
                let is_image = state.cache.get_link_data(&url).map_or(false, |d| {
                    if let fetch_link_metadata_response::Data::IsMedia(media) = d {
                        is_raster_image(&media.mimetype)
                    } else {
                        false
                    }
                });
                if is_image.not() {
                    has_only_image = false;
                }
                tot.push((maybe_url, url));
            } else {
                has_only_image = false;
            }
            (tot, has_only_image)
        })
}

pub fn load_harmony_lotus(ctx: &egui::Context) -> (TextureHandle, Vec2) {
    const HARMONY_LOTUS: &[u8] = include_bytes!("../data/lotus.png");
    let image = image::load_from_memory(HARMONY_LOTUS).expect("harmony lotus must be fine");
    let image = image.into_rgba8();
    let (w, h) = image.dimensions();
    let size = [w as usize, h as usize];
    let rgba = image.into_raw();
    let texid = ctx.load_texture(
        "harmony-lotus",
        egui::ImageData::Color(egui::ColorImage::from_rgba_unmultiplied(size, &rgba)),
    );
    (texid, [w as f32, h as f32].into())
}

/// Shorthand for generating a [`egui::Id`].
#[inline(always)]
pub fn id(source: impl std::hash::Hash) -> egui::Id {
    egui::Id::new(source)
}

pub fn show_notification(summary: impl AsRef<str>, body: impl AsRef<str>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut notif = notify_rust::Notification::new();
        notif.summary(summary.as_ref()).body(body.as_ref());

        #[cfg(not(target_os = "macos"))]
        notif
            .appname("loqui")
            .icon("loqui")
            .hint(notify_rust::Hint::Category("im".to_string()));

        if let Err(err) = notif.show() {
            tracing::error!("error while trying to send notification: {}", err);
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut options = web_sys::NotificationOptions::new();
        options.body(body.as_ref()).icon("loqui").require_interaction(false);
        let _ = web_sys::Notification::new_with_options(summary.as_ref(), &options);
    }
}

pub fn override_from_profile(p: &ProfileOverride) -> Override {
    Override {
        name: p.username.clone(),
        avatar_url: p.avatar.as_ref().and_then(|s| FileId::from_str(s).ok()),
        reason: p.reason.clone().map(|reason| match reason {
            profile_override::Reason::UserDefined(custom) => Reason::UserDefined(custom),
            profile_override::Reason::SystemPlurality(empty) => Reason::SystemPlurality(empty),
        }),
    }
}
