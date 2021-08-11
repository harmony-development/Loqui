use std::ops::Not;

use client::{
    bool_ext::BoolExt,
    harmony_rust_sdk::{api::chat::InviteId, client::api::chat::*},
    tracing::debug,
    OptionExt,
};

use super::{sub_escape_pop_screen, ClientExt, Message as TopLevelMessage, Screen as TopLevelScreen};

use crate::{
    client::{error::ClientError, Client},
    component::*,
    label, label_button, length, space,
    style::{Theme, ERROR_COLOR, PADDING, SUCCESS_COLOR},
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

#[derive(Default, Debug, Clone)]
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

            if self.guild_name.is_empty().not() {
                create_text_edit = create_text_edit.on_submit(Message::CreateGuild);
                create = create.on_press(Message::CreateGuild);
            }

            let maybe_invite = InviteId::new(&self.invite).map_or_else(
                || Err(ClientError::Custom("Please enter a valid invite".to_string())),
                Ok,
            );

            match maybe_invite {
                Ok(invite) => {
                    let msg = Message::JoinGuild(invite);
                    join_text_edit = join_text_edit.on_submit(msg.clone());
                    join = join.on_press(msg);
                }
                Err(e) => {
                    self.invite.is_empty().not().and_do(|| {
                        debug!("{}", e); // We don't print this as an error since it'll spam the logs
                        texts.push(label!(e.to_string()).color(ERROR_COLOR).into());
                    });
                }
            }
        }

        self.joined_guild
            .as_ref()
            .and_then(|id| client.guilds.get(id).map(|r| &r.name))
            .and_do(|name| texts.push(label!("Successfully joined guild {}", name).color(SUCCESS_COLOR).into()));

        self.joining_guild
            .as_ref()
            .and_do(|name| texts.push(label!("Joining guild {}", name).into()));

        let err_text = &self.error_text;
        self.error_text
            .is_empty()
            .not()
            .and_do(|| texts.push(label!(err_text).color(ERROR_COLOR).into()));

        create_widgets.push(
            row(vec![
                space!(w % 2).into(),
                create_text_edit.width(length!(% 6)).into(),
                space!(w % 2).into(),
            ])
            .into(),
        );
        create_widgets.push(
            row(vec![
                space!(w % 3).into(),
                create.width(length!(% 2)).into(),
                space!(w % 3).into(),
            ])
            .into(),
        );
        widgets.push(
            row(vec![
                space!(w % 2).into(),
                join_text_edit.width(length!(% 6)).into(),
                space!(w % 2).into(),
            ])
            .into(),
        );
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

                return client.mk_cmd(
                    |inner| async move {
                        guild::create_guild(&inner, guild::CreateGuild::new(guild_name))
                            .await
                            .map(|g| g.guild_id)
                    },
                    |id| TopLevelMessage::guild_discovery(Message::JoinedGuild(id)),
                );
            }
            Message::JoinGuild(invite) => {
                self.joined_guild = None;
                self.joining_guild = Some(invite.to_string());
                self.error_text.clear();

                return client.mk_cmd(
                    |inner| async move { guild::join_guild(&inner, invite).await.map(|e| e.guild_id) },
                    |id| TopLevelMessage::guild_discovery(Message::JoinedGuild(id)),
                );
            }
            Message::JoinedGuild(room_id) => {
                self.joined_guild = Some(room_id);
                self.joining_guild = None;
            }
            Message::GoBack => return TopLevelScreen::pop_screen_cmd(),
        }

        Command::none()
    }

    pub fn subscription(&self) -> Subscription<TopLevelMessage> {
        sub_escape_pop_screen()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.joined_guild = None;
        self.joining_guild = None;
        self.error_text = error.to_string();

        Command::none()
    }
}
