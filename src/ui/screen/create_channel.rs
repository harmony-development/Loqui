use harmony_rust_sdk::client::api::chat::channel;

use crate::{
    client::{error::ClientError, Client},
    label, label_button, length, space,
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR, PADDING, SUCCESS_COLOR},
    },
};

#[derive(Clone, Debug)]
pub enum Message {
    ChannelNameChanged(String),
    CreateChannel,
    CreatedChannel { guild_id: u64, channel_id: u64 },
    GoBack,
}

#[derive(Default, Debug)]
pub struct ChannelCreation {
    create_channel_back_but_state: button::State,
    created_channel: Option<(u64, u64)>,
    creating_channel: Option<String>,
    channel_name_textedit_state: text_input::State,
    channel_create_but_state: button::State,
    channel_name: String,
    error_text: String,
    pub guild_id: u64,
}

impl ChannelCreation {
    pub fn view(&mut self, theme: Theme, client: &Client) -> Element<Message> {
        let mut create_text_edit = TextInput::new(
            &mut self.channel_name_textedit_state,
            "Enter a channel name...",
            &self.channel_name,
            Message::ChannelNameChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let mut create = label_button!(&mut self.channel_create_but_state, "Create").style(theme);
        let mut back = label_button!(&mut self.create_channel_back_but_state, "Back").style(theme);

        let mut create_widgets = Vec::with_capacity(3);

        if self.creating_channel.is_none() {
            back = back.on_press(Message::GoBack);

            if !self.channel_name.is_empty() {
                create_text_edit = create_text_edit.on_submit(Message::CreateChannel);
                create = create.on_press(Message::CreateChannel);
            }
        }

        if let Some(name) = self
            .created_channel
            .as_ref()
            .map(|(gid, cid)| {
                client
                    .guilds
                    .get(gid)
                    .map(|r| r.channels.get(cid))
                    .flatten()
            })
            .flatten()
            .map(|channel| &channel.name)
        {
            create_widgets.push(
                label!("Successfully created channel {}", name)
                    .color(SUCCESS_COLOR)
                    .into(),
            );
        }

        if let Some(name) = self.creating_channel.as_ref() {
            create_widgets.push(label!("Creating channel {}", name).into());
        }

        if !self.error_text.is_empty() {
            create_widgets.push(label!(&self.error_text).color(ERROR_COLOR).into());
        }

        create_widgets.push(create_text_edit.into());
        create_widgets.push(
            row(vec![
                create.width(length!(+)).into(),
                space!(w+).into(),
                back.width(length!(+)).into(),
            ])
            .width(length!(+))
            .into(),
        );

        let padded_panel = row(vec![
            space!(w % 3).into(),
            column(create_widgets).width(length!(% 4)).into(),
            space!(w % 3).into(),
        ])
        .width(length!(+));

        fill_container(padded_panel).style(theme).into()
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> Command<super::Message> {
        match msg {
            Message::ChannelNameChanged(new_name) => {
                self.channel_name = new_name;
            }
            Message::CreateChannel => {
                let channel_name = self.channel_name.clone();
                let guild_id = self.guild_id;

                self.error_text.clear();
                self.created_channel = None;
                self.creating_channel = Some(channel_name.clone());
                let inner = client.inner().clone();

                return Command::perform(
                    async move {
                        let result = channel::create_channel(
                            &inner,
                            channel::CreateChannel::new(
                                guild_id,
                                channel_name,
                                harmony_rust_sdk::api::chat::Place::Top { before: 0 },
                            ),
                        )
                        .await;
                        result.map_or_else(
                            |e| super::Message::Error(Box::new(e.into())),
                            |response| {
                                super::Message::ChannelCreation(Message::CreatedChannel {
                                    guild_id,
                                    channel_id: response.channel_id,
                                })
                            },
                        )
                    },
                    |msg| msg,
                );
            }
            Message::CreatedChannel {
                guild_id,
                channel_id,
            } => {
                self.created_channel = Some((guild_id, channel_id));
                self.creating_channel = None;
            }
            Message::GoBack => return Command::perform(async {}, |_| super::Message::PopScreen),
        }

        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<super::Message> {
        self.created_channel = None;
        self.creating_channel = None;
        self.error_text = error.to_string();

        Command::none()
    }
}
