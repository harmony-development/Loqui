use crate::{
    client::{Room, Rooms},
    ui::style::{DarkButton, Theme},
};
use iced::{button, scrollable, Align, Button, Element, Length, Scrollable, Text};
use ruma::RoomId;

/// Builds a room list.
pub fn build_room_list<'a, Message: Clone + 'a>(
    rooms: &Rooms,
    current_room_id: Option<&RoomId>,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [button::State],
    on_button_press: fn(RoomId) -> Message,
    theme: Theme,
) -> Element<'a, Message> {
    let mut rooms = rooms.iter().collect::<Vec<(&RoomId, &Room)>>();
    rooms.sort_unstable_by(|(_, room), (_, other_room)| {
        room.get_display_name().cmp(&other_room.get_display_name())
    });

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

    for ((room_id, room), button_state) in rooms.into_iter().zip(buttons_state.iter_mut()) {
        let mut but = Button::new(button_state, Text::new(room.get_display_name()))
            .width(Length::Fill)
            .style(DarkButton);

        if !is_current_room(room_id) {
            but = but.on_press(on_button_press(room_id.clone()));
        }

        room_list = room_list.push(but);
    }

    room_list.into()
}
