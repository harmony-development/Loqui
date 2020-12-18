use crate::{
    client::{error::ClientError, Client},
    ui::{component::*, style::Theme},
};
use iced::{
    button, Align, Button, Color, Column, Command, Container, Element, Length, Row, Space, Text,
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
            Container::new(Text::new("Logging out...").size(30))
                .center_y()
                .center_x()
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme)
                .into()
        } else {
            #[inline(always)]
            fn make_button<'a>(
                state: &'a mut button::State,
                confirm: bool,
                theme: Theme,
            ) -> Element<'a, Message> {
                Button::new(
                    state,
                    Container::new(Text::new(if confirm { "Yes" } else { "No" }))
                        .width(Length::Fill)
                        .center_x(),
                )
                .width(Length::FillPortion(1))
                .on_press(confirm)
                .style(theme)
                .into()
            }

            #[inline(always)]
            fn make_space<'a>(units: u16) -> Element<'a, Message> {
                Space::with_width(Length::FillPortion(units)).into()
            }

            let logout_confirm_panel = Column::with_children(
                    vec![
                        Text::new("Do you want to logout?").into(),
                        Text::new("This will delete your current session and you will need to login with your password.")
                            .color(Color::from_rgb(1.0, 0.0, 0.0))
                            .into(),
                        Row::with_children(
                            vec![
                                make_button(&mut self.logout_approve_but_state, true, theme),
                                make_space(1),
                                make_button(&mut self.logout_cancel_but_state, false, theme),
                        ])
                        .width(Length::Fill)
                        .align_items(Align::Center)
                        .into(),
                    ])
                    .align_items(Align::Center)
                    .spacing(12);

            let padded_panel = Row::with_children(vec![
                make_space(3),
                logout_confirm_panel.width(Length::FillPortion(4)).into(),
                make_space(3),
            ])
            .height(Length::Fill)
            .align_items(Align::Center);

            fill_container(padded_panel).style(theme).into()
        }
    }

    pub fn update(&mut self, msg: Message, client: &mut Client) -> Command<super::Message> {
        if msg {
            self.confirmation = true;
            Command::perform(
                Client::logout(
                    client.inner(),
                    client.content_store().session_file().to_path_buf(),
                ),
                |result| match result {
                    Ok(_) => super::Message::PopScreen,
                    Err(err) => super::Message::MatrixError(Box::new(err)),
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
