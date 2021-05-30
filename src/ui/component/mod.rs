pub mod chan_guild_list;
pub mod event_history;

use crate::length;
pub use crate::{align, color, label};
pub use chan_guild_list::build_channel_list;
use client::{harmony_rust_sdk::api::rest::FileId, IndexMap};
pub use event_history::build_event_history;
pub use iced::{
    button, pick_list, scrollable, text_input, Align, Button, Color, Column, Command, Container, Element, Image,
    Length, PickList, Row, Scrollable, Space, Subscription, Text, TextInput,
};
pub use iced_aw::Icon;

use super::style::{PADDING, SPACING};

#[inline(always)]
pub fn column<M>(children: Vec<Element<M>>) -> Column<M> {
    Column::with_children(children)
        .align_items(align!(|))
        .padding(PADDING / 2)
        .spacing(SPACING)
}

#[inline(always)]
pub fn row<M>(children: Vec<Element<M>>) -> Row<M> {
    Row::with_children(children)
        .align_items(align!(|))
        .padding(PADDING / 2)
        .spacing(SPACING)
}

#[inline(always)]
/// Creates a `Container` that fills all the space available and centers it child.
pub fn fill_container<'a, M>(child: impl Into<Element<'a, M>>) -> Container<'a, M> {
    Container::new(child)
        .center_x()
        .center_y()
        .width(length!(+))
        .height(length!(+))
}

#[inline(always)]
pub fn icon(icon: Icon) -> Text {
    label!(icon).font(iced_aw::ICON_FONT)
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

#[macro_export]
macro_rules! align {
    (>|) => {
        ::iced::Align::End
    };
    (|) => {
        ::iced::Align::Center
    };
    (|<) => {
        ::iced::Align::Start
    };
}

pub use iced::image::Handle as ImageHandle;

pub fn get_image_size_from_handle(handle: &ImageHandle) -> Option<u64> {
    use iced_native::image::Data;
    // This one angers me a lot, iced pls read the file beforehand and cache it
    match handle.data() {
        Data::Bytes(raw) => Some(raw.len() as u64),
        Data::Path(path) => std::fs::metadata(path).map_or(None, |meta| Some(meta.len())),
        Data::Pixels {
            pixels,
            height: _,
            width: _,
        } => Some(pixels.len() as u64),
    }
}

#[derive(Debug)]
pub struct ThumbnailCache {
    thumbnails: IndexMap<FileId, ImageHandle>,
    max_size: u64,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        const MAX_CACHE_SIZE: u64 = 1000 * 1000 * 100; // 100Mb
        Self::new(MAX_CACHE_SIZE)
    }
}

impl ThumbnailCache {
    pub fn new(max_size: u64) -> Self {
        Self {
            thumbnails: IndexMap::default(),
            max_size,
        }
    }

    pub fn put_thumbnail(&mut self, thumbnail_id: FileId, thumbnail: ImageHandle) {
        let thumbnail_size = match get_image_size_from_handle(&thumbnail) {
            Some(size) => size,
            None => return,
        };
        let cache_size = self.len();

        if cache_size + thumbnail_size > self.max_size {
            let mut current_size = 0;
            let mut remove_upto = 0;
            for (index, size) in self
                .thumbnails
                .values()
                .flat_map(|h| get_image_size_from_handle(h))
                .enumerate()
            {
                if current_size >= thumbnail_size {
                    remove_upto = index + 1;
                    break;
                }
                current_size += size;
            }
            for index in 0..remove_upto {
                self.thumbnails.shift_remove_index(index);
            }
        } else {
            self.thumbnails.insert(thumbnail_id, thumbnail);
        }
    }

    pub fn len(&self) -> u64 {
        self.thumbnails
            .values()
            .flat_map(|h| get_image_size_from_handle(h))
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() < 1
    }

    pub fn has_thumbnail(&self, thumbnail_id: &FileId) -> bool {
        self.thumbnails.contains_key(thumbnail_id)
    }

    pub fn get_thumbnail(&self, thumbnail_id: &FileId) -> Option<&ImageHandle> {
        self.thumbnails.get(thumbnail_id)
    }

    pub fn invalidate_thumbnail(&mut self, thumbnail_id: &FileId) {
        self.thumbnails.remove(thumbnail_id);
    }
}
