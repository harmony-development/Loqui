use crate::{
    client::{channel::Channels, content::ThumbnailCache, guild::Guilds},
    ui::{
        component::*,
        style::{Theme, PADDING, SPACING},
    },
};

/// Builds a room list.
#[allow(clippy::clippy::too_many_arguments)]
pub fn build_channel_list<'a, Message: Clone + 'a>(
    channels: &Channels,
    current_channel_id: Option<u64>,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [button::State],
    on_button_press: fn(u64) -> Message,
    theme: Theme,
) -> Element<'a, Message> {
    let mut channel_list = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(Length::Fill)
        .spacing(SPACING * 2)
        .padding(PADDING / 4);

    let is_current_channel = |channel_id: u64| {
        if let Some(id) = current_channel_id {
            if channel_id == id {
                return true;
            }
        }
        false
    };

    for ((channel_id, channel), button_state) in channels.iter().zip(buttons_state.iter_mut()) {
        let channel_name_prefix = if channel.is_category { "+" } else { "#" };
        let channel_name_formatted = format!("{}{}", channel_name_prefix, channel.name);
        let content = label(channel_name_formatted);

        let mut but = Button::new(button_state, content)
            .width(Length::Fill)
            .style(theme.secondary());

        if !is_current_channel(*channel_id) {
            but = but.on_press(on_button_press(*channel_id));
        }

        channel_list = channel_list.push(but);
    }

    channel_list.into()
}

#[allow(clippy::clippy::too_many_arguments)]
pub fn build_guild_list<'a, Message: Clone + 'a>(
    guilds: &Guilds,
    thumbnail_cache: &ThumbnailCache,
    current_guild_id: Option<u64>,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [button::State],
    on_button_press: fn(u64) -> Message,
    theme: Theme,
) -> Element<'a, Message> {
    let mut guild_list = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(Length::Fill)
        .spacing(SPACING * 2)
        .padding(PADDING / 4);

    let is_current_guild = |room_id: u64| {
        if let Some(id) = current_guild_id {
            if room_id == id {
                return true;
            }
        }
        false
    };

    for ((guild_id, guild), button_state) in guilds.into_iter().zip(buttons_state.iter_mut()) {
        let content = fill_container(
            if let Some(handle) = guild
                .picture
                .as_ref()
                .map(|guild_picture| thumbnail_cache.get_thumbnail(&guild_picture))
                .flatten()
            {
                Element::from(Image::new(handle.clone()))
            } else {
                Element::from(
                    label(
                        guild
                            .name
                            .chars()
                            .next()
                            .unwrap_or('u')
                            .to_ascii_uppercase(),
                    )
                    .size(30),
                )
            },
        );

        let mut but = Button::new(button_state, content)
            .width(Length::Fill)
            .style(theme.secondary());

        if !is_current_guild(*guild_id) {
            but = but.on_press(on_button_press(*guild_id));
        }

        guild_list = guild_list.push(but);
    }

    guild_list.into()
}
