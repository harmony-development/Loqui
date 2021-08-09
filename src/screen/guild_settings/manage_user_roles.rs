use std::{convert::identity, ops::Not};

use super::{
    super::{Message as TopLevelMessage, Screen as TopLevelScreen},
    Message as ParentMessage,
};
use client::{
    error::ClientResult,
    harmony_rust_sdk::{
        api::chat::Role,
        client::api::chat::permissions::{
            self, AddGuildRole, DeleteGuildRole, ManageUserRoles, ManageUserRolesSelfBuilder, ModifyGuildRole,
            ModifyGuildRoleSelfBuilder,
        },
    },
};
use iced_aw::{number_input, Card};

use crate::{
    client::{error::ClientError, Client},
    component::*,
    label, label_button, length,
    screen::{map_to_nothing, ClientExt},
    style::{Theme, ERROR_COLOR, PADDING, SPACING, SUCCESS_COLOR},
};

#[derive(Debug, Clone)]
pub enum Message {
    GoBack,
    RoleChange {
        guild_id: u64,
        role_id: u64,
        user_id: u64,
        give_or_take: bool,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ManageUserRolesModal {
    but_states: Vec<button::State>,
    given_roles_state: scrollable::State,
    taken_roles_state: scrollable::State,
    pub user_id: u64,
}

impl ManageUserRolesModal {
    pub fn view(&mut self, theme: Theme, client: &Client, guild_id: u64) -> Element<Message> {
        let mut widgets = Vec::with_capacity(3);

        let mut given_roles = Scrollable::new(&mut self.given_roles_state)
            .style(theme)
            .align_items(Align::Start)
            .spacing(SPACING)
            .padding(PADDING / 2);
        let mut taken_roles = Scrollable::new(&mut self.taken_roles_state)
            .style(theme)
            .align_items(Align::End)
            .spacing(SPACING)
            .padding(PADDING / 2);

        if let Some(guild) = client.guilds.get(&guild_id) {
            if let Some(user_roles) = guild.members.get(&self.user_id) {
                self.but_states.resize_with(guild.roles.len(), Default::default);
                for ((role_id, role), but_state) in guild.roles.iter().zip(self.but_states.iter_mut()) {
                    let role_color = Color::from_rgb8(role.color.0, role.color.1, role.color.2);
                    if user_roles.contains(role_id) {
                        given_roles = given_roles.push(
                            Button::new(but_state, label!(role.name.as_str()).color(role_color))
                                .on_press(Message::RoleChange {
                                    guild_id,
                                    role_id: *role_id,
                                    user_id: self.user_id,
                                    give_or_take: false,
                                })
                                .style(theme.background_color(Color { a: 0.2, ..role_color })),
                        );
                    } else {
                        taken_roles = taken_roles.push(
                            Button::new(but_state, label!(role.name.as_str()).color(role_color))
                                .on_press(Message::RoleChange {
                                    guild_id,
                                    role_id: *role_id,
                                    user_id: self.user_id,
                                    give_or_take: true,
                                })
                                .style(theme.background_color(Color { a: 0.2, ..role_color })),
                        );
                    }
                }
            }
        }

        widgets.push(
            Column::with_children(vec![label!("Given").into(), space!(h+).into(), given_roles.into()])
                .width(length!(+))
                .align_items(Align::Center)
                .into(),
        );
        widgets.push(Rule::vertical(SPACING * 2).style(theme).into());
        widgets.push(
            Column::with_children(vec![label!("Others").into(), space!(h+).into(), taken_roles.into()])
                .width(length!(+))
                .align_items(Align::Center)
                .into(),
        );

        Container::new(
            Card::new(
                label!("Manage user roles").width(length!(= 600 - PADDING - SPACING * 2)),
                row(widgets).width(length!(= 600)).height(length!(= 600)),
            )
            .style(theme.round())
            .on_close(Message::GoBack),
        )
        .style(theme.round().border_width(0.0))
        .center_x()
        .center_y()
        .into()
    }

    pub fn update(&mut self, message: Message, client: &Client) -> (Command<TopLevelMessage>, bool) {
        (
            match message {
                Message::GoBack => return (Command::none(), true),
                Message::RoleChange {
                    guild_id,
                    role_id,
                    user_id,
                    give_or_take,
                } => client.mk_cmd(
                    |inner| async move {
                        let mut request = ManageUserRoles::new(guild_id, user_id);
                        if give_or_take {
                            request = request.give_role_ids(vec![role_id]);
                        } else {
                            request = request.take_role_ids(vec![role_id]);
                        }
                        permissions::manage_user_roles(&inner, request).await
                    },
                    map_to_nothing,
                ),
            },
            false,
        )
    }
}
