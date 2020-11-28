pub mod event_history;
pub mod room_list;

pub use event_history::build_event_history;
pub use room_list::build_room_list;

use super::style::{PADDING, SPACING};
use iced::{Align, Column, Element, Row};

pub fn column<'a, M>(children: Vec<Element<'a, M>>) -> Column<'a, M> {
    Column::with_children(children)
        .align_items(Align::Start)
        .padding(PADDING)
        .spacing(SPACING)
}

pub fn row<'a, M>(children: Vec<Element<'a, M>>) -> Row<'a, M> {
    Row::with_children(children)
        .align_items(Align::Start)
        .padding(PADDING)
        .spacing(SPACING)
}
