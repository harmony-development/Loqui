use client::bool_ext::BoolExt;

use super::super::{LoginScreen, Message as TopLevelMessage, Screen as TopLevelScreen};

use crate::{
    client::{error::ClientError, Client},
    component::*,
    label, label_button, length,
    screen::ResultExt,
    space,
    style::{Theme, DEF_SIZE, ERROR_COLOR},
};

pub type Message = bool;

#[derive(Debug, Default)]
pub struct LogoutModal {
    logout_approve_but_state: button::State,
    logout_cancel_but_state: button::State,
    confirmation: bool,
}

impl LogoutModal {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        self.confirmation.map_or_else(
            move || {
                let make_button = |state, confirm: bool| {
                    let text = confirm.some("Yes").unwrap_or("No");

                    label_button!(state, text)
                        .style(theme)
                        .on_press(confirm)
                        .width(length!(= 80))
                };

                let logout_confirm_panel = column(vec![
                    label!("Do you want to logout?").size(DEF_SIZE + 2).into(),
                    label!("This will delete your current session.")
                        .color(ERROR_COLOR)
                        .size(DEF_SIZE + 2)
                        .into(),
                    row(vec![
                        make_button(&mut self.logout_approve_but_state, true).into(),
                        space!(w = 200).into(),
                        make_button(&mut self.logout_cancel_but_state, false).into(),
                    ])
                    .into(),
                ])
                .spacing(12);

                Container::new(logout_confirm_panel)
                    .style(theme.round().border_width(0.0))
                    .center_x()
                    .center_y()
                    .into()
            },
            || fill_container(label!("Logging out...").size(30)).style(theme).into(),
        )
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> Command<TopLevelMessage> {
        msg.map_or_else(Command::none, || {
            Command::perform(client.logout(true), |result| {
                result.unwrap().map_to_msg_def(|_| {
                    TopLevelMessage::Logout(TopLevelScreen::Login(LoginScreen::new().into()).into())
                })
            })
        })
    }

    pub fn on_error(&mut self, _error: &ClientError) -> Command<TopLevelMessage> {
        self.confirmation = false;

        Command::none()
    }
}
