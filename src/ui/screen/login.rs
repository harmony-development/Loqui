use crate::{
    client::{content::ContentStore, error::ClientError, Client, Session},
    label, label_button, length, space,
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR, PADDING},
    },
};
use harmony_rust_sdk::{
    api::{
        auth::{auth_step::Step, next_step_request::form_fields::Field},
        exports::hrpc::url::Url,
    },
    client::{
        api::auth::{AuthStep, AuthStepResponse},
        AuthStatus,
    },
};
use super::Message as TopLevelMessage;
use std::{collections::HashMap, sync::Arc};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum AuthType {
    Form,
    Choice,
    Waiting,
}

#[derive(Debug, Clone, Copy)]
enum AuthPart {
    Homeserver,
    Step(AuthType),
}

#[derive(Debug, Clone)]
pub enum Message {
    FieldChanged(String, String),
    ProceedWithChoice(String),
    Proceed,
    GoBack,
    AuthStep(Option<AuthStep>),
}

#[derive(Debug)]
pub struct LoginScreen {
    fields: Vec<(String, (text_input::State, String, String))>,
    choices: HashMap<String, button::State>,
    proceed: button::State,
    back: button::State,

    current_step: AuthPart,
    can_go_back: bool,
    /// The error formatted as a string to be displayed to the user.
    current_error: String,
    content_store: Arc<ContentStore>,
    pub waiting: bool,
}

impl LoginScreen {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        let mut screen = Self {
            content_store,
            fields: Default::default(),
            choices: Default::default(),
            proceed: Default::default(),
            back: Default::default(),
            current_step: AuthPart::Homeserver,
            can_go_back: false,
            current_error: Default::default(),
            waiting: false,
        };
        screen.reset_to_first_step();
        screen
    }

    pub fn reset_to_first_step(&mut self) {
        self.waiting = false;
        self.can_go_back = false;
        self.current_step = AuthPart::Homeserver;
        self.fields.clear();
        self.choices.clear();
        self.fields.push((
            "homeserver".to_string(),
            (
                Default::default(),
                "https://chat.harmonyapp.io:2289".to_string(),
                "text".to_string(),
            ),
        ));
    }

    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if self.waiting {
            return fill_container(label!("Please wait...").size(30))
                .style(theme)
                .into();
        }

        let mut widgets = Vec::with_capacity(self.fields.len() + self.choices.len() + 1);
        if !self.current_error.is_empty() {
            let error_text = label!(self
                .current_error
                .as_str()
                .chars()
                .take(250)
                .collect::<String>())
            .color(ERROR_COLOR)
            .size(18);
            widgets.push(error_text.into());
        }

        if !self.fields.is_empty() {
            for (name, (state, value, r#type)) in self.fields.iter_mut() {
                let namee = name.clone();
                let mut input = TextInput::new(state, name, value, move |new| {
                    Message::FieldChanged(namee.clone(), new)
                })
                .padding(PADDING / 2)
                .style(theme);
                input = match r#type.as_str() {
                    "password" | "new-password" => input.password(),
                    _ => input,
                };
                widgets.push(input.into());
            }
        }

        if !self.choices.is_empty() {
            let mut sorted_choices = self.choices.iter_mut().collect::<Vec<_>>();
            sorted_choices.sort_unstable_by_key(|(name, _)| name.as_str());
            for (name, state) in &mut self.choices {
                widgets.push(
                    Button::new(state, label!(name))
                        .on_press(Message::ProceedWithChoice(name.clone()))
                        .style(theme)
                        .into(),
                );
            }
        }

        if let AuthPart::Step(AuthType::Form) | AuthPart::Homeserver = self.current_step {
            widgets.push(
                label_button!(&mut self.proceed, "Proceed")
                    .on_press(Message::Proceed)
                    .style(theme)
                    .into(),
            );
        }

        if self.can_go_back {
            widgets.push(
                label_button!(&mut self.back, "Back")
                    .on_press(Message::GoBack)
                    .style(theme)
                    .into(),
            );
        }

        let field_panel = column(widgets);

        let padded_panel = row(vec![
            space!(w = 3).into(),
            field_panel.width(length!(% 4)).into(),
            space!(w = 3).into(),
        ])
        .height(length!(+));

        fill_container(padded_panel).style(theme).into()
    }

    pub fn update(
        &mut self,
        client: Option<&Client>,
        msg: Message,
        content_store: &Arc<ContentStore>,
    ) -> Command<TopLevelMessage> {
        fn respond(
            screen: &mut LoginScreen,
            client: &Client,
            response: AuthStepResponse,
        ) -> Command<TopLevelMessage> {
            screen.waiting = true;
            let inner = client.inner().clone();
            Command::perform(
                async move { inner.next_auth_step(response).await },
                |result| {
                    result.map_or_else(
                        |err| TopLevelMessage::Error(Box::new(err.into())),
                        |step| TopLevelMessage::LoginScreen(Message::AuthStep(step)),
                    )
                },
            )
        }

        match msg {
            Message::FieldChanged(field, value) => {
                if let Some(index) = self.fields.iter().position(|(key, _)| key == &field) {
                    if let Some((_, (_, val, _))) = self.fields.get_mut(index) {
                        *val = value;
                    }
                }
            }
            Message::GoBack => {
                if let Some(client) = client {
                    self.waiting = true;
                    let inner = client.inner().clone();
                    return Command::perform(
                        async move { inner.prev_auth_step().await },
                        |result| {
                            result.map_or_else(
                                |err| TopLevelMessage::Error(Box::new(err.into())),
                                |step| TopLevelMessage::LoginScreen(Message::AuthStep(Some(step))),
                            )
                        },
                    );
                }
            }
            Message::ProceedWithChoice(choice) => {
                if let Some(client) = client {
                    let response = AuthStepResponse::Choice(choice);
                    return respond(self, client, response);
                }
            }
            Message::Proceed => {
                if let (Some(client), AuthPart::Step(step)) = (client, self.current_step) {
                    let response = match step {
                        AuthType::Choice => unreachable!("choice is not handled here"),
                        AuthType::Form => AuthStepResponse::form(
                            self.fields
                                .iter()
                                .map(|(_, (_, value, r#type))| match r#type.as_str() {
                                    "number" => Field::Number(value.parse().unwrap()),
                                    "password" => Field::Bytes(value.as_bytes().to_vec()),
                                    "new-password" => Field::Bytes(value.as_bytes().to_vec()),
                                    _ => Field::String(value.clone()),
                                })
                                .collect(),
                        ),
                        _ => todo!("Implement waiting"),
                    };
                    return respond(self, client, response);
                } else if let AuthPart::Homeserver = &self.current_step {
                    if let Some(index) = self.fields.iter().position(|(key, _)| key == "homeserver")
                    {
                        if let Some(homeserver) = self
                            .fields
                            .get(index)
                            .map(|(_, (_, homeserver, _))| homeserver.clone())
                        {
                            return match homeserver.parse::<Url>() {
                                Ok(uri) => {
                                    let content_store = content_store.clone();
                                    self.waiting = true;
                                    Command::perform(
                                        Client::new(uri, None, content_store),
                                        |result| {
                                            result.map_or_else(
                                                |err| TopLevelMessage::Error(Box::new(err)),
                                                TopLevelMessage::ClientCreated,
                                            )
                                        },
                                    )
                                }
                                Err(err) => {
                                    self.on_error(ClientError::UrlParse(homeserver.clone(), err))
                                }
                            };
                        }
                    }
                }
            }
            Message::AuthStep(step) => match step {
                Some(step) => {
                    self.current_error = String::default();
                    self.waiting = false;
                    self.fields.clear();
                    self.choices.clear();
                    self.can_go_back = step.can_go_back;

                    if let Some(step) = step.step {
                        match step {
                            Step::Choice(choice) => {
                                for option in choice.options {
                                    self.choices.insert(option, Default::default());
                                }
                                self.current_step = AuthPart::Step(AuthType::Choice);
                            }
                            Step::Form(form) => {
                                for field in form.fields {
                                    self.fields.push((
                                        field.name,
                                        (Default::default(), Default::default(), field.r#type),
                                    ));
                                }
                                self.current_step = AuthPart::Step(AuthType::Form);
                            }
                            _ => todo!("Implement waiting"),
                        }
                    }
                }
                None => {
                    self.waiting = true;
                    // If these unwraps fail, then something is very wrong, so we abort here.
                    // (How can there be no client, but we get authenticated?)
                    // We *can* recover from here but it's not worth the effort
                    let auth_status = client.unwrap().auth_status();
                    let homeserver = client.unwrap().inner().homeserver_url().to_string();
                    let session_file = content_store.session_file().to_path_buf();
                    return Command::perform(
                        async move {
                            if let AuthStatus::Complete(session) = auth_status {
                                let session = Session {
                                    homeserver,
                                    session_token: session.session_token,
                                    user_id: session.user_id.to_string(),
                                };

                                // This should never ever fail in our case, if it does something is very very very wrong
                                let ser = toml::ser::to_vec(&session).unwrap();
                                tokio::fs::write(session_file, ser).await?;
                            }
                            Ok(TopLevelMessage::LoginComplete(None))
                        },
                        |result| result.unwrap_or_else(|err| TopLevelMessage::Error(Box::new(err))),
                    );
                }
            },
        }
        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.current_error = error.to_string();
        self.reset_to_first_step();

        Command::none()
    }
}
