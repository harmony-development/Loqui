use harmony_rust_sdk::client::api::chat::profile::{profile_update, ProfileUpdate};
use iced_aw::Card;

use crate::{
    client::{content::ThumbnailCache, Client},
    label_button, length,
    ui::{component::*, style::*},
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

    pub fn view(
        &mut self,
        theme: Theme,
        client: &Client,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<Message> {
        let content: Element<Message> = if let Some(user_profile) =
            client.members.get(&self.user_id)
        {
            let user_img: Element<Message> = if let Some(handle) = user_profile
                .avatar_url
                .as_ref()
                .map(|id| thumbnail_cache.get_thumbnail(id))
                .flatten()
            {
                Image::new(handle.clone())
                    .height(length!(+))
                    .width(length!(+))
                    .into()
            } else {
                label!(user_profile
                    .username
                    .chars()
                    .next()
                    .unwrap_or('U')
                    .to_ascii_uppercase())
                .size((DEF_SIZE * 3) + 4)
                .into()
            };
            let avatar_but = Button::new(&mut self.avatar_but, fill_container(user_img))
                .on_press(Message::UploadPfp)
                .style(theme);
            let username = label!(format!("Hello, {}.", user_profile.username)).size(DEF_SIZE + 12);
            let username_field = TextInput::new(
                &mut self.username_edit,
                "Enter a new username...",
                &self.current_username,
                Message::UpdateNewUsername,
            )
            .on_submit(Message::ChangeName)
            .padding(PADDING / 2)
            .style(theme);
            let username_change_but =
                label_button!(&mut self.username_change_but, "Change username")
                    .on_press(Message::ChangeName)
                    .style(theme);
            let content = Column::with_children(vec![
                row(vec![
                    avatar_but.width(length!(=96)).height(length!(=96)).into(),
                    username.into(),
                ])
                .into(),
                row(vec![
                    username_field.width(length!(=256)).into(),
                    username_change_but.into(),
                ])
                .into(),
            ])
            .align_items(align!(|<));
            content.into()
        } else {
            label!("No profile loaded yet.").into()
        };

        Container::new(
            Card::new(
                label!("Edit profile").width(length!(= 380 + (PADDING * 2) - SPACING)),
                content,
            )
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
                    let inner = client.inner().clone();
                    let username = self.current_username.drain(..).collect::<String>();
                    Command::perform(
                        async move {
                            Ok(profile_update(
                                &inner,
                                ProfileUpdate::default().new_username(username),
                            )
                            .await?)
                        },
                        |result| {
                            result.map_or_else(
                                |err| TopLevelMessage::Error(Box::new(err)),
                                |_| TopLevelMessage::Nothing,
                            )
                        },
                    )
                }
                Message::UploadPfp => {
                    let inner = client.inner().clone();
                    let content_store = client.content_store_arc();
                    Command::perform(
                        async move {
                            let id = select_upload_files(&inner, content_store)
                                .await?
                                .remove(0)
                                .0;
                            Ok(profile_update(
                                &inner,
                                ProfileUpdate::default().new_avatar(Some(id)),
                            )
                            .await?)
                        },
                        |result| {
                            result.map_or_else(
                                |err| TopLevelMessage::Error(Box::new(err)),
                                |_| TopLevelMessage::Nothing,
                            )
                        },
                    )
                }
                Message::Back => return (Command::none(), true),
            },
            false,
        )
    }
}
