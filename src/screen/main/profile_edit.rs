use client::{
    error::ClientError,
    harmony_rust_sdk::client::api::chat::profile::{profile_update, ProfileUpdate},
};
use iced::Tooltip;
use iced_aw::Card;

use crate::{
    client::Client,
    component::*,
    length,
    screen::{map_to_nothing, ClientExt},
    space,
    style::*,
};

use super::super::{select_upload_files, Message as TopLevelMessage};

#[derive(Debug, Clone)]
pub enum Message {
    UploadPfp,
    ChangeName,
    UpdateNewUsername(String),
    Back,
    IsBotChecked(bool),
    CopyId,
    CopyUsername(String),
}

#[derive(Debug, Default, Clone)]
pub struct ProfileEditModal {
    pub user_id: u64,
    pub guild_id: Option<u64>,
    pub is_edit: bool,
    avatar_but: button::State,
    back_but: button::State,
    username_edit: text_input::State,
    username_change_but: button::State,
    username_but_state: button::State,
    userid_but_state: button::State,
    current_username: String,
}

impl ProfileEditModal {
    pub fn view(&mut self, theme: Theme, client: &Client, thumbnail_cache: &ThumbnailCache) -> Element<Message> {
        const MAX_LENGTH: u16 = 400 + (PADDING * 2) - SPACING;

        let content: Element<Message> = if let Some(user_profile) = client.members.get(&self.user_id) {
            let user_img: Element<Message> = if let Some(handle) = user_profile
                .avatar_url
                .as_ref()
                .and_then(|id| thumbnail_cache.profile_avatars.get(id))
            {
                Image::new(handle.clone()).height(length!(+)).width(length!(+)).into()
            } else {
                fill_container(
                    label!(user_profile.username.chars().next().unwrap_or('U').to_ascii_uppercase())
                        .size((DEF_SIZE * 3) + 4),
                )
                .into()
            };
            let mut avatar_but = Button::new(&mut self.avatar_but, user_img).style(theme);
            if self.is_edit {
                avatar_but = avatar_but.on_press(Message::UploadPfp);
            }
            let username_text = if self.is_edit {
                format!("Hello, {}.", user_profile.username).into()
            } else {
                user_profile.username.clone()
            };
            let user_id = Tooltip::new(
                Button::new(
                    &mut self.userid_but_state,
                    label!(format!("ID {}", self.user_id)).size(DEF_SIZE - 4),
                )
                .on_press(Message::CopyId)
                .style(theme),
                "Click to copy",
                iced::tooltip::Position::Top,
            )
            .style(theme);
            let username = Tooltip::new(
                Button::new(&mut self.username_but_state, label!(username_text).size(DEF_SIZE + 12))
                    .on_press(Message::CopyUsername(user_profile.username.to_string()))
                    .style(theme),
                "Click to copy",
                iced::tooltip::Position::Top,
            )
            .style(theme);
            let mut profile_widgets = Vec::with_capacity(4);
            profile_widgets.push(
                avatar_but
                    .width(length!(= PROFILE_AVATAR_WIDTH))
                    .height(length!(= PROFILE_AVATAR_WIDTH))
                    .into(),
            );
            profile_widgets.push(space!(w+).into());
            let is_bot = if self.is_edit {
                Checkbox::new(user_profile.is_bot, "Bot", Message::IsBotChecked)
                    .style(theme)
                    .into()
            } else if user_profile.is_bot {
                label!("Bot").size(DEF_SIZE + 4).into()
            } else {
                space!(w = 0).into()
            };
            let guild = self.guild_id.and_then(|id| client.guilds.get(&id));
            let roles: Element<Message> = match guild {
                Some(guild) => {
                    let roles = guild.members.get(&self.user_id).map_or_else(Vec::new, |user_roles| {
                        guild
                            .roles
                            .iter()
                            .filter(|(id, _)| user_roles.contains(id))
                            .map(|(_, role)| {
                                let color = tuple_to_iced_color(role.color);
                                Container::new(label!(role.name.as_str()).size(DEF_SIZE - 4).color(color))
                                    .padding(SPACING / 2)
                                    .style(theme.background_color(Color { a: 0.2, ..color }))
                                    .into()
                            })
                            .collect::<Vec<_>>()
                    });
                    Row::with_children(roles)
                        .align_items(Align::Center)
                        .spacing(SPACING)
                        .height(length!(= 48))
                        .into()
                }
                None => space!(h = 48).into(),
            };
            profile_widgets.push(
                Column::with_children(vec![
                    username.into(),
                    space!(h = SPACING * 2).into(),
                    user_id.into(),
                    roles,
                    is_bot,
                ])
                .align_items(Align::End)
                .into(),
            );
            let profile_widgets = row(profile_widgets);

            let mut widgets = Vec::with_capacity(2);
            widgets.push(profile_widgets.into());
            if self.is_edit {
                let username_change_widgets = {
                    let username_field = TextInput::new(
                        &mut self.username_edit,
                        "Enter a new username...",
                        &self.current_username,
                        Message::UpdateNewUsername,
                    )
                    .on_submit(Message::ChangeName)
                    .padding(PADDING / 2)
                    .style(theme);
                    let username_change_but = Button::new(
                        &mut self.username_change_but,
                        label!("Update username").size(DEF_SIZE - 1),
                    )
                    .on_press(Message::ChangeName)
                    .style(theme);

                    row(vec![
                        username_field.width(length!(=256)).into(),
                        space!(w+).into(),
                        username_change_but.into(),
                    ])
                };
                widgets.push(username_change_widgets.into());
            }

            Column::with_children(widgets)
                .max_width((MAX_LENGTH + PADDING + SPACING) as u32)
                .align_items(Align::Start)
                .into()
        } else {
            label!("No profile loaded yet.").into()
        };

        let profile_header_text = if self.is_edit { "Edit profile" } else { "Profile" };
        Container::new(
            Card::new(label!(profile_header_text).width(length!(= MAX_LENGTH)), content)
                .style(theme.round())
                .on_close(Message::Back),
        )
        .style(theme.round().border_width(0.0))
        .center_x()
        .center_y()
        .into()
    }

    pub fn update(
        &mut self,
        msg: Message,
        client: &Client,
        clip: &mut iced::Clipboard,
    ) -> (Command<TopLevelMessage>, bool) {
        (
            match msg {
                Message::IsBotChecked(is_bot) => client.mk_cmd(
                    |inner| async move { profile_update(&inner, ProfileUpdate::default().new_is_bot(is_bot)).await },
                    map_to_nothing,
                ),
                Message::UpdateNewUsername(new) => {
                    self.current_username = new;
                    Command::none()
                }
                Message::ChangeName => {
                    let username = self.current_username.drain(..).collect::<String>();
                    client.mk_cmd(
                        |inner| async move {
                            profile_update(
                                &inner,
                                ProfileUpdate::default().new_username(username),
                            )
                            .await
                        },
                        map_to_nothing,
                    )
                }
                Message::UploadPfp => {
                    let content_store = client.content_store_arc();
                    client.mk_cmd(
                        |inner| async move {
                            let id = select_upload_files(&inner, content_store, true).await?.remove(0).id;
                            profile_update(&inner, ProfileUpdate::default().new_avatar(Some(id)))
                                .await
                                .map_err(ClientError::Internal)
                        },
                        map_to_nothing,
                    )
                }
                Message::Back => return (Command::none(), true),
                Message::CopyId => {
                    clip.write(self.user_id.to_string());
                    Command::none()
                }
                Message::CopyUsername(username) => {
                    clip.write(username);
                    Command::none()
                }
            },
            false,
        )
    }
}
