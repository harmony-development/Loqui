use crate::{
    client::{error::ClientError, Client},
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR, PADDING, SUCCESS_COLOR},
    },
};
use ruma::{RoomId, RoomIdOrAliasId};

#[derive(Clone, Debug)]
pub enum Message {
    DirectJoinRoomIdOrAliasChanged(String),
    JoinRoom(RoomIdOrAliasId),
    JoinedRoom(RoomId),
    GoBack,
}

#[derive(Default, Debug)]
pub struct RoomDiscovery {
    direct_join_textedit_state: text_input::State,
    direct_join_but_state: button::State,
    join_room_back_but_state: button::State,
    room_join_alias_or_id: String,
    joined_room: Option<RoomId>,
    joining_room: Option<String>,
}

impl RoomDiscovery {
    pub fn view(&mut self, theme: Theme, client: &Client) -> Element<Message> {
        let mut text_edit = TextInput::new(
            &mut self.direct_join_textedit_state,
            "Enter a room ID or alias...",
            &self.room_join_alias_or_id,
            Message::DirectJoinRoomIdOrAliasChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let mut join = label_button(&mut self.direct_join_but_state, "Join").style(theme);

        let back = label_button(&mut self.join_room_back_but_state, "Back")
            .style(theme)
            .on_press(Message::GoBack);

        let mut widgets = Vec::with_capacity(3);

        let maybe_room_alias_or_id = self
            .room_join_alias_or_id
            .parse::<RoomIdOrAliasId>()
            .map_err(|e| {
                ClientError::Custom(format!("Please enter a valid room alias or ID: {}", e))
            });

        match maybe_room_alias_or_id {
            Ok(alias_or_id) => {
                let msg = Message::JoinRoom(alias_or_id);
                text_edit = text_edit.on_submit(msg.clone());
                join = join.on_press(msg);
            }
            Err(e) => {
                if !self.room_join_alias_or_id.is_empty() {
                    log::debug!("{}", e); // We don't print this as an error since it'll spam the logs
                    widgets.push(label(e.to_string()).color(ERROR_COLOR).into());
                }
            }
        }

        if let Some(name) = self
            .joined_room
            .as_ref()
            .map(|id| client.rooms.get(id).map(|r| r.get_display_name()))
            .flatten()
        {
            widgets.push(
                label(format!("Successfully joined room {}", name))
                    .color(SUCCESS_COLOR)
                    .into(),
            );
        }

        if let Some(name) = self.joining_room.as_ref() {
            widgets.push(label(format!("Joining room {}", name)).into());
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
            Message::DirectJoinRoomIdOrAliasChanged(new_join_room_alias_or_id) => {
                self.room_join_alias_or_id = new_join_room_alias_or_id;
            }
            Message::JoinRoom(room_alias_or_id) => {
                self.joined_room = None;
                self.joining_room = Some(room_alias_or_id.to_string());
                return Command::perform(
                    Client::join_room(client.inner(), room_alias_or_id),
                    |result| match result {
                        Ok(response) => super::Message::RoomDiscoveryScreen(Message::JoinedRoom(
                            response.room_id,
                        )),
                        Err(e) => super::Message::MatrixError(Box::new(e)),
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
