use iced::{Command, Element};

use crate::{
    client::{error::ClientError, Client},
    ui::{component::*, style::*},
};

#[derive(Debug)]
pub enum Message {}

#[derive(Debug)]
pub struct GuildSettings {
    guild_id: u64
}

impl GuildSettings {
    pub fn new(guild_id: u64) -> Self {
        Self {
            guild_id
        }
    }

    pub fn view(&mut self, theme: Theme, client: &Client) -> Element<Message> {
        label!("asd").into()
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> Command<super::Message> {
        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<super::Message> {
        Command::none()
    }
}
