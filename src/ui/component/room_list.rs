use crate::{
    client::Rooms,
    ui::style::{DarkButton, Theme},
};
use fuzzy_matcher::skim::SkimMatcherV2;
use iced::{button, scrollable, Align, Button, Element, Length, Scrollable, Text};
use ruma::RoomId;

/// Builds a room list.
pub fn build_room_list<'a, Message: Clone + 'a>(
    rooms: &Rooms,
    current_room_id: Option<&RoomId>,
    room_filter_text: &str,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [button::State],
    on_button_press: fn(RoomId) -> Message,
    theme: Theme,
) -> (Element<'a, Message>, Option<RoomId>) {
    let mut rooms = rooms
        .iter()
        .map(|(room_id, room)| (room_id, room.get_display_name()))
        .collect::<Vec<(&RoomId, String)>>();

    if room_filter_text.is_empty() {
        rooms.sort_unstable_by(|(_, room_name), (_, other_room_name)| {
            room_name.cmp(&other_room_name)
        });
    } else {
        let matcher = SkimMatcherV2::default();

        let mut rooms_filtered = rooms
            .drain(..)
            .flat_map(|(room_id, room_name)| {
                Some((
                    matcher.fuzzy(&room_name, room_filter_text, false)?.0, // extract match score
                    room_id,
                    room_name,
                ))
            })
            .collect::<Vec<_>>();
        rooms_filtered.sort_unstable_by_key(|(score, _, _)| *score);
        rooms = rooms_filtered
            .into_iter()
            .rev()
            .map(|(_, room_id, room_name)| (room_id, room_name))
            .collect();
    }

    let first_room_id = rooms.first().map(|(room_id, _)| room_id.clone().clone());

    let mut room_list = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(Length::Fill)
        .spacing(8)
        .padding(4);

    let is_current_room = |room_id: &RoomId| {
        if let Some(id) = current_room_id {
            if room_id == id {
                return true;
            }
        }
        false
    };

    for ((room_id, room_name), button_state) in rooms.into_iter().zip(buttons_state.iter_mut()) {
        let mut but = Button::new(button_state, Text::new(room_name))
            .width(Length::Fill)
            .style(DarkButton);

        if !is_current_room(room_id) {
            but = but.on_press(on_button_press(room_id.clone()));
        }

        room_list = room_list.push(but);
    }

    (room_list.into(), first_room_id)
}
