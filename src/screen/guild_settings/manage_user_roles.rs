use super::super::Message as TopLevelMessage;
use client::harmony_rust_sdk::client::api::chat::permissions::{ManageUserRoles, ManageUserRolesSelfBuilder};
use iced::{tooltip::Position, Tooltip};
use iced_aw::Card;

use crate::{
    client::Client,
    component::*,
    label, length,
    screen::{map_to_nothing, ClientExt},
    style::{tuple_to_iced_color, Theme, DEF_SIZE, PADDING, SPACING},
};

#[derive(Debug, Clone)]
pub enum Message {
    GoBack,
    RoleChange { role_id: u64, give_or_take: bool },
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
                    let is_given = user_roles.contains(role_id);
                    let (tooltip, tooltip_position) = is_given
                        .then(|| ("Remove role from user", Position::Right))
                        .unwrap_or(("Give role to user", Position::Left));
                    let role_color = tuple_to_iced_color(role.color);
                    let role_but = Tooltip::new(
                        Button::new(but_state, label!(role.name.as_str()).color(role_color))
                            .on_press(Message::RoleChange {
                                role_id: *role_id,
                                give_or_take: !is_given,
                            })
                            .style(theme.background_color(Color { a: 0.2, ..role_color })),
                        tooltip,
                        tooltip_position,
                    )
                    .style(theme)
                    .size(DEF_SIZE - 2)
                    .gap(SPACING * 2);
                    if is_given {
                        given_roles = given_roles.push(role_but);
                    } else {
                        taken_roles = taken_roles.push(role_but);
                    }
                }
            }
        }

        widgets.push(
            Column::with_children(vec![
                label!("Given").into(),
                given_roles.height(length!(+)).width(length!(+)).into(),
            ])
            .width(length!(+))
            .height(length!(+))
            .align_items(Align::Center)
            .into(),
        );
        widgets.push(Rule::vertical(SPACING * 2).style(theme).into());
        widgets.push(
            Column::with_children(vec![
                label!("Not given").into(),
                taken_roles.height(length!(+)).width(length!(+)).into(),
            ])
            .width(length!(+))
            .height(length!(+))
            .align_items(Align::Center)
            .into(),
        );

        Container::new(
            Card::new(
                label!("Manage user roles").width(length!(= 600 - PADDING - SPACING)),
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

    pub fn update(&mut self, message: Message, client: &Client, guild_id: u64) -> (Command<TopLevelMessage>, bool) {
        (
            match message {
                Message::GoBack => return (Command::none(), true),
                Message::RoleChange { role_id, give_or_take } => {
                    let user_id = self.user_id;
                    client.mk_cmd(
                        |inner| async move {
                            let mut request = ManageUserRoles::new(guild_id, user_id);
                            if give_or_take {
                                request = request.give_role_ids(vec![role_id]);
                            } else {
                                request = request.take_role_ids(vec![role_id]);
                            }
                            inner.chat().await.manage_user_roles(request).await
                        },
                        map_to_nothing,
                    )
                }
            },
            false,
        )
    }
}
