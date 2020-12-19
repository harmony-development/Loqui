use crate::{
    client::{content::ThumbnailCache, Rooms},
    ui::{
        component::*,
        style::{Theme, PADDING, SPACING},
    },
};
use fuzzy_matcher::skim::SkimMatcherV2;
use http::Uri;
use ruma::RoomId;

/// Builds a room list.
pub fn build_room_list<'a, Message: Clone + 'a>(
    rooms: &Rooms,
    thumbnail_cache: &ThumbnailCache,
    current_room_id: Option<&RoomId>,
    room_filter_text: &str,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [button::State],
    on_button_press: fn(RoomId) -> Message,
    theme: Theme,
) -> (Element<'a, Message>, Option<RoomId>) {
    let mut rooms = rooms
        .iter()
        .map(|(room_id, room)| (room_id, room.get_display_name(), room.avatar_url()))
        .collect::<Vec<(&RoomId, String, Option<&Uri>)>>();

    if room_filter_text.is_empty() {
        rooms.sort_unstable_by(|(_, room_name, _), (_, other_room_name, _)| {
            room_name.cmp(&other_room_name)
        });
    } else {
        let matcher = SkimMatcherV2::default();

        let mut rooms_filtered = rooms
            .drain(..)
            .flat_map(|(room_id, room_name, room_avatar)| {
                Some((
                    matcher.fuzzy(&room_name, room_filter_text, false)?.0, // extract match score
                    room_id,
                    room_name,
                    room_avatar,
                ))
            })
            .collect::<Vec<_>>();
        rooms_filtered.sort_unstable_by_key(|(score, _, _, _)| *score);
        rooms = rooms_filtered
            .into_iter()
            .rev()
            .map(|(_, room_id, room_name, room_avatar)| (room_id, room_name, room_avatar))
            .collect();
    }

    let first_room_id = rooms.first().map(|(room_id, _, _)| room_id.clone().clone());

    let mut room_list = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(Length::Fill)
        .spacing(SPACING * 2)
        .padding(PADDING / 4);

    let is_current_room = |room_id: &RoomId| {
        if let Some(id) = current_room_id {
            if room_id == id {
                return true;
            }
        }
        false
    };

    for ((room_id, room_name, room_avatar), button_state) in
        rooms.into_iter().zip(buttons_state.iter_mut())
    {
        let mut content = Vec::with_capacity(2);
        if let Some(handle) = room_avatar
            .map(|u| thumbnail_cache.get_thumbnail(u))
            .flatten()
            .cloned()
        {
            content.push(Image::new(handle).width(Length::Units(32)).into());
        }
        content.push(label(room_name).into());

        let mut but = Button::new(button_state, row(content).padding(0))
            .width(Length::Fill)
            .style(theme.secondary());

        if !is_current_room(room_id) {
            but = but.on_press(on_button_press(room_id.clone()));
        }

        room_list = room_list.push(but);
    }

    (room_list.into(), first_room_id)
}
