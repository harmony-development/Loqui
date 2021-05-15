use iced::{Command, Element};

use crate::{
    client::{error::ClientError, Client},
    ui::{component::*, style::*},
};

#[derive(Debug)]
pub enum Message {}

#[derive(Debug)]
pub struct GuildSettings {}

impl GuildSettings {
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
