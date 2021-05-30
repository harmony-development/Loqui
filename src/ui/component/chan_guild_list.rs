use crate::{
    client::{
        channel::Channels,
        content::ThumbnailCache,
        guild::{Guild, Guilds},
    },
    label, space,
    ui::{
        component::*,
        screen::main::Message,
        style::{Theme, DEF_SIZE, PADDING, SPACING},
    },
};

use iced::{tooltip::Position, Tooltip};

/// Builds a room list.
#[allow(clippy::too_many_arguments)]
pub fn build_channel_list<'a>(
    channels: &Channels,
    guild_id: u64,
    current_channel_id: Option<u64>,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [(button::State, button::State, button::State)],
    on_button_press: fn(u64) -> Message,
    theme: Theme,
) -> Element<'a, Message> {
    let mut channel_list = Scrollable::new(state)
        .style(theme)
        .align_items(align!(|<))
        .height(length!(+))
        .spacing(SPACING)
        .padding(PADDING / 4);

    for ((channel_id, channel), (button_state, edit_state, copy_state)) in
        channels.iter().zip(buttons_state.iter_mut())
    {
        let channel_name_prefix = if channel.is_category {
            Icon::ListNested
        } else {
            Icon::Hash
        };

        let mut content_widgets = Vec::with_capacity(6);
        content_widgets.push(
            icon(channel_name_prefix)
                .size(DEF_SIZE - 2)
                .vertical_alignment(iced::VerticalAlignment::Bottom)
                .into(),
        );
        content_widgets.push(label!(&channel.name).size(DEF_SIZE - 2).into());
        content_widgets.push(space!(w+).into());
        content_widgets.push(
            Button::new(
                copy_state,
                icon(Icon::Clipboard)
                    .vertical_alignment(iced::VerticalAlignment::Top)
                    .size(DEF_SIZE - 8),
            )
            .style(theme)
            .on_press(Message::CopyToClipboard(channel_id.to_string()))
            .into(),
        );

        if channel.user_perms.manage_channel {
            content_widgets.push(space!(w = SPACING / 2).into());
            content_widgets.push(
                Button::new(edit_state, icon(Icon::Pencil).size(DEF_SIZE - 8))
                    .style(theme)
                    .on_press(Message::TryShowUpdateChannelModal(guild_id, *channel_id))
                    .into(),
            );
        }

        let content = Row::with_children(content_widgets).align_items(align!(|));
        let mut but = Button::new(button_state, content)
            .width(length!(+))
            .style(theme.secondary());

        if current_channel_id != Some(*channel_id) {
            but = but.on_press(on_button_press(*channel_id));
        }

        channel_list = channel_list.push(but);
    }

    channel_list.into()
}

#[allow(clippy::too_many_arguments)]
pub fn build_guild_list<'a>(
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
        .align_items(align!(|<))
        .height(length!(+))
        .spacing(SPACING)
        .padding(PADDING / 4);

    let buttons_state_len = buttons_state.len();
    for ((guild_id, guild), (index, button_state)) in guilds
        .into_iter()
        .chain(std::iter::once((&0, &Guild::default()))) // [ref:create_join_guild_but_state]
        .zip(buttons_state.iter_mut().enumerate())
    {
        if index >= buttons_state_len - 1 {
            // [ref:create_join_guild_but_state]
            let but = Button::new(
                button_state,
                fill_container(icon(Icon::Plus).size(DEF_SIZE + 10)),
            )
            .width(length!(+))
            .style(theme.secondary())
            .on_press(Message::OpenCreateJoinGuild);
            guild_list = guild_list.push(but);
        } else {
            let content = fill_container(
                guild
                    .picture
                    .as_ref()
                    .map(|guild_picture| thumbnail_cache.get_thumbnail(&guild_picture))
                    .flatten()
                    .map_or_else(
                        || {
                            Element::from(
                                label!(guild
                                    .name
                                    .chars()
                                    .next()
                                    .unwrap_or('u')
                                    .to_ascii_uppercase())
                                .size(DEF_SIZE + 10),
                            )
                        },
                        |handle| Element::from(Image::new(handle.clone())),
                    ),
            );

            let mut but = Button::new(button_state, content)
                .width(length!(+))
                .style(theme.secondary());

            if current_guild_id != Some(*guild_id) {
                but = but.on_press(on_button_press(*guild_id));
            }

            let tooltip = Tooltip::new(but, &guild.name, Position::Bottom)
                .gap(8)
                .style(theme.secondary());

            guild_list = guild_list.push(tooltip);
        }
    }

    guild_list.into()
}
