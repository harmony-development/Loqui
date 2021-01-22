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
pub struct Logout {
    logout_approve_but_state: button::State,
    logout_cancel_but_state: button::State,

    confirmation: bool,
}

impl Logout {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if self.confirmation {
            fill_container(label!("Logging out...").size(30))
                .style(theme)
                .into()
        } else {
            let make_button = |state, confirm| {
                let text = if confirm { "Yes" } else { "No" };

                label_button!(state, text).style(theme).on_press(confirm)
            };

            let logout_confirm_panel = column(
                    vec![
                        label!("Do you want to logout?").into(),
                        label!("This will delete your current session and you will need to login with your password.")
                            .color(ERROR_COLOR)
                            .into(),
                        row(
                            vec![
                                make_button(&mut self.logout_approve_but_state, true).width(length!(+)).into(),
                                space!(w+).into(),
                                make_button(&mut self.logout_cancel_but_state, false).width(length!(+)).into(),
                        ])
                        .width(length!(+))
                        .into(),
                    ])
                    .spacing(12);

            let padded_panel = row(vec![
                space!(w % 3).into(),
                logout_confirm_panel.width(length!(% 4)).into(),
                space!(w % 3).into(),
            ]);

            fill_container(padded_panel).style(theme).into()
        }
    }

    pub fn update(&mut self, msg: Message, client: &mut Client) -> Command<super::Message> {
        if msg {
            self.confirmation = true;
            Command::perform(
                Client::logout(
                    client.inner().clone(),
                    client.content_store().session_file().to_path_buf(),
                ),
                |result| match result {
                    Ok(_) => super::Message::PopScreen,
                    Err(err) => super::Message::Error(Box::new(err)),
                },
            )
        } else {
            Command::perform(async {}, |_| super::Message::PopScreen)
        }
    }

    pub fn on_error(&mut self, _: ClientError) -> Command<super::Message> {
        self.confirmation = false;
        Command::none()
    }
}
