use crate::{
    client::{
        channel::Channels,
        guild::{Guild, Guilds},
    },
    component::*,
    label,
    screen::{main::Message, truncate_string},
    space,
    style::{Theme, ALT_COLOR, AVATAR_WIDTH, DEF_SIZE, PADDING, SPACING},
};

use client::{bool_ext::BoolExt, channel::Channel};
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
    type Item<'a, 'b> = (
        (&'b u64, &'b Channel),
        &'a mut (button::State, button::State, button::State),
    );
    let process_item =
        |mut list: Scrollable<'a, Message>,
         ((channel_id, channel), (button_state, edit_state, copy_state)): Item<'a, '_>| {
            let (channel_name_prefix, channel_prefix_size) = channel
                .is_category
                .some((Icon::ListNested, DEF_SIZE - 4))
                .unwrap_or((Icon::Hash, DEF_SIZE));

            let read_color = channel.has_unread.then(|| Color::WHITE).unwrap_or(ALT_COLOR);

            let mut content_widgets = Vec::with_capacity(7);
            let icon_content = icon(channel_name_prefix).color(read_color).size(channel_prefix_size);
            let icon_content = if channel.is_category {
                Column::with_children(vec![space!(h = SPACING - (SPACING / 4)).into(), icon_content.into()])
                    .align_items(Align::Center)
                    .into()
            } else {
                icon_content.into()
            };
            content_widgets.push(icon_content);
            channel
                .is_category
                .and_do(|| content_widgets.push(space!(w = SPACING).into()));
            content_widgets.push(
                label!(truncate_string(
                    &channel.name,
                    channel.user_perms.manage_channel.then(|| 15).unwrap_or(17)
                ))
                .color(read_color)
                .size(DEF_SIZE - 2)
                .into(),
            );
            content_widgets.push(space!(w+).into());
            content_widgets.push(
                Button::new(copy_state, icon(Icon::Clipboard).size(DEF_SIZE - 8))
                    .style(theme)
                    .on_press(Message::CopyIdToClipboard(*channel_id))
                    .into(),
            );

            if channel.user_perms.manage_channel {
                content_widgets.push(space!(w = SPACING / 2).into());
                content_widgets.push(
                    Button::new(edit_state, icon(Icon::Pencil).size(DEF_SIZE - 8))
                        .style(theme)
                        .on_press(Message::ShowUpdateChannelModal(guild_id, *channel_id))
                        .into(),
                );
            }

            let mut but = Button::new(
                button_state,
                Row::with_children(content_widgets).align_items(Align::Center),
            )
            .width(length!(+))
            .style(theme.secondary());

            if channel.is_category {
                but = but.style(theme.embed());
            } else if current_channel_id != Some(*channel_id) {
                but = but.on_press(on_button_press(*channel_id));
            }

            list = list.push(but);

            if channel.is_category {
                list = list.push(Rule::horizontal(SPACING).style(theme.secondary()));
            }

            list
        };

    let list_init = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(length!(+))
        .spacing(SPACING)
        .padding(PADDING / 4);

    channels
        .iter()
        .zip(buttons_state.iter_mut())
        .fold(list_init, process_item)
        .into()
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
    let buttons_state_len = buttons_state.len();

    type Item<'a, 'b> = ((&'b u64, &'b Guild), (usize, &'a mut button::State));
    let process_item = |mut list: Scrollable<'a, Message>, ((guild_id, guild), (index, button_state)): Item<'a, '_>| {
        let mk_but = |state: &'a mut button::State, content: Element<'a, Message>| {
            let theme = if guild.channels.values().any(|c| c.has_unread) {
                theme.round().border_color(Color::WHITE)
            } else {
                theme
            };

            Button::new(state, fill_container(content).style(theme))
                .width(length!(+))
                .style(theme.secondary())
        };

        let but = if index >= buttons_state_len - 1 {
            // [ref:create_join_guild_but_state]
            mk_but(button_state, icon(Icon::Plus).size(DEF_SIZE + 10).into()).on_press(Message::OpenCreateJoinGuild)
        } else {
            let content = guild
                .picture
                .as_ref()
                .and_then(|guild_picture| thumbnail_cache.thumbnails.get(guild_picture))
                .map_or_else::<Element<Message>, _, _>(
                    || {
                        label!(guild.name.chars().next().unwrap_or('u').to_ascii_uppercase())
                            .size(DEF_SIZE + 10)
                            .into()
                    },
                    |handle| Image::new(handle.clone()).width(length!(= AVATAR_WIDTH - 4)).into(),
                );

            let mut but = mk_but(button_state, content);

            if current_guild_id != Some(*guild_id) {
                but = but.on_press(on_button_press(*guild_id));
            }

            but
        };

        list = list.push(
            Tooltip::new(but, &guild.name, Position::Bottom)
                .gap(8)
                .style(theme.secondary()),
        );

        if index < buttons_state_len - 1 {
            list = list.push(Rule::horizontal(SPACING).style(theme.secondary()));
        }

        list
    };
    let list_init = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(length!(+))
        .spacing(SPACING)
        .padding(PADDING / 4);

    guilds
        .into_iter()
        .chain(std::iter::once((
            &0,
            &Guild {
                name: String::from("Create / join guild"),
                ..Default::default()
            },
        ))) // [ref:create_join_guild_but_state]
        .zip(buttons_state.iter_mut().enumerate())
        .fold(list_init, process_item)
        .into()
}
