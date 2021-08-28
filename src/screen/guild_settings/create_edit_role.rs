use std::{convert::identity, ops::Not};

use super::{super::Message as TopLevelMessage, Message as ParentMessage};
use client::{
    error::ClientResult,
    harmony_rust_sdk::{
        api::chat::Role,
        client::api::chat::permissions::{AddGuildRole, DeleteGuildRole, ModifyGuildRole, ModifyGuildRoleSelfBuilder},
    },
};
use iced_aw::Card;

use crate::{
    client::{error::ClientError, Client},
    component::*,
    label, label_button, length,
    screen::ClientExt,
    style::{Theme, ERROR_COLOR, PADDING, SPACING, SUCCESS_COLOR},
};

#[derive(Debug, Clone)]
pub enum RoleState {
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

impl Default for RoleState {
    fn default() -> Self {
        RoleState::None
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    RoleNameChanged(String),
    CreateRole,
    DeleteRole,
    CreatedRole { guild_id: u64, role_id: u64 },
    GoBack,
    HoistToggle(bool),
    PingableToggle(bool),
}

#[derive(Default, Debug, Clone)]
pub struct RoleModal {
    role_back_but_state: button::State,
    role_name_textedit_state: text_input::State,
    role_create_but_state: button::State,
    role_delete_but_state: button::State,
    role_creation_state: RoleState,
    color_but_state: button::State,
    pub role_name_field: String,
    error_text: String,
    pub is_hoist: bool,
    pub is_pingable: bool,
    pub guild_id: u64,
    pub editing: Option<u64>,
}

impl RoleModal {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        let mut create_text_edit = TextInput::new(
            &mut self.role_name_textedit_state,
            "Enter a role name...",
            &self.role_name_field,
            Message::RoleNameChanged,
        )
        .padding(PADDING / 2)
        .width(length!(= 300))
        .style(theme);

        let create_keyword = self.editing.is_some().then(|| "Edit").unwrap_or("Create");
        let mut create = label_button!(&mut self.role_create_but_state, create_keyword).style(theme);

        let is_hoise = Checkbox::new(self.is_hoist, "Hoist", Message::HoistToggle).style(theme);
        let is_pingable = Checkbox::new(self.is_pingable, "Pingable", Message::PingableToggle).style(theme);

        if let RoleState::None | RoleState::Created { .. } = &self.role_creation_state {
            if self.role_name_field.is_empty().not() {
                create_text_edit = create_text_edit.on_submit(Message::CreateRole);
                create = create.on_press(Message::CreateRole);
            }
        }

        let mut create_widgets = Vec::with_capacity(2);
        match &self.role_creation_state {
            RoleState::Created { name, .. } => {
                let keyword = self.editing.is_some().then(|| "edited").unwrap_or("created");
                create_widgets.push(
                    label!("Successfully {} role {}", keyword, name)
                        .color(SUCCESS_COLOR)
                        .into(),
                );
            }
            RoleState::Creating { name } => {
                let keyword = self.editing.is_some().then(|| "Editing").unwrap_or("Creating");
                create_widgets.push(label!("{} role {}", keyword, name).into())
            }
            _ => {}
        }

        if self.error_text.is_empty().not() {
            create_widgets.push(label!(&self.error_text).color(ERROR_COLOR).into());
        }
        let mut buttons = Vec::with_capacity(2);
        buttons.push(create.width(length!(+)).into());
        if self.editing.is_some() {
            buttons.push(
                label_button!(&mut self.role_delete_but_state, "Delete")
                    .on_press(Message::DeleteRole)
                    .style(theme)
                    .width(length!(+))
                    .into(),
            );
        }
        create_widgets.push(
            Row::with_children(vec![
                Column::with_children(vec![
                    is_hoise.width(length!(+)).into(),
                    is_pingable.width(length!(+)).into(),
                ])
                .spacing(SPACING * 2)
                .max_width(110)
                .align_items(Align::Center)
                .into(),
                create_text_edit.into(),
                Column::with_children(buttons)
                    .spacing(SPACING * 2)
                    .max_width(80)
                    .align_items(Align::Center)
                    .into(),
            ])
            .align_items(Align::Center)
            .spacing(SPACING * 2)
            .into(),
        );

        Container::new(
            Card::new(
                label!("{} role", create_keyword).width(length!(= 490 + ((SPACING * 2) + SPACING) + PADDING)),
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
            Message::HoistToggle(hoist) => self.is_hoist = hoist,
            Message::RoleNameChanged(new_name) => self.role_name_field = new_name,
            Message::CreateRole => {
                let role_name = self.role_name_field.clone();

                self.error_text.clear();
                self.role_creation_state = RoleState::Creating {
                    name: role_name.clone(),
                };
                let guild_id = self.guild_id;
                let hoist = self.is_hoist;
                let pingable = self.is_pingable;
                let editing = self.editing;

                return (
                    client.mk_cmd(
                        |inner| async move {
                            let mut chat = inner.chat().await;
                            let role_id = match editing {
                                Some(role_id) => {
                                    chat.modify_guild_role(
                                        ModifyGuildRole::new(guild_id, role_id)
                                            .new_hoist(hoist)
                                            .new_pingable(pingable)
                                            .new_name(role_name),
                                    )
                                    .await?;
                                    role_id
                                }
                                None => {
                                    chat.add_guild_role(AddGuildRole::new(
                                        guild_id,
                                        Role {
                                            name: role_name,
                                            hoist,
                                            pingable,
                                            ..Default::default()
                                        },
                                    ))
                                    .await?
                                    .role_id
                                }
                            };
                            ClientResult::Ok(TopLevelMessage::guild_settings(ParentMessage::RoleMessage(
                                Message::CreatedRole { guild_id, role_id },
                            )))
                        },
                        identity,
                    ),
                    go_back,
                );
            }
            Message::DeleteRole => {
                if let Some(role_id) = self.editing {
                    self.error_text.clear();
                    let guild_id = self.guild_id;
                    return (
                        client.mk_cmd(
                            |inner| async move {
                                inner
                                    .chat()
                                    .await
                                    .delete_guild_role(DeleteGuildRole::new(guild_id, role_id))
                                    .await
                            },
                            |_| TopLevelMessage::guild_settings(ParentMessage::RoleMessage(Message::GoBack)),
                        ),
                        false,
                    );
                }
            }
            Message::CreatedRole {
                guild_id,
                role_id: channel_id,
            } => {
                self.role_creation_state = RoleState::Created {
                    guild_id,
                    channel_id,
                    name: self.role_name_field.clone(),
                };
                if self.editing.is_none() {
                    self.role_name_field.clear();
                    self.is_hoist = false;
                    self.is_pingable = false;
                };
            }
            Message::GoBack => {
                self.role_creation_state = RoleState::None;
                self.role_name_field.clear();
                self.error_text.clear();
                go_back = true;
            }
            Message::PingableToggle(pingable) => self.is_pingable = pingable,
        }

        (Command::none(), go_back)
    }

    pub fn on_error(&mut self, error: &ClientError) -> Command<TopLevelMessage> {
        self.error_text = error.to_string();
        self.role_creation_state = RoleState::None;

        Command::none()
    }
}
