mod general;
mod invite;

use crate::{
    client::{error::ClientError, Client},
    component::*,
    screen::{
        guild_settings::{
            general::{GeneralMessage, GeneralTab},
            invite::{InviteMessage, InviteTab},
        },
        Message as TopLevelMessage, ScreenMessage as TopLevelScreenMessage,
    },
    style::*,
};
use client::harmony_rust_sdk::client::api::chat::invite::{
    get_guild_invites, get_guild_invites_response::Invite, GetGuildInvitesRequest,
};
use iced::{Align, Column, Container, Element, Length};
use iced_aw::{TabLabel, Tabs, ICON_FONT};

const TAB_PADDING: u16 = 16;

#[derive(Debug)]
pub struct GuildMetaData {
    invites: Option<Vec<Invite>>,
}

#[derive(Debug)]
pub struct GuildSettings {
    guild_id: u64,
    active_tab: usize,
    general_tab: GeneralTab,
    invite_tab: InviteTab,
    current_error: String,
    meta_data: GuildMetaData,
    back_button: button::State,
}

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(usize),
    General(GeneralMessage),
    Invite(InviteMessage),
}

impl GuildSettings {
    pub fn new(guild_id: u64) -> Self {
        GuildSettings {
            guild_id,
            active_tab: 0,
            general_tab: GeneralTab::new(guild_id),
            invite_tab: InviteTab::default(),
            current_error: String::from(""),
            meta_data: GuildMetaData { invites: None },
            back_button: Default::default(),
        }
    }

    pub fn update(&mut self, message: Message, client: &Client) -> Command<TopLevelMessage> {
        match message {
            Message::TabSelected(selected) => {
                self.active_tab = selected;
                match selected {
                    0 => {
                        self.general_tab.error_message.clear();
                    }
                    1 => {
                        self.invite_tab.error_message.clear();
                        // Invite tab
                        // Is Triggered when the invite tab is clicked
                        // Triggers the fetch of the invites, receiving is handled in invite.rs
                        let guild_id = self.guild_id;
                        let inner_client = client.inner().clone();
                        return Command::perform(
                            async move {
                                let request = GetGuildInvitesRequest { guild_id };
                                let invites = get_guild_invites(&inner_client, request).await?.invites;
                                Ok(TopLevelMessage::ChildMessage(TopLevelScreenMessage::GuildSettings(
                                    Message::Invite(InviteMessage::InvitesLoaded(invites)),
                                )))
                            },
                            |result| result.unwrap_or_else(|err| TopLevelMessage::Error(Box::new(err))),
                        );
                    }
                    _ => {}
                };
            }
            Message::General(message) => {
                return self
                    .general_tab
                    .update(message, client, &mut self.meta_data, self.guild_id)
            }
            Message::Invite(message) => {
                return self
                    .invite_tab
                    .update(message, client, &mut self.meta_data, self.guild_id)
            }
        }

        Command::none()
    }

    pub fn view(&mut self, theme: Theme, client: &Client, thumbnail_cache: &ThumbnailCache) -> Element<'_, Message> {
        let position = iced_aw::TabBarPosition::Top;
        Tabs::new(self.active_tab, Message::TabSelected)
            .push(
                self.general_tab.tab_label(),
                self.general_tab
                    .view(client, &mut self.meta_data, theme, thumbnail_cache),
            )
            .push(
                self.invite_tab.tab_label(),
                self.invite_tab
                    .view(client, &mut self.meta_data, theme, thumbnail_cache),
            )
            .tab_bar_style(theme)
            .icon_font(ICON_FONT)
            .tab_bar_position(position)
            .into()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        match self.active_tab {
            0 => self.general_tab.on_error(error),
            1 => self.invite_tab.on_error(error),
            _ => Command::none(),
        }
    }
}

trait Tab {
    fn title(&self) -> String;

    fn tab_label(&self) -> TabLabel;

    fn view(
        &mut self,
        client: &Client,
        meta_data: &mut GuildMetaData,
        theme: Theme,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<'_, Message> {
        let column = Column::new()
            .spacing(20)
            .push(self.content(client, meta_data, theme, thumbnail_cache));

        Container::new(column)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Align::Center)
            .align_y(Align::Center)
            .padding(TAB_PADDING)
            .style(theme)
            .into()
    }

    fn content(
        &mut self,
        client: &Client,
        meta_data: &mut GuildMetaData,
        theme: Theme,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<'_, Message>;
}
