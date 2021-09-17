mod manage_emotes;

use std::ops::Not;

use client::{
    error::ClientError,
    harmony_rust_sdk::{
        api::emote::{DeleteEmotePackRequest, DequipEmotePackRequest, EquipEmotePackRequest},
        client::api::emote::CreateEmotePack,
    },
    Client,
};
use iced::Tooltip;
use iced_aw::{modal, Modal};

use crate::{
    component::*,
    label_button, length,
    screen::{map_to_nothing, ClientExt, Message as TopLevelMessage, Screen as TopLevelScreen},
    space,
    style::{Theme, PADDING, SPACING},
};

use self::manage_emotes::ManageEmotesModal;

use super::sub_escape_pop_screen;

#[derive(Debug, Clone)]
pub enum Message {
    CopyToClipboard(String),
    CopyIdToClipboard(u64),
    ManageEmotes(u64),
    DeleteEmotePack(u64),
    CreateEmotePack,
    PackNameChanged(String),
    PackIdChanged(String),
    ManageEmotesMessage(manage_emotes::Message),
    DequipEmotePack(u64),
    EquipEmotePack,
    GoBack,
}

#[derive(Debug, Default, Clone)]
pub struct ManageEmotesScreen {
    button_states: Vec<(
        button::State,
        button::State,
        button::State,
        button::State,
        button::State,
    )>,
    packs_list_state: scrollable::State,
    back_but_state: button::State,
    create_pack_state: button::State,
    equip_pack_state: button::State,
    pack_id_state: text_input::State,
    pack_name_state: text_input::State,
    pack_name: String,
    pack_id: String,
    manage_emotes_modal: modal::State<ManageEmotesModal>,
    pub error_message: String,
}

impl ManageEmotesScreen {
    pub fn update(
        &mut self,
        message: Message,
        client: &Client,
        clip: &mut iced::Clipboard,
    ) -> Command<TopLevelMessage> {
        match message {
            Message::CopyToClipboard(val) => clip.write(val),
            Message::CopyIdToClipboard(id) => clip.write(id.to_string()),
            Message::ManageEmotes(pack_id) => {
                self.manage_emotes_modal.inner_mut().pack_id = pack_id;
                self.manage_emotes_modal.show(true);
                self.error_message.clear();
            }
            Message::DeleteEmotePack(pack_id) => {
                return client.mk_cmd(
                    |inner| async move { inner.call(DeleteEmotePackRequest { pack_id }).await },
                    map_to_nothing,
                );
            }
            Message::CreateEmotePack => {
                let pack_name = self.pack_name.drain(..).collect::<String>();
                return client.mk_cmd(
                    |inner| async move { inner.call(CreateEmotePack::new(pack_name)).await },
                    map_to_nothing,
                );
            }
            Message::GoBack => return TopLevelScreen::pop_screen_cmd(),
            Message::PackNameChanged(pack_name) => self.pack_name = pack_name,
            Message::ManageEmotesMessage(message) => {
                let (cmd, go_back) = self.manage_emotes_modal.inner_mut().update(message, client, clip);
                self.manage_emotes_modal.show(!go_back);
                return cmd;
            }
            Message::DequipEmotePack(pack_id) => {
                return client.mk_cmd(
                    |inner| async move { inner.call(DequipEmotePackRequest { pack_id }).await },
                    map_to_nothing,
                );
            }
            Message::EquipEmotePack => {
                if let Ok(pack_id) = self.pack_id.parse::<u64>() {
                    return client.mk_cmd(
                        |inner| async move { inner.call(EquipEmotePackRequest { pack_id }).await },
                        map_to_nothing,
                    );
                }
            }
            Message::PackIdChanged(id) => {
                if id.chars().all(|c| c.is_ascii_digit()) {
                    self.pack_id = id
                }
            }
        }

        Command::none()
    }

    pub fn view<'a>(
        &'a mut self,
        theme: &'a Theme,
        client: &'a Client,
        thumbnails: &'a ThumbnailCache,
    ) -> Element<'a, Message> {
        let mut packs = Scrollable::new(&mut self.packs_list_state)
            .align_items(Align::Start)
            .height(length!(+))
            .width(length!(+))
            .padding(PADDING)
            .spacing(SPACING)
            .style(theme);

        self.button_states
            .resize_with(client.emote_packs.len(), Default::default);
        for ((pack_id, pack), (copy_name_state, copy_id_state, manage_emote_state, delete_state, dequip_state)) in
            client.emote_packs.iter().zip(&mut self.button_states)
        {
            let pack_id = *pack_id;

            let mut content_widgets = Vec::with_capacity(6);
            content_widgets.push(
                Tooltip::new(
                    label_button!(copy_name_state, pack.pack_name.as_str())
                        .style(theme)
                        .on_press(Message::CopyToClipboard(pack.pack_name.to_string())),
                    "Click to copy",
                    iced::tooltip::Position::Top,
                )
                .style(theme)
                .into(),
            );
            content_widgets.push(
                Tooltip::new(
                    label_button!(copy_id_state, format!("ID {}", pack_id))
                        .style(theme)
                        .on_press(Message::CopyIdToClipboard(pack_id)),
                    "Click to copy",
                    iced::tooltip::Position::Top,
                )
                .style(theme)
                .into(),
            );
            content_widgets.push(space!(w+).into());
            content_widgets.push(
                Tooltip::new(
                    Button::new(dequip_state, icon(Icon::Archive))
                        .style(theme)
                        .on_press(Message::DequipEmotePack(pack_id)),
                    "Dequip pack",
                    iced::tooltip::Position::Top,
                )
                .style(theme)
                .into(),
            );
            if Some(pack.pack_owner) == client.user_id {
                content_widgets.push(
                    Tooltip::new(
                        Button::new(manage_emote_state, icon(Icon::Pencil))
                            .style(theme)
                            .on_press(Message::ManageEmotes(pack_id)),
                        "Manage emotes",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
                content_widgets.push(
                    Tooltip::new(
                        Button::new(delete_state, icon(Icon::Trash))
                            .style(theme)
                            .on_press(Message::DeleteEmotePack(pack_id)),
                        "Delete pack",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
            }

            packs = packs.push(Container::new(row(content_widgets)).style(theme));
        }

        let mut content = Vec::with_capacity(4);

        if !self.error_message.is_empty() {
            content.push(label!(self.error_message.as_str()).color(theme.user_theme.error).into())
        }
        content.push(fill_container(packs).style(theme).into());
        content.push(
            Row::with_children(vec![
                TextInput::new(
                    &mut self.pack_id_state,
                    "Enter pack ID...",
                    &self.pack_id,
                    Message::PackIdChanged,
                )
                .style(theme)
                .padding(PADDING / 2)
                .on_submit(Message::EquipEmotePack)
                .into(),
                label_button!(&mut self.equip_pack_state, "Equip")
                    .on_press(Message::EquipEmotePack)
                    .style(theme)
                    .into(),
            ])
            .align_items(Align::Center)
            .spacing(SPACING * 2)
            .width(length!(+))
            .into(),
        );
        content.push(
            Row::with_children(vec![
                TextInput::new(
                    &mut self.pack_name_state,
                    "Enter pack name...",
                    &self.pack_name,
                    Message::PackNameChanged,
                )
                .style(theme)
                .padding(PADDING / 2)
                .on_submit(Message::CreateEmotePack)
                .into(),
                label_button!(&mut self.create_pack_state, "Create")
                    .on_press(Message::CreateEmotePack)
                    .style(theme)
                    .into(),
            ])
            .align_items(Align::Center)
            .spacing(SPACING * 2)
            .width(length!(+))
            .into(),
        );
        content.push(
            label_button!(&mut self.back_but_state, "Back")
                .on_press(Message::GoBack)
                .style(theme)
                .into(),
        );

        let content = fill_container(column(content))
            .style(theme.border_width(0.0))
            .padding(PADDING * 10);

        Modal::new(&mut self.manage_emotes_modal, content, move |state| {
            state.view(theme, client, thumbnails).map(Message::ManageEmotesMessage)
        })
        .style(theme)
        .backdrop(Message::ManageEmotesMessage(manage_emotes::Message::GoBack))
        .on_esc(Message::ManageEmotesMessage(manage_emotes::Message::GoBack))
        .into()
    }

    pub fn subscription(&self) -> Subscription<TopLevelMessage> {
        self.manage_emotes_modal
            .is_shown()
            .not()
            .then(sub_escape_pop_screen)
            .unwrap_or_else(Subscription::none)
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_message = error.to_string();
        Command::none()
    }
}
