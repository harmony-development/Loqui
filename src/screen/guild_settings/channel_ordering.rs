use client::{
    error::ClientError,
    harmony_rust_sdk::{
        api::{
            chat::all_permissions::{CHANNELS_MANAGE_CHANGE_INFORMATION, CHANNELS_MANAGE_CREATE, CHANNELS_MANAGE_MOVE},
            harmonytypes::ItemPosition,
        },
        client::api::chat::channel::UpdateChannelOrder,
    },
    Client,
};
use iced::Tooltip;
use iced_aw::TabLabel;

use crate::{
    component::*,
    label_button, length,
    screen::{
        guild_settings::Message as ParentMessage, map_to_nothing, ClientExt, Message as TopLevelMessage,
        Screen as TopLevelScreen,
    },
    space,
    style::{Theme, PADDING, SPACING},
};

use super::{GuildMetadata, Tab};

#[derive(Debug, Clone)]
pub enum OrderingMessage {
    MoveChannel { id: u64, new_place: ItemPosition },
    GoBack,
}

#[derive(Debug, Default, Clone)]
pub struct OrderingTab {
    button_states: Vec<(
        button::State,
        button::State,
        button::State,
        button::State,
        button::State,
    )>,
    channel_list_state: scrollable::State,
    back_but_state: button::State,
    create_channel_state: button::State,
    pub error_message: String,
}

impl OrderingTab {
    pub fn update(
        &mut self,
        message: OrderingMessage,
        client: &Client,
        _: &mut GuildMetadata,
        guild_id: u64,
    ) -> Command<TopLevelMessage> {
        match message {
            OrderingMessage::MoveChannel { id, new_place } => client.mk_cmd(
                |inner| async move {
                    inner
                        .chat()
                        .await
                        .update_channel_order(UpdateChannelOrder::new(guild_id, id, new_place))
                        .await
                },
                map_to_nothing,
            ),
            OrderingMessage::GoBack => TopLevelScreen::pop_screen_cmd(),
        }
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_message = error.to_string();
        Command::none()
    }
}

impl Tab for OrderingTab {
    type Message = ParentMessage;

    fn title(&self) -> String {
        String::from("Channels")
    }

    fn tab_label(&self) -> TabLabel {
        TabLabel::IconText(Icon::List.into(), self.title())
    }

    fn content(
        &mut self,
        client: &Client,
        guild_id: u64,
        _: &mut GuildMetadata,
        theme: &Theme,
        _: &ThumbnailCache,
    ) -> Element<'_, ParentMessage> {
        let mut channels = Scrollable::new(&mut self.channel_list_state)
            .align_items(Align::Start)
            .height(length!(+))
            .width(length!(+))
            .padding(PADDING)
            .spacing(SPACING)
            .style(theme);

        if let Some(guild) = client.guilds.get(&guild_id) {
            self.button_states.resize_with(guild.channels.len(), Default::default);
            for ((channel_id, channel), (up_state, down_state, edit_state, copy_state, copy_name_state)) in
                guild.channels.iter().zip(&mut self.button_states)
            {
                let channel_id = *channel_id;

                let mut content_widgets = Vec::with_capacity(6);
                content_widgets.push(channel_icon(channel));
                content_widgets.push(
                    Tooltip::new(
                        label_button!(copy_name_state, channel.name.as_str())
                            .style(theme)
                            .on_press(ParentMessage::CopyToClipboard(channel.name.to_string())),
                        "Click to copy",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
                content_widgets.push(
                    Tooltip::new(
                        label_button!(copy_state, format!("ID {}", channel_id))
                            .style(theme)
                            .on_press(ParentMessage::CopyIdToClipboard(channel_id)),
                        "Click to copy",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
                content_widgets.push(space!(w+).into());
                if channel.has_perm(CHANNELS_MANAGE_CHANGE_INFORMATION) {
                    content_widgets.push(
                        Tooltip::new(
                            Button::new(edit_state, icon(Icon::Pencil))
                                .style(theme)
                                .on_press(ParentMessage::ShowUpdateChannelModal(channel_id)),
                            "Edit channel",
                            iced::tooltip::Position::Top,
                        )
                        .style(theme)
                        .into(),
                    );
                }
                if guild.has_perm(CHANNELS_MANAGE_MOVE) {
                    let channel_index = guild.channels.get_index_of(&channel_id).unwrap();

                    let up_place = guild
                        .channels
                        .get_index(channel_index.wrapping_sub(2))
                        .map(|(id, _)| *id);
                    let down_place = guild.channels.get_index(channel_index + 2).map(|(id, _)| *id);

                    let mk_place = |id, id_after| match (id, id_after) {
                        (Some(before), Some(after)) => (before != after).then(|| ItemPosition::new_after(after)),
                        (Some(before), None) => (channel_index != guild.channels.len().saturating_sub(1))
                            .then(|| ItemPosition::new_before(before)),
                        (None, Some(after)) => (channel_index != 0).then(|| ItemPosition::new_after(after)),
                        (None, None) => None,
                    };
                    let mut up_but = Button::new(up_state, icon(Icon::ArrowUp)).style(theme);
                    if let Some(new_place) = mk_place(up_place, Some(channel_id)) {
                        up_but = up_but.on_press(ParentMessage::Ordering(OrderingMessage::MoveChannel {
                            id: channel_id,
                            new_place,
                        }));
                    }
                    let mut down_but = Button::new(down_state, icon(Icon::ArrowDown)).style(theme);
                    if let Some(new_place) = mk_place(Some(channel_id), down_place) {
                        down_but = down_but.on_press(ParentMessage::Ordering(OrderingMessage::MoveChannel {
                            id: channel_id,
                            new_place,
                        }));
                    }

                    content_widgets.push(
                        Tooltip::new(up_but, "Move up", iced::tooltip::Position::Top)
                            .style(theme)
                            .into(),
                    );
                    content_widgets.push(
                        Tooltip::new(down_but, "Move down", iced::tooltip::Position::Top)
                            .style(theme)
                            .into(),
                    );
                }

                channels = channels.push(Container::new(row(content_widgets)).style(theme));
            }
            if guild.has_perm(CHANNELS_MANAGE_CREATE) {
                channels = channels.push(
                    fill_container(
                        label_button!(&mut self.create_channel_state, "Create Channel")
                            .on_press(ParentMessage::NewChannel)
                            .style(theme),
                    )
                    .height(length!(-)),
                );
            }
        }

        let mut content = Vec::with_capacity(3);

        if !self.error_message.is_empty() {
            content.push(label!(self.error_message.as_str()).color(theme.user_theme.error).into())
        }
        content.push(fill_container(channels).style(theme).into());
        content.push(
            label_button!(&mut self.back_but_state, "Back")
                .on_press(ParentMessage::Ordering(OrderingMessage::GoBack))
                .style(theme)
                .into(),
        );

        Container::new(column(content)).padding(PADDING * 10).into()
    }
}
