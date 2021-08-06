use std::ops::Not;

use super::{super::Message as TopLevelMessage, Message as ParentMessage};
use client::{
    bool_ext::BoolExt,
    harmony_rust_sdk::{api::chat::DeleteChannelRequest, client::api::chat::channel},
};
use iced_aw::Card;

use crate::{
    client::{error::ClientError, Client},
    component::*,
    label, label_button, length,
    screen::{map_to_nothing, ClientExt},
    style::{Theme, ERROR_COLOR, PADDING, SPACING},
};

#[derive(Clone, Debug)]
pub enum Message {
    ChannelNameChanged(String),
    UpdateChannel,
    DeleteChannel,
    GoBack,
}

#[derive(Default, Debug, Clone)]
pub struct UpdateChannelModal {
    back_but_state: button::State,
    channel_name_textedit_state: text_input::State,
    channel_update_but_state: button::State,
    channel_delete_but_state: button::State,
    pub channel_name_field: String,
    pub guild_id: u64,
    pub channel_id: u64,
    error_text: String,
}

impl UpdateChannelModal {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        let mut name_text_edit = TextInput::new(
            &mut self.channel_name_textedit_state,
            "Enter new channel name...",
            &self.channel_name_field,
            Message::ChannelNameChanged,
        )
        .padding(PADDING / 2)
        .width(length!(= 300))
        .style(theme);

        let mut update = label_button!(&mut self.channel_update_but_state, "Update").style(theme);

        if self.channel_name_field.is_empty().not() {
            name_text_edit = name_text_edit.on_submit(Message::UpdateChannel);
            update = update.on_press(Message::UpdateChannel);
        }

        let mut widgets = Vec::with_capacity(2);

        if self.error_text.is_empty().not() {
            widgets.push(label!(&self.error_text).color(ERROR_COLOR).into());
        }
        widgets.push(
            Row::with_children(vec![
                label!("Channel name").width(length!(= 120)).into(),
                name_text_edit.into(),
                update.width(length!(= 80)).into(),
            ])
            .align_items(Align::Center)
            .spacing(SPACING * 2)
            .into(),
        );
        widgets.push(
            label_button!(&mut self.channel_delete_but_state, "Delete")
                .on_press(Message::DeleteChannel)
                .style(theme)
                .into(),
        );

        let length = length!(= 500 + PADDING + (SPACING * 3));

        Container::new(
            Card::new(label!("Update channel information").width(length), column(widgets))
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
            Message::ChannelNameChanged(new_name) => {
                self.error_text.clear();
                new_name
                    .is_empty()
                    .and_do(|| self.error_text.push_str("Channel name can't be empty"));
                self.channel_name_field = new_name;
            }
            Message::UpdateChannel => {
                let channel_name = self.channel_name_field.clone();

                self.error_text.clear();
                let guild_id = self.guild_id;
                let channel_id = self.channel_id;

                return (
                    client.mk_cmd(
                        |inner| async move {
                            channel::update_channel_information(
                                &inner,
                                channel::UpdateChannelInformation::new(guild_id, channel_id)
                                    .new_channel_name(channel_name),
                            )
                            .await
                        },
                        map_to_nothing,
                    ),
                    go_back,
                );
            }
            Message::DeleteChannel => {
                self.error_text.clear();
                let guild_id = self.guild_id;
                let channel_id = self.channel_id;

                return (
                    client.mk_cmd(
                        |inner| async move {
                            channel::delete_channel(&inner, DeleteChannelRequest { guild_id, channel_id }).await
                        },
                        |_| TopLevelMessage::main(ParentMessage::UpdateChannelMessage(Message::GoBack)),
                    ),
                    false,
                );
            }
            Message::GoBack => {
                self.channel_name_field.clear();
                self.error_text.clear();
                go_back = true;
            }
        }

        (Command::none(), go_back)
    }

    pub fn on_error(&mut self, error: &ClientError) -> Command<TopLevelMessage> {
        self.error_text = error.to_string();

        Command::none()
    }
}
