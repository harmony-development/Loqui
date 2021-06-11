use client::{
    error::ClientError,
    harmony_rust_sdk::client::api::chat::profile::{profile_update, ProfileUpdate},
};
use iced_aw::Card;

use crate::{
    client::Client,
    length, space,
    ui::{
        component::*,
        screen::{map_to_nothing, ClientExt},
        style::*,
    },
};

use super::super::{select_upload_files, Message as TopLevelMessage};

#[derive(Debug, Clone)]
pub enum Message {
    UploadPfp,
    ChangeName,
    UpdateNewUsername(String),
    Back,
}

#[derive(Debug, Default)]
pub struct ProfileEditModal {
    pub user_id: u64,
    pub is_edit: bool,
    avatar_but: button::State,
    back_but: button::State,
    username_edit: text_input::State,
    username_change_but: button::State,
    current_username: String,
}

impl ProfileEditModal {
    pub fn new(user_id: u64) -> Self {
        Self {
            user_id,
            ..Default::default()
        }
    }

    pub fn view(&mut self, theme: Theme, client: &Client, thumbnail_cache: &ThumbnailCache) -> Element<Message> {
        const MAX_LENGTH: u16 = 380 + (PADDING * 2) - SPACING;

        let content: Element<Message> = if let Some(user_profile) = client.members.get(&self.user_id) {
            let user_img: Element<Message> = if let Some(handle) = user_profile
                .avatar_url
                .as_ref()
                .map(|id| thumbnail_cache.get_thumbnail(id))
                .flatten()
            {
                Image::new(handle.clone()).height(length!(+)).width(length!(+)).into()
            } else {
                fill_container(
                    label!(user_profile.username.chars().next().unwrap_or('U').to_ascii_uppercase())
                        .size((DEF_SIZE * 3) + 4),
                )
                .into()
            };
            let mut avatar_but = Button::new(&mut self.avatar_but, user_img)
                .height(length!(+))
                .width(length!(+))
                .style(theme);
            if self.is_edit {
                avatar_but = avatar_but.on_press(Message::UploadPfp);
            }
            let username_text = if self.is_edit {
                format!("Hello, {}.", user_profile.username).into()
            } else {
                user_profile.username.clone()
            };
            let username = label!(username_text).size(DEF_SIZE + 12);
            let status_color = Color {
                a: 0.5,
                ..theme.status_color(user_profile.status)
            };
            let mut profile_widgets = Vec::with_capacity(4);
            profile_widgets.push(
                fill_container(avatar_but)
                    .style(theme.round().with_border_color(status_color))
                    .width(length!(=96))
                    .height(length!(=96))
                    .into(),
            );
            profile_widgets.push(space!(w+).into());
            profile_widgets.push(username.into());
            if !self.is_edit {
                profile_widgets.push(space!(w+).into())
            }
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
                .align_items(align!(|<))
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
        .style(theme.round())
        .center_x()
        .center_y()
        .into()
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> (Command<TopLevelMessage>, bool) {
        (
            match msg {
                Message::UpdateNewUsername(new) => {
                    self.current_username = new;
                    Command::none()
                }
                Message::ChangeName => {
                    let username = self.current_username.drain(..).collect::<String>();
                    client.mk_cmd(
                        |inner| async move { profile_update(&inner, ProfileUpdate::default().new_username(username)).await },
                        map_to_nothing,
                    )
                }
                Message::UploadPfp => {
                    let content_store = client.content_store_arc();
                    client.mk_cmd(
                        |inner| async move {
                            let id = select_upload_files(&inner, content_store).await?.remove(0).0;
                            profile_update(&inner, ProfileUpdate::default().new_avatar(Some(id)))
                                .await
                                .map_err(ClientError::Internal)
                        },
                        map_to_nothing,
                    )
                }
                Message::Back => return (Command::none(), true),
            },
            false,
        )
    }
}
