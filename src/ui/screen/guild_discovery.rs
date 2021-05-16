use harmony_rust_sdk::{
    api::chat::InviteId,
    client::api::chat::{guild::AddGuildToGuildListRequest, *},
};

use super::{
    Message as TopLevelMessage,
};

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
    InviteChanged(String),
    GuildNameChanged(String),
    CreateGuild,
    JoinGuild(InviteId),
    JoinedGuild(u64),
    GoBack,
}

#[derive(Default, Debug)]
pub struct GuildDiscovery {
    direct_join_textedit_state: text_input::State,
    direct_join_but_state: button::State,
    join_room_back_but_state: button::State,
    invite: String,
    joined_guild: Option<u64>,
    joining_guild: Option<String>,
    guild_name_textedit_state: text_input::State,
    guild_create_but_state: button::State,
    guild_name: String,
    error_text: String,
}

impl GuildDiscovery {
    pub fn view(&mut self, theme: Theme, client: &Client) -> Element<Message> {
        let mut join_text_edit = TextInput::new(
            &mut self.direct_join_textedit_state,
            "Enter a guild invite...",
            &self.invite,
            Message::InviteChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let mut create_text_edit = TextInput::new(
            &mut self.guild_name_textedit_state,
            "Enter a guild name...",
            &self.guild_name,
            Message::GuildNameChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let mut join = label_button!(&mut self.direct_join_but_state, "Join").style(theme);
        let mut create = label_button!(&mut self.guild_create_but_state, "Create").style(theme);
        let mut back = label_button!(&mut self.join_room_back_but_state, "Back").style(theme);

        let mut texts = Vec::with_capacity(2);
        let mut widgets = Vec::with_capacity(3);
        let mut create_widgets = Vec::with_capacity(3);

        if self.joining_guild.is_none() {
            back = back.on_press(Message::GoBack);

            if !self.guild_name.is_empty() {
                create_text_edit = create_text_edit.on_submit(Message::CreateGuild);
                create = create.on_press(Message::CreateGuild);
            }

            let maybe_invite = InviteId::new(&self.invite).map_or_else(
                || {
                    Err(ClientError::Custom(
                        "Please enter a valid invite".to_string(),
                    ))
                },
                Ok,
            );

            match maybe_invite {
                Ok(invite) => {
                    let msg = Message::JoinGuild(invite);
                    join_text_edit = join_text_edit.on_submit(msg.clone());
                    join = join.on_press(msg);
                }
                Err(e) => {
                    if !self.invite.is_empty() {
                        tracing::debug!("{}", e); // We don't print this as an error since it'll spam the logs
                        texts.push(label!(e.to_string()).color(ERROR_COLOR).into());
                    }
                }
            }
        }

        if let Some(name) = self
            .joined_guild
            .as_ref()
            .map(|id| client.guilds.get(id).map(|r| &r.name))
            .flatten()
        {
            texts.push(
                label!("Successfully joined guild {}", name)
                    .color(SUCCESS_COLOR)
                    .into(),
            );
        }

        if let Some(name) = self.joining_guild.as_ref() {
            texts.push(label!("Joining guild {}", name).into());
        }

        if !self.error_text.is_empty() {
            texts.push(label!(&self.error_text).color(ERROR_COLOR).into());
        }

        create_widgets.push(create_text_edit.into());
        create_widgets.push(
            row(vec![
                space!(w % 3).into(),
                create.width(length!(% 2)).into(),
                space!(w % 3).into(),
            ])
            .into(),
        );
        widgets.push(join_text_edit.into());
        widgets.push(
            row(vec![
                space!(w % 3).into(),
                join.width(length!(% 2)).into(),
                space!(w % 3).into(),
            ])
            .into(),
        );

        let padded_panel = column(vec![
            column(texts).height(length!(-)).into(),
            row(vec![
                column(widgets).width(length!(+)).into(),
                column(create_widgets).width(length!(+)).into(),
            ])
            .width(length!(+))
            .into(),
            row(vec![
                space!(w % 4).into(),
                back.width(length!(% 2)).into(),
                space!(w % 4).into(),
            ])
            .into(),
        ]);

        fill_container(padded_panel).style(theme).into()
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> Command<TopLevelMessage> {
        match msg {
            Message::InviteChanged(new_invite) => {
                self.invite = new_invite;
            }
            Message::GuildNameChanged(new_name) => {
                self.guild_name = new_name;
            }
            Message::CreateGuild => {
                let guild_name = self.guild_name.clone();

                self.joined_guild = None;
                self.joining_guild = Some(guild_name.clone());
                self.error_text.clear();
                let inner = client.inner().clone();

                return Command::perform(
                    async move {
                        let guild_id =
                            guild::create_guild(&inner, guild::CreateGuild::new(guild_name))
                                .await?
                                .guild_id;
                        guild::add_guild_to_guild_list(
                            &inner,
                            AddGuildToGuildListRequest {
                                guild_id,
                                homeserver: inner.homeserver_url().to_string(),
                            },
                        )
                        .await?;
                        Ok(guild_id)
                    },
                    |result| {
                        result.map_or_else(
                            |e| TopLevelMessage::Error(Box::new(e)),
                            |response| {
                                TopLevelMessage::GuildDiscovery(Message::JoinedGuild(response))
                            },
                        )
                    },
                );
            }
            Message::JoinGuild(invite) => {
                self.joined_guild = None;
                self.joining_guild = Some(invite.to_string());
                self.error_text.clear();
                let inner = client.inner().clone();

                return Command::perform(
                    async move {
                        let guild_id = guild::join_guild(&inner, invite).await?.guild_id;
                        guild::add_guild_to_guild_list(
                            &inner,
                            AddGuildToGuildListRequest {
                                guild_id,
                                homeserver: inner.homeserver_url().to_string(),
                            },
                        )
                        .await?;
                        Ok(guild_id)
                    },
                    |result| {
                        result.map_or_else(
                            |e| TopLevelMessage::Error(Box::new(e)),
                            |response| {
                                TopLevelMessage::GuildDiscovery(Message::JoinedGuild(response))
                            },
                        )
                    },
                );
            }
            Message::JoinedGuild(room_id) => {
                self.joined_guild = Some(room_id);
                self.joining_guild = None;
            }
            Message::GoBack => return Command::perform(async {}, |_| TopLevelMessage::PopScreen),
        }

        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.joined_guild = None;
        self.joining_guild = None;
        self.error_text = error.to_string();

        Command::none()
    }
}
