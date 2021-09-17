use super::{
    super::{Message as TopLevelMessage, Screen as TopLevelScreen},
    GuildMetadata,
};
use crate::{
    client::Client,
    component::*,
    label, label_button, length,
    screen::{
        guild_settings::{Message as ParentMessage, Tab},
        select_upload_files,
    },
    style::{Theme, PADDING, PROFILE_AVATAR_WIDTH},
};
use crate::{
    screen::{guild_settings::TabLabel, ClientExt},
    style::DEF_SIZE,
};
use client::{
    error::{ClientError, ClientResult},
    harmony_rust_sdk::{
        api::chat::all_permissions::GUILD_MANAGE_CHANGE_INFORMATION, client::api::chat::guild::UpdateGuildInformation,
    },
};
use iced::Tooltip;
use iced_aw::Icon;

#[derive(Debug, Clone)]
pub enum GeneralMessage {
    NameChanged(String),
    NameButPressed,
    NameButSuccess,
    GoBack,
    UploadGuildImage,
}

#[derive(Debug, Default, Clone)]
pub struct GeneralTab {
    name_edit_state: text_input::State,
    name_edit_field: String,
    name_edit_but_state: button::State,
    icon_edit_but_state: button::State,
    back_but_state: button::State,
    name_but_state: button::State,
    id_but_state: button::State,
    loading_text: Option<String>,
    pub error_message: String,
}

impl GeneralTab {
    pub fn update(
        &mut self,
        message: GeneralMessage,
        client: &Client,
        _: &mut GuildMetadata,
        guild_id: u64,
    ) -> Command<TopLevelMessage> {
        match message {
            GeneralMessage::NameChanged(text) => {
                self.name_edit_field = text;
            }
            GeneralMessage::NameButPressed => {
                //Show loading text via a Option
                self.loading_text = Some("Updating...".to_string());
                let current_name = self.name_edit_field.clone();
                return client.mk_cmd(
                    |inner| async move {
                        // Build the GuildInformationRequest and update the Name
                        inner
                            .call(UpdateGuildInformation::new(guild_id).with_new_guild_name(current_name))
                            .await
                    },
                    |_| TopLevelMessage::guild_settings(ParentMessage::General(GeneralMessage::NameButSuccess)),
                );
            }
            GeneralMessage::NameButSuccess => {
                self.name_edit_field.clear();
                self.loading_text = Some("Name updated!".to_string());
            }
            GeneralMessage::UploadGuildImage => {
                let content_store = client.content_store_arc();
                return client.mk_cmd(
                    |inner| async move {
                        // Select new Guild image and Upload
                        let id = select_upload_files(&inner, content_store, true).await?.remove(0).id;
                        ClientResult::Ok(
                            inner
                                .call(UpdateGuildInformation::new(guild_id).with_new_guild_picture(Some(id)))
                                .await?,
                        )
                    },
                    |_| TopLevelMessage::Nothing,
                );
            }
            GeneralMessage::GoBack => {
                return TopLevelScreen::pop_screen_cmd();
            }
        }
        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_message = error.to_string();
        Command::none()
    }
}

impl Tab for GeneralTab {
    type Message = ParentMessage;

    fn title(&self) -> String {
        String::from("General")
    }

    fn tab_label(&self) -> TabLabel {
        TabLabel::IconText(Icon::Gear.into(), self.title())
    }

    fn content(
        &mut self,
        client: &Client,
        guild_id: u64,
        _: &mut GuildMetadata,
        theme: &Theme,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<'_, ParentMessage> {
        let name_edit_but_state = &mut self.name_edit_but_state;
        let guild = client.guilds.get(&guild_id).unwrap();

        let ui_update_guild_icon = fill_container(
            guild
                .picture
                .as_ref()
                .map(|guild_picture| {
                    thumbnail_cache
                        .profile_avatars
                        .get(guild_picture)
                        .or_else(|| thumbnail_cache.thumbnails.get(guild_picture))
                })
                .flatten()
                .map_or_else(
                    || Element::from(label!(guild.name.chars().next().unwrap_or('u').to_ascii_uppercase()).size(30)),
                    |handle| Element::from(Image::new(handle.clone())),
                ),
        );
        let mut ui_image_but = Button::new(&mut self.icon_edit_but_state, ui_update_guild_icon)
            .height(length!(= PROFILE_AVATAR_WIDTH))
            .width(length!(= PROFILE_AVATAR_WIDTH))
            .style(theme);
        if guild.has_perm(GUILD_MANAGE_CHANGE_INFORMATION) {
            ui_image_but = ui_image_but.on_press(GeneralMessage::UploadGuildImage);
        }
        let ui_image_but = Element::from(
            Tooltip::new(
                ui_image_but,
                "Click to upload a new image",
                iced::tooltip::Position::Top,
            )
            .style(theme),
        )
        .map(ParentMessage::General);

        let back = Element::from(
            label_button!(&mut self.back_but_state, "Back")
                .style(theme)
                .on_press(GeneralMessage::GoBack),
        )
        .map(ParentMessage::General);

        let mut content = Vec::with_capacity(5);
        if !self.error_message.is_empty() {
            content.push(
                label!(&self.error_message)
                    .color(theme.user_theme.error)
                    .size(DEF_SIZE + 2)
                    .into(),
            );
        }
        if let Some(ldg_text) = &self.loading_text {
            content.push(label!(ldg_text).into());
        }
        content.push(
            row(vec![
                Tooltip::new(
                    label_button!(&mut self.name_but_state, &guild.name)
                        .on_press(ParentMessage::CopyToClipboard(guild.name.clone()))
                        .style(theme),
                    "Click to copy",
                    iced::tooltip::Position::Top,
                )
                .style(theme)
                .into(),
                Tooltip::new(
                    label_button!(&mut self.id_but_state, format!("ID {}", guild_id))
                        .on_press(ParentMessage::CopyIdToClipboard(guild_id))
                        .style(theme),
                    "Click to copy",
                    iced::tooltip::Position::Top,
                )
                .style(theme)
                .into(),
            ])
            .into(),
        );
        content.push(ui_image_but);
        if guild.has_perm(GUILD_MANAGE_CHANGE_INFORMATION) {
            content.push(
                row(vec![
                    Element::from(
                        TextInput::new(
                            &mut self.name_edit_state,
                            "Enter a new name...",
                            self.name_edit_field.as_str(),
                            GeneralMessage::NameChanged,
                        )
                        .style(theme)
                        .padding(PADDING / 2)
                        .width(length!(= 300)),
                    )
                    .map(ParentMessage::General),
                    Element::from(
                        label_button!(name_edit_but_state, "Update")
                            .on_press(GeneralMessage::NameButPressed)
                            .style(theme),
                    )
                    .map(ParentMessage::General),
                ])
                .into(),
            );
        }
        content.push(back);

        column(content).into()
    }
}
