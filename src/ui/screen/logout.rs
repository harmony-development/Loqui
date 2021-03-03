use crate::{
    client::{error::ClientError, Client},
    label, label_button, length, space,
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR},
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
                    .width(length!(+))
            };

            let logout_confirm_panel = column(
                    vec![
                        label!("Do you want to logout?").size(18).height(length!(%2)).into(),
                        label!("This will delete your current session and you will need to login with your password.")
                            .color(ERROR_COLOR)
                            .size(18)
                            .height(length!(%3))
                            .into(),
                        row(vec![
                            make_button(&mut self.logout_approve_but_state, true).into(),
                            space!(w+).into(),
                            make_button(&mut self.logout_cancel_but_state, false).into(),
                        ])
                        .height(length!(%6))
                        .width(length!(+))
                        .into(),
                    ])
                    .spacing(12);

            row(vec![
                space!(w % 3).into(),
                column(vec![
                    space!(h % 4).into(),
                    fill_container(logout_confirm_panel.width(length!(+)).height(length!(+)))
                        .height(length!(% 3))
                        .style(theme.round())
                        .into(),
                    space!(h % 4).into(),
                ])
                .width(length!(% 4))
                .height(length!(+))
                .into(),
                space!(w % 3).into(),
            ])
            .height(length!(+))
            .width(length!(+))
            .into()
        }
    }

    pub fn update(&mut self, msg: Message, client: &Client) -> Command<super::Message> {
        if msg {
            let content_store = client.content_store_arc();
            let inner = client.inner().clone();
            Command::perform(
                async move {
                    let result =
                        Client::logout(inner, content_store.session_file().to_path_buf()).await;

                    result.map_or_else(
                        |err| super::Message::Error(Box::new(err)),
                        |_| {
                            super::Message::Logout(
                                super::Screen::Login(super::LoginScreen::new(content_store)).into(),
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

    pub fn on_error(&mut self, _error: &ClientError) -> Command<super::Message> {
        self.confirmation = false;

        Command::none()
    }
}
