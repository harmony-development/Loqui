use crate::{
    client::{content::ContentStore, error::ClientError, Client, LoginInformation},
    ui::{
        component::*,
        style::{Theme, PADDING, SPACING},
    },
};
use iced::{
    button, text_input, Align, Button, Color, Column, Command, Container, Element, Length, Row,
    Space, Text, TextInput,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Message {
    HomeserverChanged(String),
    UsernameChanged(String),
    PasswordChanged(String),
    LoginWithSession,
    LoginInitiated,
}

#[derive(Debug)]
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
    content_store: Arc<ContentStore>,
}

impl LoginScreen {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        Self {
            homeserver_field: Default::default(),
            username_field: Default::default(),
            password_field: Default::default(),
            login_button: Default::default(),
            login_info: Default::default(),
            logging_in: None,
            current_error: String::new(),
            content_store,
        }
    }

    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if let Some(restoring_session) = self.logging_in {
            return fill_container(
                Text::new(if restoring_session {
                    "Restoring session..."
                } else {
                    "Logging in..."
                })
                .size(30),
            )
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

        fill_container(padded_panel).style(theme).into()
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
            Message::LoginWithSession => {
                self.logging_in = Some(true);
                return Command::perform(
                    Client::new_with_session(self.content_store.clone()),
                    |result| match result {
                        Ok(client) => super::Message::LoginComplete(client),
                        Err(err) => super::Message::MatrixError(Box::new(err)),
                    },
                );
            }
            Message::LoginInitiated => {
                self.logging_in = Some(false);
                return Command::perform(
                    Client::new(self.login_info.clone(), self.content_store.clone()),
                    |result| match result {
                        Ok(client) => super::Message::LoginComplete(client),
                        Err(err) => super::Message::MatrixError(Box::new(err)),
                    },
                );
            }
        }
        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<super::Message> {
        self.current_error = error.to_string();
        self.logging_in = None;

        Command::none()
    }
}
