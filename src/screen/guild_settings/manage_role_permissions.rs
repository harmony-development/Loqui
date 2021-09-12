use super::super::Message as TopLevelMessage;
use client::harmony_rust_sdk::{api::chat::Permission, client::api::chat::permissions::SetPermissions};
use iced_aw::Card;

use crate::{
    client::Client,
    component::*,
    label, label_button, length,
    screen::{map_to_nothing, ClientExt},
    style::{tuple_to_iced_color, Theme, DEF_SIZE, PADDING, SPACING},
};

#[derive(Debug, Clone)]
pub enum Message {
    GoBack,
    SetPerm(Permission, bool),
    NewPermNameChanged(String),
    AddPerm,
}

#[derive(Debug, Clone, Default)]
pub struct ManageRolePermissionsModal {
    delete_but_states: Vec<button::State>,
    perms_state: scrollable::State,
    new_perm_name_state: text_input::State,
    new_perm_add_state: button::State,
    new_perm_name: String,
    pub channel_id: Option<u64>,
    pub role_id: u64,
}

impl ManageRolePermissionsModal {
    pub fn view<'a>(&'a mut self, theme: Theme, client: &Client, guild_id: u64) -> Element<'a, Message> {
        let guild = client.guilds.get(&guild_id);
        let channel = |channel_id| guild.and_then(|g| g.channels.get(&channel_id));
        let mut perms = Scrollable::new(&mut self.perms_state)
            .style(theme)
            .align_items(Align::Start)
            .spacing(SPACING)
            .padding(PADDING / 2)
            .width(length!(+))
            .height(length!(+));

        if let Some(guild) = guild {
            let mk_perm_card = |perm: &Permission, delete_state: &'a mut button::State| {
                let perm = perm.clone();
                let matches = perm.matches.clone();
                Container::new(
                    Row::with_children(vec![
                        label!(&perm.matches).into(),
                        space!(w+).into(),
                        Toggler::new(perm.ok, None, move |set| {
                            Message::SetPerm(
                                Permission {
                                    matches: matches.clone(),
                                    ok: set,
                                },
                                false,
                            )
                        })
                        .width(length!(-))
                        .style(theme)
                        .into(),
                        Button::new(delete_state, icon(Icon::Trash))
                            .style(theme)
                            .on_press(Message::SetPerm(perm, true))
                            .into(),
                    ])
                    .align_items(Align::Center)
                    .spacing(SPACING),
                )
                .padding(PADDING / 2)
                .style(theme)
                .center_x()
                .center_y()
            };

            let role_id = self.role_id;
            let permissions = self.channel_id.map_or_else(
                || guild.role_perms.get(&role_id),
                |channel_id| channel(channel_id).and_then(|c| c.role_perms.get(&role_id)),
            );

            if let Some(permissions) = permissions {
                self.delete_but_states.resize_with(permissions.len(), Default::default);
                for (perm, delete_state) in permissions.iter().zip(self.delete_but_states.iter_mut()) {
                    perms = perms.push(mk_perm_card(perm, delete_state));
                }
            }
        }

        let widgets = vec![
            Row::with_children(vec![
                label!("Role name").into(),
                space!(w+).into(),
                label!("Is allowed").into(),
            ])
            .align_items(Align::Center)
            .into(),
            perms.into(),
            Row::with_children(vec![
                TextInput::new(
                    &mut self.new_perm_name_state,
                    "Enter role name...",
                    &self.new_perm_name,
                    Message::NewPermNameChanged,
                )
                .style(theme)
                .padding(PADDING / 2)
                .into(),
                space!(w+).into(),
                label_button!(&mut self.new_perm_add_state, "Add perm")
                    .on_press(Message::AddPerm)
                    .style(theme)
                    .into(),
            ])
            .align_items(Align::Center)
            .into(),
        ];

        let role_id = self.role_id;
        let (role_color, role_name) = guild
            .and_then(|g| g.roles.get(&role_id).map(|r| (r.color, r.name.as_str())))
            .unwrap_or(([255, 255, 255], "role deleted?"));
        let color = tuple_to_iced_color(role_color);

        let mut label_widgets = Vec::with_capacity(3);
        label_widgets.push(label!("Manage role permissions for").into());
        label_widgets.push(
            Container::new(label!(role_name).color(color))
                .padding(PADDING / 4)
                .style(theme.background_color(Color { a: 0.2, ..color }))
                .into(),
        );

        if let Some(channel_id) = self.channel_id {
            let channel_name = channel(channel_id).map_or("deleted channel?", |c| c.name.as_str());
            label_widgets.push(label!(format!("on #{}", channel_name)).into());
        }

        Container::new(
            Card::new(
                Row::with_children(label_widgets)
                    .width(length!(= 600 - PADDING - SPACING - (PADDING / 2)))
                    .align_items(Align::Center)
                    .spacing(SPACING),
                column(widgets).width(length!(= 600)).height(length!(= 600)),
            )
            .close_size((DEF_SIZE + (PADDING / 2)) as f32)
            .style(theme)
            .on_close(Message::GoBack),
        )
        .style(theme.border_width(0.0))
        .center_x()
        .center_y()
        .into()
    }

    pub fn update(&mut self, message: Message, client: &Client, guild_id: u64) -> (Command<TopLevelMessage>, bool) {
        (
            match message {
                Message::GoBack => return (Command::none(), true),
                Message::SetPerm(perm, delete) => {
                    let role_id = self.role_id;
                    let mut permissions = client
                        .guilds
                        .get(&guild_id)
                        .and_then(|g| g.role_perms.get(&role_id))
                        .cloned()
                        .unwrap_or_default();

                    let find_perm = || permissions.iter().position(|p| p.matches == perm.matches);

                    if delete {
                        if let Some(pos) = find_perm() {
                            permissions.remove(pos);
                        }
                    } else if let Some(pos) = find_perm() {
                        permissions.remove(pos);
                        permissions.insert(pos, perm);
                    } else {
                        permissions.push(perm);
                    }

                    let channel_id = self.channel_id.unwrap_or(0);

                    client.mk_cmd(
                        |inner| async move {
                            inner
                                .call(
                                    SetPermissions::new(guild_id, role_id)
                                        .with_channel_id(channel_id)
                                        .with_perms_to_give(permissions),
                                )
                                .await
                        },
                        map_to_nothing,
                    )
                }
                Message::NewPermNameChanged(new_name) => {
                    self.new_perm_name = new_name;
                    Command::none()
                }
                Message::AddPerm => {
                    let matches = self.new_perm_name.drain(..).collect();
                    self.update(
                        Message::SetPerm(Permission { matches, ok: true }, false),
                        client,
                        guild_id,
                    )
                    .0
                }
            },
            false,
        )
    }
}
