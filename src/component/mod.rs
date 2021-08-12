pub mod chan_guild_list;
pub mod event_history;

pub use crate::{color, label, space};
use crate::{
    length,
    style::{ALT_COLOR, DEF_SIZE},
};
pub use chan_guild_list::build_channel_list;
use client::{bool_ext::BoolExt, channel::Channel, harmony_rust_sdk::api::rest::FileId, IndexMap};
pub use event_history::build_event_history;
pub use iced::{
    button, pick_list, scrollable, text_input, Align, Button, Checkbox, Color, Column, Command, Container, Element,
    Image, Length, PickList, Row, Rule, Scrollable, Space, Subscription, Text, TextInput, Toggler,
};
pub use iced_aw::Icon;
pub use iced_native::Padding;

use super::style::{PADDING, SPACING};

pub fn column<M>(children: Vec<Element<M>>) -> Column<M> {
    Column::with_children(children)
        .align_items(Align::Center)
        .padding(PADDING / 2)
        .spacing(SPACING)
}

pub fn row<M>(children: Vec<Element<M>>) -> Row<M> {
    Row::with_children(children)
        .align_items(Align::Center)
        .padding(PADDING / 2)
        .spacing(SPACING)
}

/// Creates a `Container` that fills all the space available and centers it child.
pub fn fill_container<'a, M>(child: impl Into<Element<'a, M>>) -> Container<'a, M> {
    Container::new(child)
        .center_x()
        .center_y()
        .width(length!(+))
        .height(length!(+))
}

pub fn icon(icon: Icon) -> Text {
    label!(icon).font(iced_aw::ICON_FONT)
}

pub fn channel_icon<'a, M: 'a>(channel: &Channel) -> Element<'a, M> {
    let (channel_name_prefix, channel_prefix_size) = channel
        .is_category
        .then(|| (Icon::ListNested, DEF_SIZE - 4))
        .unwrap_or((Icon::Hash, DEF_SIZE));

    let icon_content = icon(channel_name_prefix).color(ALT_COLOR).size(channel_prefix_size);
    if channel.is_category {
        Column::with_children(vec![space!(h = SPACING - (SPACING / 4)).into(), icon_content.into()])
            .align_items(Align::Center)
            .into()
    } else {
        icon_content.into()
    }
}

#[macro_export]
macro_rules! label_button {
    ($st:expr, $l:expr) => {
        ::iced::Button::new($st, fill_container($crate::label!($l)))
    };
    ($st:expr, $($arg:tt)*) => {
        ::iced::Button::new($st, fill_container($crate::label!($($arg)*)))
    };
}

#[macro_export]
macro_rules! label {
    ($l:expr) => {
        ::iced::Text::new($l)
    };
    ($($arg:tt)*) => {
        ::iced::Text::new(::std::format!($($arg)*))
    };
}

#[macro_export]
macro_rules! color {
    ($r:expr, $g:expr, $b:expr) => {
        ::iced::Color::from_rgb($r as f32 / 255.0, $g as f32 / 255.0, $b as f32 / 255.0)
    };
    ($r:expr, $g:expr, $b:expr, $a:expr) => {
        ::iced::Color::from_rgba(
            $r as f32 / 255.0,
            $g as f32 / 255.0,
            $b as f32 / 255.0,
            $a as f32 / 255.0,
        )
    };
    (. $r:expr, $g:expr, $b:expr) => {
        ::iced::Color::from_rgb($r, $g, $b)
    };
    (. $r:expr, $g:expr, $b:expr, $a:expr) => {
        ::iced::Color::from_rgba($r, $g, $b, $a)
    };
}

#[macro_export]
macro_rules! space {
    (w+) => {
        ::iced::Space::with_width($crate::length!(+))
    };
    (h+) => {
        ::iced::Space::with_height($crate::length!(+))
    };
    (= $w:expr, $h:expr) => {
        ::iced::Space::new($crate::length!(= $w), $crate::length!(= $h))
    };
    (w = $w:expr) => {
        ::iced::Space::with_width($crate::length!(= $w))
    };
    (h = $h:expr) => {
        ::iced::Space::with_height($crate::length!(= $h))
    };
    (% $w:expr, $h:expr) => {
        ::iced::Space::new($crate::length!(% $w), $crate::length!(% $h))
    };
    (w % $w:expr) => {
        ::iced::Space::with_width($crate::length!(% $w))
    };
    (h % $h:expr) => {
        ::iced::Space::with_height($crate::length!(% $h))
    };
}

#[macro_export]
macro_rules! length {
    (-) => {
        ::iced::Length::Shrink
    };
    (+) => {
        ::iced::Length::Fill
    };
    (= $u:expr) => {
        ::iced::Length::Units($u)
    };
    (% $u:expr) => {
        ::iced::Length::FillPortion($u)
    };
}

pub use iced::image::Handle as ImageHandle;

pub fn get_image_size_from_handle(handle: &ImageHandle) -> usize {
    use iced_native::image::Data;
    match handle.data() {
        Data::Bytes(raw) => raw.len(),
        Data::Pixels {
            pixels,
            height: _,
            width: _,
        } => pixels.len(),
        Data::Path(_) => unreachable!("we dont use images with path"),
    }
}

enum Cache {
    Thumb,
    Avatar,
    ProfileAvat,
    Emote,
}

#[derive(Debug)]
pub struct ThumbnailCache {
    pub thumbnails: IndexMap<FileId, ImageHandle>,
    pub avatars: IndexMap<FileId, ImageHandle>,
    pub profile_avatars: IndexMap<FileId, ImageHandle>,
    pub emotes: IndexMap<FileId, ImageHandle>,
    max_size: usize,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        const MAX_CACHE_SIZE: usize = 1000 * 1000 * 100; // 100Mb
        Self::new(MAX_CACHE_SIZE)
    }
}

impl ThumbnailCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            thumbnails: IndexMap::default(),
            avatars: IndexMap::default(),
            profile_avatars: IndexMap::default(),
            emotes: IndexMap::default(),
            max_size,
        }
    }

    #[inline(always)]
    pub fn put_thumbnail(&mut self, thumbnail_id: FileId, thumbnail: ImageHandle) {
        self.internal_put_thumbnail(Cache::Thumb, thumbnail_id, thumbnail);
    }

    #[inline(always)]
    pub fn put_avatar_thumbnail(&mut self, thumbnail_id: FileId, thumbnail: ImageHandle) {
        self.internal_put_thumbnail(Cache::Avatar, thumbnail_id, thumbnail)
    }

    #[inline(always)]
    pub fn put_profile_avatar_thumbnail(&mut self, thumbnail_id: FileId, thumbnail: ImageHandle) {
        self.internal_put_thumbnail(Cache::ProfileAvat, thumbnail_id, thumbnail)
    }

    #[inline(always)]
    pub fn put_emote_thumbnail(&mut self, thumbnail_id: FileId, thumbnail: ImageHandle) {
        self.internal_put_thumbnail(Cache::Emote, thumbnail_id, thumbnail)
    }

    fn internal_put_thumbnail(&mut self, cache: Cache, thumbnail_id: FileId, thumbnail: ImageHandle) {
        let map = match cache {
            Cache::Avatar => &mut self.avatars,
            Cache::ProfileAvat => &mut self.profile_avatars,
            Cache::Thumb => &mut self.thumbnails,
            Cache::Emote => &mut self.emotes,
        };

        let thumbnail_size = get_image_size_from_handle(&thumbnail);
        let cache_size: usize = map.values().map(get_image_size_from_handle).sum();

        (cache_size + thumbnail_size > self.max_size)
            .and_do(|| {
                let mut current_size = 0;
                let mut remove_upto = 0;

                for (index, size) in map.values().map(get_image_size_from_handle).enumerate() {
                    if current_size >= thumbnail_size {
                        remove_upto = index + 1;
                        break;
                    }
                    current_size += size;
                }

                let len = map.len();
                (0..remove_upto).for_each(|index| {
                    (index < len).and_do(|| {
                        map.shift_remove_index(index);
                    });
                });
            })
            .or_do(|| {
                map.insert(thumbnail_id, thumbnail);
            });
    }
}
