use client::{
    error::ClientError,
    harmony_rust_sdk::{
        api::chat::{
            all_permissions::{PERMISSIONS_MANAGE_SET, ROLES_MANAGE},
            color, Place,
        },
        client::api::chat::permissions::{ModifyGuildRole, MoveRole},
    },
    smol_str::SmolStr,
    Client,
};
use iced::Tooltip;
use iced_aw::{color_picker, ColorPicker, TabLabel};

use super::{GuildMetadata, Tab};
use crate::{
    component::*,
    label_button, length,
    screen::{
        guild_settings::Message as ParentMessage, map_to_nothing, ClientExt, Message as TopLevelMessage,
        Screen as TopLevelScreen,
    },
    space,
    style::{tuple_to_iced_color, Theme, PADDING, SPACING},
};

use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChannelSelection(Option<(u64, SmolStr)>);

impl Display for ChannelSelection {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0.as_ref() {
            Some((_, name)) => write!(f, "#{}", name),
            None => write!(f, "Guild"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum RolesMessage {
    MoveRole { id: u64, new_place: Place },
    GoBack,
    ShowColorPicker(usize, bool),
    SetColor { role_id: u64, color: Color },
    SelectedChannel(ChannelSelection),
}

#[derive(Debug, Default)]
pub struct RolesTab {
    #[allow(clippy::type_complexity)]
    button_states: Vec<(
        button::State,
        button::State,
        button::State,
        button::State,
        button::State,
        button::State,
        color_picker::State,
        button::State,
    )>,
    role_list_state: scrollable::State,
    back_but_state: button::State,
    create_role_state: button::State,
    channel_select_state: pick_list::State<ChannelSelection>,
    manage_perms_on_channel: ChannelSelection,
    pub error_message: String,
}

impl Clone for RolesTab {
    fn clone(&self) -> Self {
        Self {
            button_states: {
                let mut vec = Vec::new();
                vec.resize_with(self.button_states.len(), Default::default);
                vec
            },
            role_list_state: self.role_list_state,
            back_but_state: self.back_but_state,
            create_role_state: self.create_role_state,
            channel_select_state: self.channel_select_state.clone(),
            manage_perms_on_channel: self.manage_perms_on_channel.clone(),
            error_message: self.error_message.clone(),
        }
    }
}

impl RolesTab {
    pub fn update(
        &mut self,
        message: RolesMessage,
        client: &Client,
        _: &mut GuildMetadata,
        guild_id: u64,
    ) -> Command<TopLevelMessage> {
        match message {
            RolesMessage::MoveRole { id, new_place } => client.mk_cmd(
                |inner| async move {
                    inner
                        .chat()
                        .await
                        .move_role(MoveRole::new(guild_id, id, new_place))
                        .await
                },
                map_to_nothing,
            ),
            RolesMessage::GoBack => TopLevelScreen::pop_screen_cmd(),
            RolesMessage::ShowColorPicker(index, state) => {
                self.button_states[index].6.show(state);
                Command::none()
            }
            RolesMessage::SetColor { role_id, color } => client.mk_cmd(
                |inner| async move {
                    inner
                        .call(
                            ModifyGuildRole::new(guild_id, role_id).with_new_color(color::encode_rgb([
                                (color.r * 255.0) as u8,
                                (color.g * 255.0) as u8,
                                (color.b * 255.0) as u8,
                            ])),
                        )
                        .await
                },
                map_to_nothing,
            ),
            RolesMessage::SelectedChannel(selected) => {
                self.manage_perms_on_channel = selected;
                Command::none()
            }
        }
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_message = error.to_string();
        Command::none()
    }
}

impl Tab for RolesTab {
    type Message = ParentMessage;

    fn title(&self) -> String {
        String::from("Roles")
    }

    fn tab_label(&self) -> TabLabel {
        TabLabel::IconText(Icon::ListStars.into(), self.title())
    }

    fn content(
        &mut self,
        client: &Client,
        guild_id: u64,
        _: &mut GuildMetadata,
        theme: &Theme,
        _: &ThumbnailCache,
    ) -> Element<'_, ParentMessage> {
        let mut roles = Scrollable::new(&mut self.role_list_state)
            .align_items(Align::Start)
            .height(length!(+))
            .width(length!(+))
            .padding(PADDING)
            .spacing(SPACING)
            .style(theme);

        let guild = client.guilds.get(&guild_id);
        if let Some(guild) = guild {
            self.button_states.resize_with(guild.roles.len(), Default::default);
            for (
                (role_id, role),
                (
                    up_state,
                    down_state,
                    edit_state,
                    copy_state,
                    copy_name_state,
                    color_but_state,
                    color_picker_state,
                    manage_perms_state,
                ),
            ) in guild.roles.iter().zip(&mut self.button_states)
            {
                let role_index = guild.roles.get_index_of(role_id).unwrap();
                let role_id = *role_id;

                let up_place = guild.roles.get_index(role_index.wrapping_sub(2)).map(|(id, _)| *id);
                let down_place = guild.roles.get_index(role_index + 2).map(|(id, _)| *id);

                let mk_place = |id, id_after| match (id, id_after) {
                    (Some(before), Some(after)) => (before != after).then(|| Place::between(before, after)),
                    (Some(_), None) => (role_index != guild.roles.len().saturating_sub(1)).then(Place::top),
                    (None, Some(_)) => (role_index != 0).then(Place::bottom),
                    (None, None) => None,
                };
                let mut up_but = Button::new(up_state, icon(Icon::ArrowUp)).style(theme);
                if let Some(new_place) = mk_place(up_place, Some(role_id)) {
                    up_but = up_but.on_press(ParentMessage::Roles(RolesMessage::MoveRole { id: role_id, new_place }));
                }
                let mut down_but = Button::new(down_state, icon(Icon::ArrowDown)).style(theme);
                if let Some(new_place) = mk_place(Some(role_id), down_place) {
                    down_but =
                        down_but.on_press(ParentMessage::Roles(RolesMessage::MoveRole { id: role_id, new_place }));
                }

                let mut content_widgets = Vec::with_capacity(6);
                if role.hoist {
                    content_widgets.push(
                        Tooltip::new(icon(Icon::List), "Hoistable", iced::tooltip::Position::Top)
                            .style(theme)
                            .into(),
                    );
                }
                if role.pingable {
                    content_widgets.push(
                        Tooltip::new(icon(Icon::At), "Pingable", iced::tooltip::Position::Top)
                            .style(theme)
                            .into(),
                    );
                }
                let role_color = tuple_to_iced_color(role.color);
                content_widgets.push(
                    Tooltip::new(
                        Button::new(copy_name_state, label!(role.name.as_str()).color(role_color))
                            .style(theme.background_color(Color { a: 0.2, ..role_color }))
                            .on_press(ParentMessage::CopyToClipboard(role.name.to_string())),
                        "Click to copy",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
                content_widgets.push(
                    Tooltip::new(
                        label_button!(copy_state, format!("ID {}", role_id))
                            .style(theme)
                            .on_press(ParentMessage::CopyIdToClipboard(role_id)),
                        "Click to copy",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
                content_widgets.push(space!(w+).into());
                if guild.has_perm(PERMISSIONS_MANAGE_SET) {
                    content_widgets.push(
                        Tooltip::new(
                            Button::new(manage_perms_state, icon(Icon::ListCheck))
                                .style(theme)
                                .on_press(ParentMessage::ShowManagePermsModal(
                                    role_id,
                                    self.manage_perms_on_channel.0.as_ref().map(|(id, _)| *id),
                                )),
                            "Manage permissions",
                            iced::tooltip::Position::Top,
                        )
                        .style(theme)
                        .into(),
                    );
                }
                if guild.has_perm(ROLES_MANAGE) {
                    content_widgets.push(
                        ColorPicker::new(
                            color_picker_state,
                            Tooltip::new(
                                Button::new(color_but_state, icon(Icon::Brush))
                                    .style(theme)
                                    .on_press(ParentMessage::Roles(RolesMessage::ShowColorPicker(role_index, true))),
                                "Pick color",
                                iced::tooltip::Position::Top,
                            )
                            .style(theme),
                            ParentMessage::Roles(RolesMessage::ShowColorPicker(role_index, false)),
                            move |color| ParentMessage::Roles(RolesMessage::SetColor { role_id, color }),
                        )
                        .style(theme)
                        .into(),
                    );
                    content_widgets.push(
                        Tooltip::new(
                            Button::new(edit_state, icon(Icon::Pencil))
                                .style(theme)
                                .on_press(ParentMessage::ShowEditRoleModal(role_id)),
                            "Edit role",
                            iced::tooltip::Position::Top,
                        )
                        .style(theme)
                        .into(),
                    );
                    content_widgets.push(
                        Tooltip::new(up_but, "Move up", iced::tooltip::Position::Top)
                            .style(theme)
                            .into(),
                    );
                    content_widgets.push(
                        Tooltip::new(down_but, "Move down", iced::tooltip::Position::Top)
                            .style(theme)
                            .into(),
                    );
                }
                roles = roles.push(Container::new(row(content_widgets)).style(theme));
            }
            if guild.has_perm(ROLES_MANAGE) {
                roles = roles.push(
                    fill_container(
                        label_button!(&mut self.create_role_state, "Create Role")
                            .on_press(ParentMessage::NewRole)
                            .style(theme),
                    )
                    .height(length!(-)),
                );
            }
        }

        let mut content = Vec::with_capacity(4);

        if !self.error_message.is_empty() {
            content.push(label!(self.error_message.as_str()).color(theme.user_theme.error).into())
        }
        let mut options = Vec::with_capacity(guild.map_or(0, |g| g.channels.len()) + 1);
        options.push(ChannelSelection(None));
        if let Some(guild) = guild {
            options.extend(
                guild
                    .channels
                    .iter()
                    .map(|(id, channel)| ChannelSelection(Some((*id, channel.name.clone())))),
            );
            if guild.has_perm(ROLES_MANAGE) {
                content.push(
                    Row::with_children(vec![
                        label!("Manage roles on:").into(),
                        PickList::new(
                            &mut self.channel_select_state,
                            options,
                            Some(self.manage_perms_on_channel.clone()),
                            |selected| ParentMessage::Roles(RolesMessage::SelectedChannel(selected)),
                        )
                        .style(theme)
                        .into(),
                    ])
                    .align_items(Align::Center)
                    .spacing(SPACING)
                    .into(),
                );
            }
        }
        content.push(fill_container(roles).style(theme).into());
        content.push(
            label_button!(&mut self.back_but_state, "Back")
                .on_press(ParentMessage::Roles(RolesMessage::GoBack))
                .style(theme)
                .into(),
        );

        Container::new(column(content).align_items(Align::Center))
            .padding(PADDING * 8)
            .into()
    }
}
