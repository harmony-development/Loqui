pub mod event_history;
pub mod room_list;

use crate::length;
pub use crate::{align, color, label};
pub use event_history::build_event_history;
pub use iced::{
    button, pick_list, scrollable, text_input, Align, Button, Color, Column, Command, Container,
    Element, Image, Length, PickList, Row, Scrollable, Space, Subscription, Text, TextInput,
};
pub use room_list::build_channel_list;

use super::style::{PADDING, SPACING};

#[inline(always)]
pub fn column<M>(children: Vec<Element<M>>) -> Column<M> {
    Column::with_children(children)
        .align_items(align!(|))
        .padding(PADDING)
        .spacing(SPACING)
}

#[inline(always)]
pub fn row<M>(children: Vec<Element<M>>) -> Row<M> {
    Row::with_children(children)
        .align_items(align!(|))
        .padding(PADDING)
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
