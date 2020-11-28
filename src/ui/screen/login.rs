use super::LoginInformation;
use crate::{
    client::{error::ClientError, Client, Session},
    ui::style::{Theme, PADDING, SPACING},
};
use iced::{
    button, text_input, Align, Button, Color, Column, Command, Container, Element, Length, Row,
    Space, Subscription, Text, TextInput,
};

#[derive(Debug, Clone)]
pub enum Message {
    HomeserverChanged(String),
    UsernameChanged(String),
    PasswordChanged(String),
    LoginWithSession(Session),
    LoginInitiated,
}

#[derive(Default)]
pub struct LoginScreen {
    homeserver_field: text_input::State,
    username_field: text_input::State,
    password_field: text_input::State,
    login_button: button::State,

    login_info: LoginInformation,
    /// `None` if not logging out, `Some(restoring_session)` if logging in.
    logging_in: Option<bool>,
    /// The error formatted as a string to be displayed to the user.
    current_error: String,
}

impl LoginScreen {
    pub fn with_logging_in(logging_in: Option<bool>) -> Self {
        Self {
            logging_in,
            ..Self::default()
        }
    }

    pub fn with_error(current_error: String) -> Self {
        Self {
            current_error,
            ..Self::default()
        }
    }

    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if let Some(restoring_session) = self.logging_in {
            return Container::new(
                Text::new(if restoring_session {
                    "Restoring session..."
                } else {
                    "Logging in..."
                })
                .size(30),
            )
            .center_x()
            .center_y()
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme)
            .into();
        }

        let error_text = Text::new(&self.current_error)
            .color(Color::from_rgb8(200, 0, 0))
            .size(18);

        let homeserver_prefix = Text::new("https://");
        let homeserver_field = TextInput::new(
            &mut self.homeserver_field,
            "Enter your homeserver domain here...",
            &self.login_info.homeserver_domain,
            Message::HomeserverChanged,
        )
        .padding(PADDING / 2)
        .style(theme);
        let homeserver_area = Row::with_children(vec![
            Container::new(homeserver_prefix)
                .padding(PADDING / 2)
                .into(),
            homeserver_field.into(),
        ])
        .align_items(Align::Start);

        let username_field = TextInput::new(
            &mut self.username_field,
            "Enter your username here...",
            &self.login_info.username,
            Message::UsernameChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let password_field = TextInput::new(
            &mut self.password_field,
            "Enter your password here...",
            &self.login_info.password,
            Message::PasswordChanged,
        )
        .padding(PADDING / 2)
        .style(theme)
        .on_submit(Message::LoginInitiated)
        .password();

        let login_button = Button::new(&mut self.login_button, Text::new("Login"))
            .on_press(Message::LoginInitiated)
            .style(theme);

        let login_panel = Column::with_children(vec![
            error_text.into(),
            homeserver_area.into(),
            username_field.into(),
            password_field.into(),
            login_button.into(),
        ])
        .align_items(Align::Center)
        .spacing(SPACING * 3);

        let padded_panel = Row::with_children(vec![
            Space::with_width(Length::FillPortion(3)).into(),
            login_panel.width(Length::FillPortion(4)).into(),
            Space::with_width(Length::FillPortion(3)).into(),
        ])
        .height(Length::Fill)
        .align_items(Align::Center);

        Container::new(padded_panel)
            .style(theme)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn update(&mut self, msg: Message) -> Command<super::Message> {
        match msg {
            Message::HomeserverChanged(new_homeserver) => {
                self.login_info.homeserver_domain = new_homeserver;
            }
            Message::UsernameChanged(new_username) => {
                self.login_info.username = new_username;
            }
            Message::PasswordChanged(new_password) => {
                self.login_info.password = new_password;
            }
            Message::LoginWithSession(session) => {
                async fn try_login(session: Session) -> Result<Client, ClientError> {
                    let mut client = Client::new_with_session(session)?;
                    client.initial_sync().await?;

                    Ok(client)
                }

                return Command::perform(try_login(session), |result| match result {
                    Ok(client) => super::Message::LoginComplete(client),
                    Err(err) => super::Message::MatrixError(Box::new(err)),
                });
            }
            Message::LoginInitiated => {
                async fn try_login(login_info: LoginInformation) -> Result<Client, ClientError> {
                    let mut client = Client::new(
                        &format!("https://{}", login_info.homeserver_domain),
                        &login_info.username,
                        &login_info.password,
                    )
                    .await?;

                    client.initial_sync().await?;

                    Ok(client)
                }

                self.logging_in = Some(false);
                return Command::perform(
                    try_login(self.login_info.clone()),
                    |result| match result {
                        Ok(client) => super::Message::LoginComplete(client),
                        Err(err) => super::Message::MatrixError(Box::new(err)),
                    },
                );
            }
        }
        Command::none()
    }

    pub fn subscription(&self) -> Subscription<super::Message> {
        Subscription::none()
    }

    pub fn on_error(&mut self, error_string: String) {
        self.current_error = error_string;
        self.logging_in = None;
    }
}
