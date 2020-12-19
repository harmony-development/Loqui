use crate::{
    client::{content::ContentStore, error::ClientError, AuthInfo, AuthMethod, Client},
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR, PADDING},
    },
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Message {
    UsernameChanged(String),
    PasswordChanged(String),
    HomeserverChanged(String),
    AuthWith(AuthMethod),
}

#[derive(Debug, Default)]
pub struct LoginScreen {
    homeserver_field: text_input::State,
    username_field: text_input::State,
    password_field: text_input::State,
    login_button: button::State,
    register_button: button::State,
    guest_button: button::State,

    auth_info: AuthInfo,
    cur_auth_method: Option<AuthMethod>,
    /// The error formatted as a string to be displayed to the user.
    current_error: String,
    content_store: Arc<ContentStore>,
}

impl LoginScreen {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        Self {
            content_store,
            ..Self::default()
        }
    }

    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if let Some(method) = &self.cur_auth_method {
            let text = match method {
                AuthMethod::LoginOrRegister { info, register } => {
                    if *register {
                        format!(
                            "Registering with username \"{}\" to homeserver \"{}\"",
                            info.username, info.homeserver_domain
                        )
                    } else {
                        format!(
                            "Logging in with username \"{}\" to homeserver \"{}\"",
                            info.username, info.homeserver_domain
                        )
                    }
                }
                AuthMethod::Guest { homeserver_domain } => {
                    format!(
                        "Creating a guest account on homeserver {}...",
                        homeserver_domain
                    )
                }
                AuthMethod::RestoringSession => String::from("Restoring existing session..."),
            };

            return fill_container(label(text).size(30)).style(theme).into();
        }

        let error_text = label(&self.current_error).color(ERROR_COLOR).size(18);

        let homeserver_prefix = Container::new(label("https://")).padding(PADDING / 2);
        let homeserver_field = TextInput::new(
            &mut self.homeserver_field,
            "Enter your homeserver domain here...",
            &self.auth_info.homeserver_domain,
            Message::HomeserverChanged,
        )
        .padding(PADDING / 2)
        .style(theme);
        let homeserver_field =
            Row::with_children(vec![homeserver_prefix.into(), homeserver_field.into()]);

        let username_field = TextInput::new(
            &mut self.username_field,
            "Enter your username here...",
            &self.auth_info.username,
            Message::UsernameChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let password_field = TextInput::new(
            &mut self.password_field,
            "Enter your password here...",
            &self.auth_info.password,
            Message::PasswordChanged,
        )
        .padding(PADDING / 2)
        .style(theme)
        .on_submit(Message::AuthWith(AuthMethod::LoginOrRegister {
            info: self.auth_info.clone(),
            register: false,
        }))
        .password();

        let login_button = label_button(&mut self.login_button, "Login")
            .on_press(Message::AuthWith(AuthMethod::LoginOrRegister {
                info: self.auth_info.clone(),
                register: false,
            }))
            .style(theme);

        let register_button = label_button(&mut self.register_button, "Register")
            .on_press(Message::AuthWith(AuthMethod::LoginOrRegister {
                info: self.auth_info.clone(),
                register: true,
            }))
            .style(theme);

        let guest_button = label_button(&mut self.guest_button, "Guest")
            .on_press(Message::AuthWith(AuthMethod::Guest {
                homeserver_domain: self.auth_info.homeserver_domain.clone(),
            }))
            .style(theme);

        let login_panel = column(vec![
            error_text.into(),
            homeserver_field.into(),
            username_field.into(),
            password_field.into(),
            row(vec![
                login_button.width(Length::FillPortion(1)).into(),
                wspace(1).into(),
                register_button.width(Length::FillPortion(1)).into(),
                wspace(1).into(),
                guest_button.width(Length::FillPortion(1)).into(),
            ])
            .width(Length::Fill)
            .into(),
        ]);

        let padded_panel = row(vec![
            wspace(2).into(),
            login_panel.width(Length::FillPortion(6)).into(),
            wspace(2).into(),
        ])
        .height(Length::Fill);

        fill_container(padded_panel).style(theme).into()
    }

    pub fn update(&mut self, msg: Message) -> Command<super::Message> {
        match msg {
            Message::HomeserverChanged(new_homeserver) => {
                self.auth_info.homeserver_domain = new_homeserver;
            }
            Message::UsernameChanged(new_username) => {
                self.auth_info.username = new_username;
            }
            Message::PasswordChanged(new_password) => {
                self.auth_info.password = new_password;
            }
            Message::AuthWith(method) => {
                self.cur_auth_method = Some(method.clone());
                return Command::perform(
                    Client::new(method, self.content_store.clone()),
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
        self.cur_auth_method = None;

        Command::none()
    }
}
