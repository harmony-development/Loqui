pub mod create_channel;
pub mod guild_discovery;
pub mod login;
pub mod logout;
pub mod main;

pub use create_channel::ChannelCreation;
pub use guild_discovery::GuildDiscovery;
use iced_futures::subscription::Recipe;
pub use login::LoginScreen;
pub use logout::Logout as LogoutScreen;
pub use main::MainScreen;

use crate::{
    client::{
        content::{ContentStore, ImageHandle, ThumbnailCache},
        error::ClientError,
        message::{Message as IcyMessage, MessageId},
        Client, PostProcessEvent, Session,
    },
    ui::style::Theme,
};

use harmony_rust_sdk::{
    api::{
        chat::{
            event::{Event, GuildAddedToList, GuildUpdated, ProfileUpdated},
            GetGuildListRequest,
        },
        exports::hrpc::url::Url,
        harmonytypes::Override,
    },
    client::{
        api::{
            auth::AuthStepResponse,
            chat::{
                guild::{get_guild, get_guild_list},
                message::{SendMessage, SendMessageSelfBuilder},
                profile::get_user,
                EventSource, GuildId, UserId,
            },
            harmonytypes::Message as HarmonyMessage,
            rest::FileId,
        },
        EventsSocket,
    },
};
use iced::{executor, futures, Application, Command, Element, Subscription};
use std::{
    hash::Hasher,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum Message {
    LoginScreen(login::Message),
    LogoutScreen(logout::Message),
    MainScreen(main::Message),
    GuildDiscovery(guild_discovery::Message),
    ChannelCreation(create_channel::Message),
    PopScreen,
    PushScreen(Box<Screen>),
    Logout(Box<Screen>),
    LoginComplete(Option<Client>),
    ClientCreated(Client),
    Nothing,
    DownloadedThumbnail {
        thumbnail_url: FileId,
        thumbnail: ImageHandle,
    },
    EventsReceived(Vec<Event>),
    UpdateTypings(Vec<TypingMember>),
    SocketEvent {
        socket: Box<EventsSocket>,
        event: Option<harmony_rust_sdk::client::error::ClientResult<Event>>,
    },
    GetEventsBackwardsResponse {
        messages: Vec<HarmonyMessage>,
        reached_top: bool,
        guild_id: u64,
        channel_id: u64,
    },
    MessageSent {
        message_id: u64,
        transaction_id: u64,
        guild_id: u64,
        channel_id: u64,
    },
    SendMessage {
        message: IcyMessage,
        retry_after: Duration,
        guild_id: u64,
        channel_id: u64,
    },
    /// Sent whenever an error occurs.
    Error(Box<ClientError>),
}

#[derive(Debug)]
pub enum Screen {
    Logout(LogoutScreen),
    Login(LoginScreen),
    Main(Box<MainScreen>),
    GuildDiscovery(GuildDiscovery),
    ChannelCreation(ChannelCreation),
}

impl Screen {
    fn on_error(&mut self, error: ClientError) -> Command<Message> {
        match self {
            Screen::Login(screen) => screen.on_error(error),
            Screen::Logout(screen) => screen.on_error(error),
            Screen::GuildDiscovery(screen) => screen.on_error(error),
            Screen::ChannelCreation(screen) => screen.on_error(error),
            _ => Command::none(),
        }
    }
}

pub struct ScreenStack {
    stack: Vec<Screen>,
}

impl ScreenStack {
    pub fn new(initial_screen: Screen) -> Self {
        Self {
            // Make sure we can't create a `ScreenStack` without screen to ensure that stack can't be empty [tag:screenstack_cant_start_empty]
            stack: vec![initial_screen],
        }
    }

    pub fn current(&self) -> &Screen {
        self.stack.last().unwrap() // this is safe cause of [ref:screenstack_cant_become_empty] [ref:screenstack_cant_start_empty]
    }

    pub fn current_mut(&mut self) -> &mut Screen {
        self.stack.last_mut().unwrap() // this is safe cause of [ref:screenstack_cant_become_empty] [ref:screenstack_cant_start_empty]
    }

    pub fn clear(&mut self, screen: Screen) -> Vec<Screen> {
        log::debug!(
            "Clearing all screens in the stack and replacing it with {:?}",
            screen
        );

        let mut temp = Vec::with_capacity(self.stack.len());
        temp.append(&mut self.stack);

        self.stack.push(screen);

        temp
    }

    pub fn push(&mut self, screen: Screen) {
        log::debug!("Pushing a screen onto stack {:?}", screen);
        self.stack.push(screen)
    }

    pub fn pop(&mut self) -> Option<Screen> {
        // There must at least one screen remain to ensure [tag:screenstack_cant_become_empty]
        (self.stack.len() > 1).then(|| {
            let screen = self.stack.pop();
            log::debug!("Popping a screen {:?}", screen);
            screen.unwrap()
        })
    }
}

pub struct ScreenManager {
    theme: Theme,
    screens: ScreenStack,
    client: Option<Client>,
    content_store: Arc<ContentStore>,
    thumbnail_cache: ThumbnailCache,
    sources_to_add: Vec<EventSource>,
    socket_reset: bool,
}

impl ScreenManager {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        Self {
            theme: Theme::default(),
            screens: ScreenStack::new(Screen::Login(LoginScreen::new(content_store.clone()))),
            client: None,
            content_store,
            thumbnail_cache: ThumbnailCache::default(),
            sources_to_add: vec![],
            socket_reset: false,
        }
    }

    fn process_post_event(&mut self, post: PostProcessEvent) -> Command<Message> {
        if let Some(client) = self.client.as_mut() {
            match post {
                PostProcessEvent::FetchThumbnail(id) => {
                    return make_thumbnail_command(client, id, &self.thumbnail_cache);
                }
                PostProcessEvent::FetchProfile(user_id) => {
                    let inner = client.inner().clone();
                    return Command::perform(
                        async move {
                            let profile = get_user(&inner, UserId::new(user_id)).await?;
                            let event = Event::ProfileUpdated(ProfileUpdated {
                                user_id,
                                new_avatar: profile.user_avatar,
                                new_status: profile.user_status,
                                new_username: profile.user_name,
                                is_bot: profile.is_bot,
                                update_is_bot: true,
                                update_status: true,
                                update_avatar: true,
                                update_username: true,
                            });
                            Ok(vec![event])
                        },
                        |result| {
                            result.map_or_else(
                                |err| Message::Error(Box::new(err)),
                                Message::EventsReceived,
                            )
                        },
                    );
                }
                PostProcessEvent::GoToFirstMsgOnChannel(channel_id) => {
                    if let Some(Screen::Main(screen)) = self
                        .screens
                        .stack
                        .iter_mut()
                        .find(|screen| matches!(screen, Screen::Main(_)))
                    {
                        return screen.update(
                            main::Message::ScrollToBottom(channel_id),
                            client,
                            &self.thumbnail_cache,
                        );
                    }
                }
                PostProcessEvent::FetchGuildData(guild_id) => {
                    let inner = client.inner().clone();
                    return Command::perform(
                        async move {
                            let guild_data = get_guild(&inner, GuildId::new(guild_id)).await?;
                            let event = Event::EditedGuild(GuildUpdated {
                                guild_id,
                                metadata: guild_data.metadata,
                                name: guild_data.guild_name,
                                picture: guild_data.guild_picture,
                                update_name: true,
                                update_picture: true,
                                update_metadata: true,
                            });
                            Ok(vec![event])
                        },
                        |result| {
                            result.map_or_else(
                                |err| Message::Error(Box::new(err)),
                                Message::EventsReceived,
                            )
                        },
                    );
                }
                PostProcessEvent::Nothing => {}
            }
        }
        Command::none()
    }
}

impl Application for ScreenManager {
    type Executor = executor::Default;
    type Message = Message;
    type Flags = ContentStore;

    fn new(content_store: Self::Flags) -> (Self, Command<Self::Message>) {
        let content_store = Arc::new(content_store);
        let mut manager = ScreenManager::new(content_store.clone());
        let cmd = if content_store.session_file().exists() {
            let session_file = content_store.session_file().to_path_buf();
            if let Screen::Login(screen) = manager.screens.current_mut() {
                screen.waiting = true;
            }
            Command::perform(
                async move {
                    let session_raw = tokio::fs::read(session_file).await?;
                    let session: Session = toml::de::from_slice(&session_raw)
                        .map_err(|_| ClientError::MissingLoginInfo)?;
                    Client::new(
                        session.homeserver.parse::<Url>().unwrap(),
                        Some(session.into()),
                        content_store.clone(),
                    )
                    .await
                },
                |result| {
                    result.map_or_else(
                        |err| Message::Error(err.into()),
                        |client| Message::LoginComplete(Some(client)),
                    )
                },
            )
        } else {
            Command::none()
        };
        (manager, cmd)
    }

    fn title(&self) -> String {
        String::from("Icy Matrix")
    }

    fn update(&mut self, msg: Self::Message) -> Command<Self::Message> {
        match msg {
            Message::Nothing => {}
            Message::LoginScreen(msg) => {
                if let Screen::Login(screen) = self.screens.current_mut() {
                    return screen.update(self.client.as_ref(), msg, &self.content_store);
                }
            }
            Message::MainScreen(msg) => {
                if let (Screen::Main(screen), Some(client)) =
                    (self.screens.current_mut(), &mut self.client)
                {
                    return screen.update(msg, client, &self.thumbnail_cache);
                }
            }
            Message::LogoutScreen(msg) => {
                if let (Screen::Logout(screen), Some(client)) =
                    (self.screens.current_mut(), &mut self.client)
                {
                    return screen.update(msg, client);
                }
            }
            Message::GuildDiscovery(msg) => {
                if let (Screen::GuildDiscovery(screen), Some(client)) =
                    (self.screens.current_mut(), &self.client)
                {
                    return screen.update(msg, client);
                }
            }
            Message::ChannelCreation(msg) => {
                if let (Screen::ChannelCreation(screen), Some(client)) =
                    (self.screens.current_mut(), &self.client)
                {
                    return screen.update(msg, client);
                }
            }
            Message::ClientCreated(client) => {
                self.client = Some(client);
                let inner = self.client.as_ref().unwrap().inner().clone();
                return Command::perform(
                    async move {
                        inner.begin_auth().await?;
                        inner.next_auth_step(AuthStepResponse::Initial).await
                    },
                    |result| {
                        result.map_or_else(
                            |err| Message::Error(Box::new(err.into())),
                            |step| Message::LoginScreen(login::Message::AuthStep(step)),
                        )
                    },
                );
            }
            Message::SocketEvent { mut socket, event } => {
                if self.client.is_some() {
                    let mut cmds = Vec::with_capacity(2);

                    if let Some(ev) = event {
                        let cmd = match ev {
                            Ok(ev) => self.update(Message::EventsReceived(vec![ev])),
                            Err(err) => self.update(Message::Error(Box::new(err.into()))),
                        };
                        cmds.push(cmd);
                    }

                    if !self.socket_reset {
                        let subs = self.sources_to_add.drain(..).collect::<Vec<_>>();
                        cmds.push(Command::perform(
                            async move {
                                for sub in subs {
                                    if let Err(err) = socket.add_source(sub).await {
                                        log::error!("can't sub to source: {}", err);
                                    }
                                }
                                let event = socket.get_event().await;
                                Message::SocketEvent { socket, event }
                            },
                            |msg| msg,
                        ));
                    } else {
                        let client = self.client.as_ref().unwrap();
                        let sources = client.subscribe_to();
                        let inner = client.inner().clone();
                        cmds.push(Command::perform(
                            async move { inner.subscribe_events(sources).await },
                            |result| {
                                result.map_or_else(
                                    |err| Message::Error(Box::new(err.into())),
                                    |socket| Message::SocketEvent {
                                        socket: socket.into(),
                                        event: None,
                                    },
                                )
                            },
                        ));
                        self.socket_reset = false;
                    }

                    return Command::batch(cmds);
                }
            }
            Message::LoginComplete(maybe_client) => {
                if let Some(client) = maybe_client {
                    self.client = Some(client); // This is the only place we set a main screen [tag:client_set_before_main_view]
                }
                self.screens
                    .push(Screen::Main(Box::new(MainScreen::default())));

                let client = self.client.as_mut().unwrap();
                let sources = client.subscribe_to();
                let inner = client.inner().clone();
                let ws_cmd = Command::perform(
                    async move { inner.subscribe_events(sources).await },
                    |result| {
                        result.map_or_else(
                            |err| Message::Error(Box::new(err.into())),
                            |socket| Message::SocketEvent {
                                socket: socket.into(),
                                event: None,
                            },
                        )
                    },
                );
                let inner = client.inner().clone();
                client.user_id = Some(inner.auth_status().session().unwrap().user_id);
                let self_id = client.user_id.unwrap();
                let init = Command::perform(
                    async move {
                        let self_profile = get_user(&inner, UserId::new(self_id)).await?;
                        let guilds = get_guild_list(&inner, GetGuildListRequest {}).await?.guilds;
                        let mut events = guilds
                            .into_iter()
                            .map(|guild| {
                                Event::GuildAddedToList(GuildAddedToList {
                                    guild_id: guild.guild_id,
                                    homeserver: guild.host,
                                })
                            })
                            .collect::<Vec<_>>();
                        events.push(Event::ProfileUpdated(ProfileUpdated {
                            update_avatar: true,
                            update_is_bot: true,
                            update_status: true,
                            update_username: true,
                            is_bot: self_profile.is_bot,
                            new_avatar: self_profile.user_avatar,
                            new_status: self_profile.user_status,
                            new_username: self_profile.user_name,
                            user_id: self_id,
                        }));
                        Ok(events)
                    },
                    |result| {
                        result.map_or_else(
                            |err| Message::Error(Box::new(err)),
                            Message::EventsReceived,
                        )
                    },
                );
                return Command::batch(vec![ws_cmd, init]);
            }
            Message::PopScreen => {
                self.screens.pop();
            }
            Message::PushScreen(screen) => {
                self.screens.push(*screen);
            }
            Message::Logout(screen) => {
                self.client = None;
                self.socket_reset = false;
                self.screens.clear(*screen);
            }
            Message::MessageSent {
                message_id,
                transaction_id,
                guild_id,
                channel_id,
            } => {
                if let Some(msg) = self
                    .client
                    .as_mut()
                    .map(|client| client.get_channel(guild_id, channel_id))
                    .flatten()
                    .map(|channel| {
                        channel
                            .messages
                            .iter_mut()
                            .find(|msg| msg.id.transaction_id() == Some(transaction_id))
                    })
                    .flatten()
                {
                    msg.id = MessageId::Ack(message_id);
                }
            }
            Message::SendMessage {
                message,
                retry_after,
                guild_id,
                channel_id,
            } => {
                if let Some(channel) = self
                    .client
                    .as_mut()
                    .map(|client| client.get_channel(guild_id, channel_id))
                    .flatten()
                {
                    if retry_after.as_secs() == 0 {
                        channel.messages.push(message.clone());
                    }

                    let inner = self.client.as_ref().unwrap().inner().clone();

                    return Command::perform(
                        async move {
                            tokio::time::sleep(retry_after).await;

                            let send_message =
                                SendMessage::new(guild_id, channel_id, message.content.clone())
                                    .echo_id(message.id.transaction_id().unwrap())
                                    .attachments(
                                        message
                                            .attachments
                                            .clone()
                                            .into_iter()
                                            .map(|a| a.id)
                                            .collect::<Vec<_>>(),
                                    )
                                    .overrides(message.overrides.as_ref().map(|o| {
                                        Override {
                                            avatar: o
                                                .avatar_url
                                                .as_ref()
                                                .map_or_else(String::default, |id| id.to_string()),
                                            name: o.name.clone(),
                                            reason: o.reason.clone(),
                                        }
                                    }));

                            let send_result =
                                harmony_rust_sdk::client::api::chat::message::send_message(
                                    &inner,
                                    send_message,
                                )
                                .await;

                            match send_result {
                                Ok(resp) => Message::MessageSent {
                                    message_id: resp.message_id,
                                    transaction_id: message.id.transaction_id().unwrap(),
                                    channel_id,
                                    guild_id,
                                },
                                Err(err) => {
                                    log::error!("error occured when sending message: {}", err);
                                    Message::SendMessage {
                                        message,
                                        retry_after: retry_after + Duration::from_secs(1),
                                        channel_id,
                                        guild_id,
                                    }
                                }
                            }
                        },
                        |retry| retry,
                    );
                }
            }
            Message::DownloadedThumbnail {
                thumbnail_url,
                thumbnail,
            } => {
                self.thumbnail_cache.put_thumbnail(thumbnail_url, thumbnail);
            }
            Message::UpdateTypings(typing_members) => {
                if let Some(client) = self.client.as_mut() {
                    client.members.values_mut().for_each(|member| {
                        member.typing_in_channel = None;
                    });

                    for (id, typing) in typing_members {
                        if let Some(member) = client.get_member(id) {
                            member.typing_in_channel = Some(typing);
                        }
                    }
                }
            }
            Message::EventsReceived(events) => {
                if let Some(client) = self.client.as_mut() {
                    let processed = events
                        .into_iter()
                        .flat_map(|event| client.process_event(event))
                        .collect::<Vec<_>>();

                    let mut cmds = Vec::with_capacity(processed.len());

                    for sub in processed.iter().flat_map(|post| {
                        if let PostProcessEvent::FetchGuildData(id) = post {
                            Some(EventSource::Guild(*id))
                        } else {
                            None
                        }
                    }) {
                        self.sources_to_add.push(sub);
                    }

                    for cmd in processed
                        .into_iter()
                        .map(|post| self.process_post_event(post))
                    {
                        cmds.push(cmd);
                    }

                    return Command::batch(cmds);
                }
            }
            Message::GetEventsBackwardsResponse {
                messages,
                reached_top,
                guild_id,
                channel_id,
            } => {
                let posts = if let Some(client) = self.client.as_mut() {
                    // Safe unwrap
                    client
                        .get_channel(guild_id, channel_id)
                        .unwrap()
                        .loading_messages_history = false;
                    client.process_get_message_history_response(
                        guild_id,
                        channel_id,
                        messages,
                        reached_top,
                    )
                } else {
                    Vec::new()
                };

                let cmds = posts.into_iter().map(|post| self.process_post_event(post));

                return Command::batch(cmds);
            }
            Message::Error(err) => {
                log::error!("\n{}\n{:?}", err, err);

                if matches!(
                    &*err,
                    ClientError::Internal(harmony_rust_sdk::client::error::ClientError::Internal(
                        harmony_rust_sdk::api::exports::hrpc::client::ClientError::SocketError(_)
                    ))
                ) {
                    self.socket_reset = true;
                }

                if err.to_string().contains("connect error") {
                    self.screens
                        .clear(Screen::Login(LoginScreen::new(self.content_store.clone())));
                }

                if let ClientError::Internal(
                    harmony_rust_sdk::client::error::ClientError::Internal(
                        harmony_rust_sdk::api::exports::hrpc::client::ClientError::EndpointError {
                            raw_error,
                            ..
                        },
                    ),
                ) = err.as_ref()
                {
                    if raw_error
                        .iter()
                        .map(|b| *b as char)
                        .collect::<String>()
                        .contains("invalid-session")
                    {
                        self.screens
                            .clear(Screen::Login(LoginScreen::new(self.content_store.clone())));
                    }
                }

                return self.screens.current_mut().on_error(*err);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if let Some(client) = self.client.as_ref() {
            Subscription::from_recipe(ProcessTyping {
                typing: client
                    .members
                    .iter()
                    .filter_map(|(id, member)| Some((*id, member.typing_in_channel?)))
                    .collect(),
            })
            .map(Message::UpdateTypings)
        } else {
            Subscription::none()
        }
    }

    fn view(&mut self) -> Element<Self::Message> {
        match self.screens.current_mut() {
            Screen::Login(screen) => screen.view(self.theme).map(Message::LoginScreen),
            Screen::Logout(screen) => screen.view(self.theme).map(Message::LogoutScreen),
            Screen::Main(screen) => screen
                .view(
                    self.theme,
                    self.client.as_ref().unwrap(), // This will not panic cause [ref:client_set_before_main_view]
                    &self.thumbnail_cache,
                )
                .map(Message::MainScreen),
            Screen::GuildDiscovery(screen) => screen
                .view(self.theme, self.client.as_ref().unwrap()) // This will not panic cause [ref:client_set_before_main_view]
                .map(Message::GuildDiscovery),
            Screen::ChannelCreation(screen) => screen
                .view(self.theme, self.client.as_ref().unwrap()) // This will not panic cause [ref:client_set_before_main_view]
                .map(Message::ChannelCreation),
        }
    }
}

type TypingMember = (u64, (u64, u64, Instant));

pub struct ProcessTyping {
    typing: Vec<TypingMember>,
}

impl<H: Hasher, I> Recipe<H, I> for ProcessTyping {
    type Output = Vec<TypingMember>;

    fn hash(&self, state: &mut H) {
        use std::hash::Hash;

        for typing in &self.typing {
            let (id, (g, c, since)) = typing;
            id.hash(state);
            g.hash(state);
            c.hash(state);
            since.hash(state);
        }
    }

    fn stream(
        self: Box<Self>,
        _: iced_futures::BoxStream<I>,
    ) -> iced_futures::BoxStream<Self::Output> {
        let still_typing = self
            .typing
            .into_iter()
            .filter_map(|(id, (g, c, since))| {
                if since.elapsed().as_secs() >= 5 {
                    None
                } else {
                    Some((id, (g, c, since)))
                }
            })
            .collect::<Vec<_>>();

        Box::pin(futures::stream::once(async move { still_typing }))
    }
}

fn make_thumbnail_command(
    client: &Client,
    thumbnail_url: FileId,
    thumbnail_cache: &ThumbnailCache,
) -> Command<Message> {
    if !thumbnail_cache.has_thumbnail(&thumbnail_url) {
        let content_path = client.content_store().content_path(&thumbnail_url);

        let inner = client.inner().clone();

        Command::perform(
            async move {
                match tokio::fs::read(&content_path).await {
                    Ok(raw) => Ok(Message::DownloadedThumbnail {
                        thumbnail_url,
                        thumbnail: ImageHandle::from_memory(raw),
                    }),
                    Err(err) => {
                        log::warn!("couldn't read thumbnail from disk: {}", err);
                        let download_task = harmony_rust_sdk::client::api::rest::download(
                            &inner,
                            thumbnail_url.clone(),
                        );
                        let resp = download_task.await?;
                        match resp.bytes().await {
                            Ok(raw_data) => {
                                tokio::fs::write(content_path, &raw_data).await?;
                                Ok(Message::DownloadedThumbnail {
                                    thumbnail_url,
                                    thumbnail: ImageHandle::from_memory(raw_data.to_vec()),
                                })
                            }
                            Err(err) => {
                                Err(harmony_rust_sdk::client::error::ClientError::Reqwest(err)
                                    .into())
                            }
                        }
                    }
                }
            },
            |msg| msg.unwrap_or_else(|err| Message::Error(Box::new(err))),
        )
    } else {
        Command::none()
    }
}
