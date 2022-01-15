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
    tracing, Client, Uri,
};
use eframe::egui::{self, Color32, Context, Frame, Key, Pos2, Response, RichText, Ui, Vec2, Widget, WidgetText};

pub(crate) use crate::futures::{handle_future, spawn_client_fut, spawn_evs, spawn_future};
use crate::{app::State, style, widgets::TextButton};
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

    #[inline(always)]
    pub fn get(&self) -> bool {
        self.inner.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[inline(always)]
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
    fn text_button(&mut self, text: impl Into<WidgetText>) -> Response;
    fn animate_bool_with_time_alternate(&mut self, id: &str, b: &mut bool, time: f32) -> f32;
    fn add_hovered(&mut self, widget: impl Widget) -> Response;
    fn group_filled_with(&self, color: Color32) -> Frame;
    fn group_filled(&self) -> Frame;
}

impl UiExt for Ui {
    #[inline(always)]
    fn text_button(&mut self, text: impl Into<WidgetText>) -> Response {
        self.add(TextButton::text(text))
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
}

pub trait CtxExt {
    fn available_center_pos(&self, offset_size: Vec2) -> Pos2;
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
        input.screen_rect().aspect_ratio() < 1.1 || input.pixels_per_point() > 2.0
    }
}

#[inline(always)]
pub fn rgb_color(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

#[inline(always)]
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

pub fn parse_urls(text: &str) -> impl Iterator<Item = (&str, Uri)> {
    text.split_whitespace()
        .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
        .filter_map(|maybe_url| Some((maybe_url, maybe_url.parse::<Uri>().ok()?)))
        .filter(|(_, url)| matches!(url.scheme_str(), Some("http" | "https")))
}

/// simple not thread safe object pooling
pub mod pool {
    use std::{
        cell::RefCell,
        collections::VecDeque,
        ops::{Deref, DerefMut},
        rc::Rc,
    };

    type Objects<T> = Rc<RefCell<VecDeque<T>>>;
    type Generator<T> = Rc<dyn Fn() -> T>;

    pub struct Pool<T> {
        objects: Objects<T>,
        generator: Generator<T>,
    }

    impl<T> Clone for Pool<T> {
        fn clone(&self) -> Self {
            Self {
                generator: self.generator.clone(),
                objects: self.objects.clone(),
            }
        }
    }

    impl<T> Pool<T> {
        #[inline(always)]
        fn new_internal(objects: Objects<T>, generator: Generator<T>) -> Self {
            Self { objects, generator }
        }

        pub fn new(generator: impl Fn() -> T + 'static) -> Self {
            Self::new_internal(Rc::new(RefCell::new(VecDeque::new())), Rc::new(generator))
        }

        pub fn new_with_count(generator: impl Fn() -> T + 'static, count: usize) -> Self {
            let initial_objects = std::iter::repeat_with(&generator).take(count).collect();

            let objects: Objects<T> = Rc::new(RefCell::new(initial_objects));
            let generator: Generator<T> = Rc::new(generator);

            Self::new_internal(objects, generator)
        }

        /// Gets an item from the pool.
        ///
        /// Will block if the pool is empty and the generator fn blocks.
        pub fn get(&self) -> PoolRef<T> {
            let maybe_item = self.objects.borrow_mut().pop_front();
            let item = maybe_item.unwrap_or_else(self.generator.as_ref());

            PoolRef {
                item: Some(item),
                pool: self.clone(),
            }
        }

        pub fn put(&self, item: T) {
            self.objects.borrow_mut().push_back(item);
        }
    }

    pub struct PoolRef<T> {
        pool: Pool<T>,
        item: Option<T>,
    }

    impl<T> Drop for PoolRef<T> {
        fn drop(&mut self) {
            self.pool.put(self.item.take().expect("pool ref dropped twice???"));
        }
    }

    impl<T> Deref for PoolRef<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.item.as_ref().expect("pool ref was dropped, but then used???")
        }
    }

    impl<T> DerefMut for PoolRef<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.item.as_mut().expect("pool ref was dropped, but then used???")
        }
    }
}
