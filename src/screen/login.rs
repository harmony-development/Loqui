use super::Message as TopLevelMessage;
use crate::{
    client::{content::ContentStore, error::ClientError, Client, Session},
    component::*,
    label, label_button,
    screen::{ClientExt, ResultExt},
    style::{Theme, DEF_SIZE, ERROR_COLOR, PADDING},
};
use client::{
    error::ClientResult,
    harmony_rust_sdk::{
        api::{
            auth::{auth_step::Step, next_step_request::form_fields::Field},
            exports::hrpc::url::Url,
        },
        client::{
            api::{
                auth::{AuthStep, AuthStepResponse},
                chat::{profile, UserId},
            },
            AuthStatus,
        },
    },
    smol_str::SmolStr,
    AHashMap, IndexMap, InnerSession,
};
use std::sync::Arc;

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

impl Default for AuthPart {
    fn default() -> Self {
        Self::Homeserver
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    FieldChanged(SmolStr, String),
    ProceedWithChoice(SmolStr),
    Proceed,
    GoBack,
    AuthStep(Option<AuthStep>),
    UseSession(Session),
}

#[derive(Debug, Default)]
pub struct LoginScreen {
    fields: IndexMap<SmolStr, (text_input::State, String, String)>,
    choices: AHashMap<SmolStr, button::State>,
    proceed: button::State,
    back: button::State,
    saved_sessions: pick_list::State<Session>,

    current_step: AuthPart,
    can_go_back: bool,
    /// The error formatted as a string to be displayed to the user.
    current_error: String,
    pub waiting: bool,
}

impl LoginScreen {
    pub fn new() -> Self {
        let mut screen = Self::default();
        screen.reset_to_first_step();
        screen
    }

    pub fn reset_to_first_step(&mut self) {
        self.waiting = false;
        self.can_go_back = false;
        self.current_step = AuthPart::Homeserver;
        self.fields.clear();
        self.choices.clear();
        self.fields.insert(
            SmolStr::new_inline("homeserver"),
            (
                Default::default(),
                "https://chat.harmonyapp.io:2289".to_string(),
                "text".to_string(),
            ),
        );
    }

    pub fn view(&mut self, theme: Theme, content_store: &Arc<ContentStore>) -> Element<Message> {
        if self.waiting {
            return fill_container(label!("Please wait...").size(DEF_SIZE + 10))
                .style(theme)
                .into();
        }

        let mut widgets = Vec::with_capacity(self.fields.len() + self.choices.len() + 1);

        if !self.current_error.is_empty() {
            let error_text = label!(&self.current_error).color(ERROR_COLOR).size(DEF_SIZE - 2);
            widgets.push(error_text.into());
        }

        if let (AuthPart::Homeserver, Ok(read_dir)) =
            (self.current_step, std::fs::read_dir(content_store.sessions_dir()))
        {
            let saved_sessions = read_dir
                .flatten()
                .filter(|entry| entry.file_name().to_str() != Some("latest"))
                .flat_map(|entry| {
                    let raw = std::fs::read(entry.path()).ok()?;
                    toml::de::from_slice::<Session>(&raw).ok()
                })
                .collect::<Vec<_>>();
            let message = if saved_sessions.is_empty() {
                "No sessions"
            } else {
                "Select a session"
            };
            let session_list = PickList::new(
                &mut self.saved_sessions,
                saved_sessions,
                Some(Session {
                    user_name: SmolStr::new_inline(message),
                    ..Default::default()
                }),
                Message::UseSession,
            )
            .style(theme);
            widgets.push(session_list.into());
        }

        for (name, (state, value, r#type)) in self.fields.iter_mut() {
            let namee = name.clone();
            let mut input = TextInput::new(state, name, value, move |new| Message::FieldChanged(namee.clone(), new))
                .padding(PADDING / 2)
                .style(theme);
            input = match r#type.as_str() {
                "password" | "new-password" => input.password(),
                _ => input,
            };
            widgets.push(input.into());
        }

        if !self.choices.is_empty() {
            let mut sorted_choices = self.choices.iter_mut().collect::<Vec<_>>();
            sorted_choices.sort_unstable_by_key(|(name, _)| name.as_str());
            for (name, state) in &mut self.choices {
                widgets.push(
                    Button::new(state, label!(name.as_str()))
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

        fill_container(field_panel).style(theme).into()
    }

    pub fn update(
        &mut self,
        client: Option<&Client>,
        msg: Message,
        content_store: &Arc<ContentStore>,
    ) -> Command<TopLevelMessage> {
        fn respond(screen: &mut LoginScreen, client: &Client, response: AuthStepResponse) -> Command<TopLevelMessage> {
            screen.waiting = true;
            client.mk_cmd(
                |inner| async move { inner.next_auth_step(response).await },
                |step| TopLevelMessage::login(Message::AuthStep(step)),
            )
        }

        match msg {
            Message::FieldChanged(field, value) => {
                if let Some((_, val, _)) = self.fields.get_mut(&field) {
                    *val = value;
                }
            }
            Message::GoBack => {
                if let Some(client) = client {
                    self.waiting = true;
                    return client.mk_cmd(
                        |inner| async move { inner.prev_auth_step().await },
                        |step| TopLevelMessage::login(Message::AuthStep(Some(step))),
                    );
                }
            }
            Message::ProceedWithChoice(choice) => {
                if let Some(client) = client {
                    let response = AuthStepResponse::Choice(choice.into());
                    return respond(self, client, response);
                }
            }
            Message::UseSession(session) => {
                let content_store = content_store.clone();
                return Command::perform(
                    async move {
                        let Session {
                            session_token,
                            user_name: _,
                            user_id,
                            homeserver,
                        } = session;
                        let client = Client::new(
                            homeserver.parse().unwrap(),
                            Some(InnerSession {
                                user_id: user_id.parse().unwrap(),
                                session_token: session_token.into(),
                            }),
                            content_store,
                        )
                        .await?;
                        let user_id = client.user_id.unwrap();
                        let user_profile = profile::get_user(client.inner(), UserId::new(user_id)).await?;
                        let session_file = client.content_store().session_path(&homeserver, user_id);
                        let latest = client.content_store().latest_session_file();
                        let _ = tokio::fs::remove_file(latest).await;
                        tokio::fs::hard_link(session_file.as_path(), latest)
                            .await
                            .map_err(|err| {
                                ClientError::Custom(format!(
                                    "couldn't link session file ({}) to latest session ({}): {}",
                                    session_file.to_string_lossy(),
                                    latest.to_string_lossy(),
                                    err
                                ))
                            })?;
                        Ok((client, user_profile))
                    },
                    |result: ClientResult<_>| {
                        result.map_to_msg_def(|(client, profile)| {
                            TopLevelMessage::LoginComplete(Some(client), Some(profile))
                        })
                    },
                );
            }
            Message::Proceed => {
                if let (Some(client), AuthPart::Step(AuthType::Form)) = (client, self.current_step) {
                    let response = AuthStepResponse::form(
                        self.fields
                            .iter()
                            .map(|(_, (_, value, r#type))| match r#type.as_str() {
                                "number" => Field::Number(value.parse().unwrap()),
                                "password" => Field::Bytes(value.as_bytes().to_vec()),
                                "new-password" => Field::Bytes(value.as_bytes().to_vec()),
                                _ => Field::String(value.clone()),
                            })
                            .collect(),
                    );
                    return respond(self, client, response);
                } else if let AuthPart::Homeserver = &self.current_step {
                    if let Some(homeserver) = self
                        .fields
                        .get("homeserver")
                        .map(|(_, homeserver, _)| homeserver.clone())
                    {
                        return match homeserver.parse::<Url>() {
                            Ok(uri) => {
                                let content_store = content_store.clone();
                                self.waiting = true;
                                Command::perform(Client::new(uri, None, content_store), |result| {
                                    result.map_to_msg_def(TopLevelMessage::ClientCreated)
                                })
                            }
                            Err(err) => self.on_error(ClientError::UrlParse(homeserver.clone(), err)),
                        };
                    }
                }
            }
            Message::AuthStep(step) => {
                match step {
                    Some(step) => {
                        self.current_error = String::default();
                        self.waiting = false;
                        self.fields.clear();
                        self.choices.clear();
                        self.can_go_back = step.can_go_back;

                        if let Some(step) = step.step {
                            match step {
                                Step::Choice(choice) => {
                                    self.choices
                                        .extend(choice.options.into_iter().map(|opt| (opt.into(), Default::default())));
                                    self.current_step = AuthPart::Step(AuthType::Choice);
                                }
                                Step::Form(form) => {
                                    self.fields.extend(form.fields.into_iter().map(|field| {
                                        (
                                            field.name.into(),
                                            (Default::default(), Default::default(), field.r#type),
                                        )
                                    }));
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
                        let homeserver = SmolStr::new(client.unwrap().inner().homeserver_url().as_str());
                        let content_store = client.unwrap().content_store_arc();
                        let inner = client.unwrap().inner_arc();
                        return Command::perform(
                            async move {
                                if let AuthStatus::Complete(session) = auth_status {
                                    let session_file = content_store.session_path(&homeserver, session.user_id);
                                    let user_profile = profile::get_user(&inner, UserId::new(session.user_id)).await?;
                                    let session = Session {
                                        homeserver,
                                        session_token: session.session_token.into(),
                                        user_id: session.user_id.to_string().into(),
                                        user_name: user_profile.user_name.as_str().into(),
                                    };

                                    // This should never ever fail in our case, if it does something is very very very wrong
                                    let ser = toml::ser::to_vec(&session).unwrap();
                                    tokio::fs::write(session_file.as_path(), ser).await.map_err(|err| {
                                        ClientError::Custom(format!(
                                            "couldn't write session file to {}: {}",
                                            session_file.to_string_lossy(),
                                            err
                                        ))
                                    })?;
                                    let latest = content_store.latest_session_file();
                                    let _ = tokio::fs::remove_file(latest).await;
                                    tokio::fs::hard_link(session_file.as_path(), latest)
                                        .await
                                        .map_err(|err| {
                                            ClientError::Custom(format!(
                                                "couldn't link session file ({}) to latest session ({}): {}",
                                                session_file.to_string_lossy(),
                                                latest.to_string_lossy(),
                                                err
                                            ))
                                        })?;
                                    Ok(Some(user_profile))
                                } else {
                                    Ok(None)
                                }
                            },
                            |result: ClientResult<_>| {
                                result.map_to_msg_def(|profile| TopLevelMessage::LoginComplete(None, profile))
                            },
                        );
                    }
                }
            }
        }
        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.current_error = error.to_string();
        self.reset_to_first_step();

        Command::none()
    }
}
