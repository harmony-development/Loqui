use std::{convert::identity, ops::Not};

use super::{super::Message as TopLevelMessage, Message as ParentMessage};
use client::harmony_rust_sdk::{api::chat::Place, client::api::chat::channel::CreateChannel};
use iced_aw::Card;

use crate::{
    client::{error::ClientError, Client},
    component::*,
    label, label_button, length,
    screen::ClientExt,
    style::{Theme, PADDING, SPACING},
};

#[derive(Debug, Clone)]
pub enum ChannelState {
    Created {
        guild_id: u64,
        channel_id: u64,
        name: String,
    },
    Creating {
        name: String,
    },
    None,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState::None
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    ChannelNameChanged(String),
    CreateChannel,
    CreatedChannel { guild_id: u64, channel_id: u64 },
    GoBack,
    IsCategoryToggle(bool),
}

#[derive(Default, Debug, Clone)]
pub struct ChannelCreationModal {
    create_channel_back_but_state: button::State,
    channel_name_textedit_state: text_input::State,
    channel_create_but_state: button::State,
    channel_creation_state: ChannelState,
    channel_name_field: String,
    error_text: String,
    is_category: bool,
    pub guild_id: u64,
}

impl ChannelCreationModal {
    pub fn view(&mut self, theme: &Theme) -> Element<Message> {
        let mut create_text_edit = TextInput::new(
            &mut self.channel_name_textedit_state,
            "Enter a channel name...",
            &self.channel_name_field,
            Message::ChannelNameChanged,
        )
        .padding(PADDING / 2)
        .width(length!(= 300))
        .style(theme);

        let mut create = label_button!(&mut self.channel_create_but_state, "Create").style(theme);

        let is_category = Checkbox::new(self.is_category, "Category", Message::IsCategoryToggle).style(theme);

        if let ChannelState::None | ChannelState::Created { .. } = &self.channel_creation_state {
            if self.channel_name_field.is_empty().not() {
                create_text_edit = create_text_edit.on_submit(Message::CreateChannel);
                create = create.on_press(Message::CreateChannel);
            }
        }

        let mut create_widgets = Vec::with_capacity(2);
        match &self.channel_creation_state {
            ChannelState::Created { name, .. } => {
                create_widgets.push(
                    label!("Successfully created channel {}", name)
                        .color(theme.user_theme.success)
                        .into(),
                );
            }
            ChannelState::Creating { name } => create_widgets.push(label!("Creating channel {}", name).into()),
            _ => {}
        }

        if self.error_text.is_empty().not() {
            create_widgets.push(label!(&self.error_text).color(theme.user_theme.error).into());
        }
        create_widgets.push(
            Row::with_children(vec![
                is_category.width(length!(= 110)).into(),
                create_text_edit.into(),
                create.width(length!(= 80)).into(),
            ])
            .align_items(Align::Center)
            .spacing(SPACING * 2)
            .into(),
        );

        Container::new(
            Card::new(
                label!("Create channel").width(length!(= 490 + ((SPACING * 2) + SPACING) + PADDING)),
                column(create_widgets),
            )
            .style(theme.round())
            .on_close(Message::GoBack),
        )
        .style(theme.round().border_width(0.0))
        .center_x()
        .center_y()
        .into()
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> (Command<TopLevelMessage>, bool) {
        let mut go_back = false;
        match msg {
            Message::IsCategoryToggle(is_category) => {
                self.is_category = is_category;
            }
            Message::ChannelNameChanged(new_name) => {
                self.channel_name_field = new_name;
            }
            Message::CreateChannel => {
                let channel_name = self.channel_name_field.clone();

                self.error_text.clear();
                self.channel_creation_state = ChannelState::Creating {
                    name: channel_name.clone(),
                };
                let guild_id = self.guild_id;
                let is_category = self.is_category;
                let after = client
                    .guilds
                    .get(&guild_id)
                    .and_then(|g| g.channels.last().map(|(k, _)| *k))
                    .unwrap_or(0);

                return (
                    client.mk_cmd(
                        |inner| async move {
                            let result = inner
                                .call(
                                    CreateChannel::new(guild_id, channel_name, Place::bottom(after))
                                        .with_is_category(is_category),
                                )
                                .await;
                            result.map(|response| {
                                TopLevelMessage::guild_settings(ParentMessage::ChannelCreationMessage(
                                    Message::CreatedChannel {
                                        guild_id,
                                        channel_id: response.channel_id,
                                    },
                                ))
                            })
                        },
                        identity,
                    ),
                    go_back,
                );
            }
            Message::CreatedChannel { guild_id, channel_id } => {
                self.channel_creation_state = ChannelState::Created {
                    guild_id,
                    channel_id,
                    name: self.channel_name_field.clone(),
                };
                self.channel_name_field.clear();
            }
            Message::GoBack => {
                self.channel_creation_state = ChannelState::None;
                self.channel_name_field.clear();
                self.error_text.clear();
                go_back = true;
            }
        }

        (Command::none(), go_back)
    }

    pub fn on_error(&mut self, error: &ClientError) -> Command<TopLevelMessage> {
        self.error_text = error.to_string();
        self.channel_creation_state = ChannelState::None;

        Command::none()
    }
}
