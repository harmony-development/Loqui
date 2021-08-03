use super::{
    super::{Message as TopLevelMessage, Screen as TopLevelScreen},
    GuildMetaData,
};
use crate::{
    client::Client,
    component::*,
    label, label_button, length,
    screen::{
        guild_settings::{Message as ParentMessage, Tab},
        select_upload_files,
    },
    style::{Theme, PADDING},
};
use crate::{
    screen::{guild_settings::TabLabel, ClientExt},
    style::{DEF_SIZE, ERROR_COLOR},
};
use client::{
    error::{ClientError, ClientResult},
    harmony_rust_sdk::client::api::chat::guild::{update_guild_information, UpdateGuildInformation},
};
use iced_aw::Icon;

#[derive(Debug, Clone)]
pub enum GeneralMessage {
    NameChanged(String),
    NameButPressed,
    NameButSuccess,
    GoBack,
    UploadGuildImage,
}

#[derive(Debug, Default)]
pub struct GeneralTab {
    name_edit_state: text_input::State,
    name_edit_field: String,
    name_edit_but_state: button::State,
    icon_edit_but_state: button::State,
    back_but_state: button::State,
    loading_text: Option<String>,
    guild_id: u64,
    pub error_message: String,
}

impl GeneralTab {
    pub fn new(guild_id: u64) -> Self {
        Self {
            guild_id,
            ..Default::default()
        }
    }

    pub fn update(
        &mut self,
        message: GeneralMessage,
        client: &Client,
        _: &mut GuildMetaData,
        guild_id: u64,
    ) -> Command<TopLevelMessage> {
        match message {
            GeneralMessage::NameChanged(text) => {
                self.name_edit_field = text;
            }
            GeneralMessage::NameButPressed => {
                //Show loading text via a Option
                self.loading_text = Some("Updating ...".to_string());
                let current_name = self.name_edit_field.clone();
                return client.mk_cmd(
                    |inner| async move {
                        // Build the GuildInformationRequest and update the Name
                        let request = UpdateGuildInformation::new(guild_id).new_guild_name(current_name);
                        update_guild_information(&inner, request).await
                    },
                    |_| TopLevelMessage::guild_settings(ParentMessage::General(GeneralMessage::NameButSuccess)),
                );
            }
            GeneralMessage::NameButSuccess => {
                self.loading_text = Some("Name updated".to_string());
            }
            GeneralMessage::UploadGuildImage => {
                let content_store = client.content_store_arc();
                return client.mk_cmd(
                    |inner| async move {
                        // Select new Guild image and Upload
                        let id = select_upload_files(&inner, content_store, true).await?.remove(0).id;
                        ClientResult::Ok(
                            update_guild_information(
                                &inner,
                                UpdateGuildInformation::new(guild_id).new_guild_picture(Some(id)),
                            )
                            .await?,
                        )
                    },
                    |_| TopLevelMessage::Nothing,
                );
            }
            GeneralMessage::GoBack => {
                return TopLevelScreen::push_screen_cmd(TopLevelScreen::Main(Box::new(
                    super::super::MainScreen::default(),
                )));
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
    fn title(&self) -> String {
        String::from("General")
    }

    fn tab_label(&self) -> TabLabel {
        TabLabel::IconText(Icon::Gear.into(), self.title())
    }

    fn content(
        &mut self,
        client: &Client,
        _: &mut GuildMetaData,
        theme: Theme,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<'_, ParentMessage> {
        let name_edit_but_state = &mut self.name_edit_but_state;
        let guild = client.guilds.get(&self.guild_id).unwrap();
        let back = label_button!(&mut self.back_but_state, "Back")
            .style(theme)
            .on_press(ParentMessage::General(GeneralMessage::GoBack));
        let ui_text_input_row = row(vec![
            TextInput::new(
                &mut self.name_edit_state,
                guild.name.as_str(),
                self.name_edit_field.as_str(),
                |text| ParentMessage::General(GeneralMessage::NameChanged(text)),
            )
            .style(theme)
            .padding(PADDING / 2)
            .width(length!(= 300))
            .into(),
            Button::new(name_edit_but_state, label!["Update"])
                .on_press(ParentMessage::General(GeneralMessage::NameButPressed))
                .style(theme)
                .into(),
        ])
        .into();

        let ui_update_guild_icon = fill_container(
            guild
                .picture
                .as_ref()
                .map(|guild_picture| thumbnail_cache.thumbnails.get(guild_picture))
                .flatten()
                .map_or_else(
                    || Element::from(label!(guild.name.chars().next().unwrap_or('u').to_ascii_uppercase()).size(30)),
                    |handle| Element::from(Image::new(handle.clone())),
                ),
        );

        let ui_image_but = Button::new(&mut self.icon_edit_but_state, ui_update_guild_icon)
            .on_press(ParentMessage::General(GeneralMessage::UploadGuildImage))
            .height(length!(= 50))
            .width(length!(= 50))
            .style(theme)
            .into();

        let mut content = Vec::with_capacity(7);
        if !self.error_message.is_empty() {
            content.push(label!(&self.error_message).color(ERROR_COLOR).size(DEF_SIZE + 2).into());
        }
        content.push(label!("Icon").into());
        content.push(ui_image_but);
        content.push(label!("Name").into());
        if let Some(ldg_text) = &self.loading_text {
            content.push(label!(ldg_text).into());
        }
        content.push(ui_text_input_row);
        content.push(back.into());

        column(content).into()
    }
}
