pub mod login;
pub mod main;

pub use login::LoginScreen;
pub use main::MainScreen;

use crate::{
    client::{content::ContentStore, error::ClientError, Client},
    ui::style::Theme,
};
use iced::{executor, Application, Command, Element, Subscription};
use std::sync::Arc;

#[derive(Debug)]
pub enum Message {
    LoginScreen(login::Message),
    MainScreen(main::Message),
    /// Sent when a logout request is completed successfully.
    LogoutComplete,
    /// Sent whenever an error occurs.
    MatrixError(Box<ClientError>),
    /// Sent when the "login" is complete, ie. establishing a session and performing an initial sync.
    LoginComplete(Client),
    /// Do nothing.
    Nothing,
}

pub enum Screen {
    Login { screen: LoginScreen },
    Main { screen: MainScreen },
}

pub struct ScreenManager {
    theme: Theme,
    screen: Screen,
    client: Option<Client>,
    content_store: Arc<ContentStore>,
}

impl ScreenManager {
    pub fn new(content_store: ContentStore) -> Self {
        Self {
            theme: Theme::Dark,
            screen: Screen::Login {
                screen: LoginScreen::default(),
            },
            client: None,
            content_store: Arc::new(content_store),
        }
    }
}

impl Application for ScreenManager {
    type Executor = executor::Default;
    type Message = Message;
    type Flags = ContentStore;

    fn new(content_store: Self::Flags) -> (Self, Command<Self::Message>) {
        (ScreenManager::new(content_store), Command::none())
    }

    fn title(&self) -> String {
        String::from("Icy Matrix")
    }

    fn update(&mut self, msg: Self::Message) -> Command<Self::Message> {
        match msg {
            Message::Nothing => {}
            Message::MainScreen(msg) => {
                if let (Screen::Main { screen }, Some(client)) =
                    (&mut self.screen, &mut self.client)
                {
                    return screen.update(msg, client);
                }
            }
            Message::LoginScreen(msg) => {
                if let Screen::Login { ref mut screen } = self.screen {
                    return screen.update(msg, self.content_store.clone());
                }
            }
            Message::LoginComplete(client) => {
                self.client = Some(client);
                self.screen = Screen::Main {
                    screen: MainScreen::new(),
                };
            }
            Message::LogoutComplete => {
                self.screen = Screen::Login {
                    screen: LoginScreen::default(),
                };
            }
            Message::MatrixError(err) => {
                use ruma::{api::client::error::ErrorKind as ClientAPIErrorKind, api::error::*};
                use ruma_client::Error as InnerClientError;

                let error_string = err.to_string();
                log::error!("{}", error_string);

                if let ClientError::Internal(err) = *err {
                    if let InnerClientError::FromHttpResponse(err) = err {
                        if let FromHttpResponseError::Http(err) = err {
                            if let ServerError::Known(err) = err {
                                // Return to login screen since the users session has expired.
                                if let ClientAPIErrorKind::UnknownToken { soft_logout: _ } =
                                    err.kind
                                {
                                    self.screen = Screen::Login {
                                        screen: LoginScreen::with_error(error_string),
                                    };

                                    return Command::none();
                                }
                            }
                        }
                    }
                }

                match &mut self.screen {
                    Screen::Login { screen } => screen.on_error(error_string),
                    Screen::Main { screen } => screen.on_error(error_string),
                }
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if let (Screen::Main { screen, .. }, Some(client)) = (&self.screen, &self.client) {
            screen.subscription(client)
        } else {
            Subscription::none()
        }
    }

    fn view(&mut self) -> Element<Self::Message> {
        match self.screen {
            Screen::Login { ref mut screen } => screen.view(self.theme).map(Message::LoginScreen),
            Screen::Main { ref mut screen } => screen
                .view(
                    self.theme,
                    self.client.as_ref().unwrap(),
                    &self.content_store,
                )
                .map(Message::MainScreen),
        }
    }
}
