use super::super::Message as TopLevelMessage;

use crate::{
    client::{error::ClientError, Client},
    label, label_button, length, space,
    ui::{
        component::*,
        style::{Theme, DEF_SIZE, ERROR_COLOR},
    },
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
        if self.confirmation {
            fill_container(label!("Logging out...").size(30))
                .style(theme)
                .into()
        } else {
            let make_button = |state, confirm| {
                let text = if confirm { "Yes" } else { "No" };

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
                .style(theme.round())
                .center_x()
                .center_y()
                .into()
        }
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> Command<TopLevelMessage> {
        if msg {
            let content_store = client.content_store_arc();
            let inner = client.inner().clone();
            Command::perform(
                async move {
                    let result =
                        Client::logout(inner, content_store.session_file().to_path_buf()).await;

                    result.map_or_else(
                        |err| TopLevelMessage::Error(Box::new(err)),
                        |_| {
                            TopLevelMessage::Logout(
                                super::super::Screen::Login(super::super::LoginScreen::new(
                                    content_store,
                                ))
                                .into(),
                            )
                        },
                    )
                },
                |msg| msg,
            )
        } else {
            Command::none()
        }
    }

    pub fn on_error(&mut self, _error: &ClientError) -> Command<TopLevelMessage> {
        self.confirmation = false;

        Command::none()
    }
}
