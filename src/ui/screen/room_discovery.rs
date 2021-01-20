use harmony_rust_sdk::{api::chat::InviteId, client::api::chat::*};

use crate::{
    client::{error::ClientError, Client},
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR, PADDING, SUCCESS_COLOR},
    },
};

#[derive(Clone, Debug)]
pub enum Message {
    InviteChanged(String),
    JoinRoom(InviteId),
    JoinedRoom(u64),
    GoBack,
}

#[derive(Default, Debug)]
pub struct RoomDiscovery {
    direct_join_textedit_state: text_input::State,
    direct_join_but_state: button::State,
    join_room_back_but_state: button::State,
    invite: String,
    joined_room: Option<u64>,
    joining_room: Option<String>,
}

impl RoomDiscovery {
    pub fn view(&mut self, theme: Theme, client: &Client) -> Element<Message> {
        let mut text_edit = TextInput::new(
            &mut self.direct_join_textedit_state,
            "Enter a guild invite...",
            &self.invite,
            Message::InviteChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let mut join = label_button(&mut self.direct_join_but_state, "Join").style(theme);

        let back = label_button(&mut self.join_room_back_but_state, "Back")
            .style(theme)
            .on_press(Message::GoBack);

        let mut widgets = Vec::with_capacity(3);

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
                let msg = Message::JoinRoom(invite);
                text_edit = text_edit.on_submit(msg.clone());
                join = join.on_press(msg);
            }
            Err(e) => {
                if !self.invite.is_empty() {
                    log::debug!("{}", e); // We don't print this as an error since it'll spam the logs
                    widgets.push(label(e.to_string()).color(ERROR_COLOR).into());
                }
            }
        }

        if let Some(name) = self
            .joined_room
            .as_ref()
            .map(|id| client.guilds.get(id).map(|r| &r.name))
            .flatten()
        {
            widgets.push(
                label(format!("Successfully joined guild {}", name))
                    .color(SUCCESS_COLOR)
                    .into(),
            );
        }

        if let Some(name) = self.joining_room.as_ref() {
            widgets.push(label(format!("Joining guild {}", name)).into());
        }

        widgets.push(text_edit.into());
        widgets.push(
            row(vec![
                join.width(Length::FillPortion(1)).into(),
                wspace(1).into(),
                back.width(Length::FillPortion(1)).into(),
            ])
            .width(Length::Fill)
            .into(),
        );

        let padded_panel = row(vec![
            wspace(3).into(),
            column(widgets).width(Length::FillPortion(4)).into(),
            wspace(3).into(),
        ])
        .width(Length::Fill);

        fill_container(padded_panel).style(theme).into()
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> Command<super::Message> {
        match msg {
            Message::InviteChanged(new_invite) => {
                self.invite = new_invite;
            }
            Message::JoinRoom(invite) => {
                self.joined_room = None;
                self.joining_room = Some(invite.to_string());
                let inner = client.inner().clone();

                return Command::perform(
                    async move { guild::join_guild(&inner, invite).await },
                    |result| match result {
                        Ok(response) => super::Message::RoomDiscoveryScreen(Message::JoinedRoom(
                            response.guild_id,
                        )),
                        Err(e) => super::Message::MatrixError(Box::new(e.into())),
                    },
                );
            }
            Message::JoinedRoom(room_id) => {
                self.joined_room = Some(room_id);
                self.joining_room = None;
            }
            Message::GoBack => return Command::perform(async {}, |_| super::Message::PopScreen),
        }

        Command::none()
    }
}
