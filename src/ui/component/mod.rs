pub mod event_history;
pub mod room_list;

pub use crate::color;
pub use event_history::build_event_history;
pub use iced::{
    button, pick_list, scrollable, text_input, Align, Button, Color, Column, Command, Container,
    Element, Image, Length, PickList, Row, Scrollable, Space, Subscription, Text, TextInput,
};
pub use room_list::build_room_list;

use super::style::{PADDING, SPACING};

#[inline(always)]
pub fn column<M>(children: Vec<Element<M>>) -> Column<M> {
    Column::with_children(children)
        .align_items(Align::Center)
        .padding(PADDING)
        .spacing(SPACING)
}

#[inline(always)]
pub fn row<M>(children: Vec<Element<M>>) -> Row<M> {
    Row::with_children(children)
        .align_items(Align::Center)
        .padding(PADDING)
        .spacing(SPACING)
}

#[inline(always)]
/// Creates a `Container` that fills all the space available and centers it child.
pub fn fill_container<'a, M>(child: impl Into<Element<'a, M>>) -> Container<'a, M> {
    Container::new(child)
        .center_x()
        .center_y()
        .width(Length::Fill)
        .height(Length::Fill)
}

#[inline(always)]
pub fn wspace(width: u16) -> Space {
    Space::with_width(Length::FillPortion(width))
}

#[inline(always)]
pub fn hspace(height: u16) -> Space {
    Space::with_height(Length::FillPortion(height))
}

#[inline(always)]
pub fn awspace(width: u16) -> Space {
    Space::with_width(Length::Units(width))
}

#[inline(always)]
pub fn ahspace(height: u16) -> Space {
    Space::with_height(Length::Units(height))
}

#[inline(always)]
pub fn label_button<'a, M: Clone + 'a>(
    state: &'a mut button::State,
    text: impl Into<String>,
) -> Button<'a, M> {
    Button::new(state, fill_container(label(text)))
}

#[inline(always)]
pub fn label(text: impl Into<String>) -> Text {
    Text::new(text)
}

#[macro_export]
macro_rules! color {
    ($r:expr, $g:expr, $b:expr) => {
        Color::from_rgb($r as f32 / 255.0, $g as f32 / 255.0, $b as f32 / 255.0)
    };
}
