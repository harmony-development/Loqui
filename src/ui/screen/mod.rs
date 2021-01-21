pub mod guild_discovery;
pub mod login;
pub mod logout;
pub mod main;

pub use guild_discovery::GuildDiscovery;
pub use login::LoginScreen;
pub use logout::Logout as LogoutScreen;
pub use main::MainScreen;

use crate::{
    client::{
        content::{ContentStore, ImageHandle, ThumbnailCache},
        error::{ClientError, ClientResult},
        message::{Message as IcyMessage, MessageId},
        Client, PostProcessEvent, Session,
    },
    ui::style::Theme,
};

use flume::Receiver;
use harmony_rust_sdk::{
    api::{
        chat::{
            event::{Event, GuildAddedToList, GuildUpdated, ProfileUpdated},
            GetGuildListRequest,
        },
        harmonytypes::Override,
    },
    client::{
        api::{
            auth::AuthStepResponse,
            chat::{
                guild::{get_guild, get_guild_list},
                message::{SendMessage, SendMessageSelfBuilder},
                profile::get_user,
                GuildId, UserId,
            },
            harmonytypes::Message as HarmonyMessage,
            rest::FileId,
        },
        error::ClientError as InnerClientError,
    },
};
use iced::{executor, Application, Command, Element, Subscription};
use iced_futures::BoxStream;
use std::{hash::Hash, hash::Hasher, sync::Arc, time::Duration};

#[derive(Debug)]
pub enum Message {
    LoginScreen(login::Message),
    LogoutScreen(logout::Message),
    MainScreen(main::Message),
    GuildDiscovery(guild_discovery::Message),
    PopScreen,
    PushScreen(Box<Screen>),
    LoginComplete(Option<Client>),
    ClientCreated(Client),
    Nothing,
    DownloadedThumbnail {
        thumbnail_url: FileId,
        thumbnail: ImageHandle,
    },
    EventsReceived(Vec<Event>),
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
}

impl Screen {
    fn on_error(&mut self, error: ClientError) -> Command<Message> {
        match self {
            Screen::Login(screen) => screen.on_error(error),
            Screen::Logout(screen) => screen.on_error(error),
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
        if self.stack.len() > 1 {
            let screen = self.stack.pop();
            log::debug!("Popping a screen {:?}", screen);
            screen
        } else {
            None
        }
    }
}

pub struct ScreenManager {
    theme: Theme,
    screens: ScreenStack,
    client: Option<Client>,
    content_store: Arc<ContentStore>,
    thumbnail_cache: ThumbnailCache,
}

impl ScreenManager {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        Self {
            theme: Theme::default(),
            screens: ScreenStack::new(Screen::Login(LoginScreen::new(content_store.clone()))),
            client: None,
            content_store,
            thumbnail_cache: ThumbnailCache::default(),
        }
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
                        session.homeserver.parse().unwrap(),
                        Some(session.into()),
                        content_store.clone(),
                    )
                    .await
                },
                |result| match result {
                    Ok(client) => Message::LoginComplete(Some(client)),
                    Err(err) => Message::Error(Box::new(err)),
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
            Message::ClientCreated(client) => {
                self.client = Some(client);
                let inner = self.client.as_ref().unwrap().inner().clone();
                return Command::perform(
                    async move {
                        inner.begin_auth().await?;
                        inner.next_auth_step(AuthStepResponse::Initial).await
                    },
                    |result| match result {
                        Ok(step) => Message::LoginScreen(login::Message::AuthStep(step)),
                        Err(err) => Message::Error(Box::new(err.into())),
                    },
                );
            }
            Message::LoginComplete(maybe_client) => {
                if let Some(client) = maybe_client {
                    self.client = Some(client); // This is the only place we set a main screen [tag:client_set_before_main_view]
                }
                self.screens
                    .push(Screen::Main(Box::new(MainScreen::default())));

                let client = self.client.as_mut().unwrap();
                let inner = client.inner().clone();
                client.user_id = Some(inner.auth_status().session().unwrap().user_id);
                return Command::perform(
                    async move {
                        let guilds = get_guild_list(&inner, GetGuildListRequest {}).await?.guilds;
                        let events = guilds
                            .into_iter()
                            .map(|guild| {
                                Event::GuildAddedToList(GuildAddedToList {
                                    guild_id: guild.guild_id,
                                    homeserver: guild.host,
                                })
                            })
                            .collect();
                        Ok(events)
                    },
                    |result| match result {
                        Err(err) => Message::Error(Box::new(err)),
                        Ok(events) => Message::EventsReceived(events),
                    },
                );
            }
            Message::PopScreen => {
                self.screens.pop();
            }
            Message::PushScreen(screen) => {
                self.screens.push(*screen);
            }
            Message::MessageSent {
                message_id,
                transaction_id,
                guild_id,
                channel_id,
            } => {
                if let Some(channel) = self
                    .client
                    .as_mut()
                    .map(|client| client.get_channel(guild_id, channel_id))
                    .flatten()
                {
                    if let Some(msg) = channel
                        .messages
                        .iter_mut()
                        .find(|msg| msg.id.transaction_id() == Some(transaction_id))
                    {
                        msg.id = MessageId::Ack(message_id);
                    }
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
                                Err(_) => Message::SendMessage {
                                    message,
                                    retry_after: retry_after + Duration::from_secs(1),
                                    channel_id,
                                    guild_id,
                                },
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
            Message::EventsReceived(events) => {
                if let Some(client) = self.client.as_mut() {
                    let mut cmds = Vec::with_capacity(events.len());

                    for result in events
                        .into_iter()
                        .map(|event| client.process_event(event))
                        .collect::<Vec<_>>()
                    {
                        match result {
                            PostProcessEvent::FetchThumbnails(ids) => {
                                for id in ids {
                                    cmds.push(make_thumbnail_command(
                                        client,
                                        id,
                                        &self.thumbnail_cache,
                                    ));
                                }
                            }
                            PostProcessEvent::FetchProfile(user_id) => {
                                let inner = client.inner().clone();
                                cmds.push(Command::perform(
                                    async move {
                                        let profile =
                                            get_user(&inner, UserId::new(user_id)).await?;
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
                                        Ok(event)
                                    },
                                    |result| match result {
                                        Ok(event) => Message::EventsReceived(vec![event]),
                                        Err(err) => Message::Error(Box::new(err)),
                                    },
                                ));
                            }
                            PostProcessEvent::GoToFirstMsgOnChannel(channel_id) => {
                                if let Some(Screen::Main(screen)) = self
                                    .screens
                                    .stack
                                    .iter_mut()
                                    .find(|screen| matches!(screen, Screen::Main(_)))
                                {
                                    cmds.push(screen.update(
                                        main::Message::ScrollToBottom(channel_id),
                                        client,
                                        &self.thumbnail_cache,
                                    ));
                                }
                            }
                            PostProcessEvent::FetchGuildData(guild_id) => {
                                let inner = client.inner().clone();
                                cmds.push(Command::perform(
                                    async move {
                                        let guild_data =
                                            get_guild(&inner, GuildId::new(guild_id)).await?;
                                        let event = Event::EditedGuild(GuildUpdated {
                                            guild_id,
                                            metadata: guild_data.metadata,
                                            name: guild_data.guild_name,
                                            picture: guild_data.guild_picture,
                                            update_name: true,
                                            update_picture: true,
                                            update_metadata: true,
                                        });
                                        Ok(event)
                                    },
                                    |result| match result {
                                        Ok(event) => Message::EventsReceived(vec![event]),
                                        Err(err) => Message::Error(Box::new(err)),
                                    },
                                ));
                            }
                            PostProcessEvent::Nothing => {}
                        }
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
                if let Some(client) = self.client.as_mut() {
                    client.process_get_message_history_response(
                        guild_id,
                        channel_id,
                        messages,
                        reached_top,
                    );
                    // Safe unwrap
                    client
                        .get_channel(guild_id, channel_id)
                        .unwrap()
                        .loading_messages_history = false;
                    //return make_thumbnail_commands(client, thumbnail_urls, &self.thumbnail_cache);
                }
            }
            Message::Error(err) => {
                log::error!("{}", err);

                if let ClientError::Internal(InnerClientError::Grpc(status)) = err.as_ref() {
                    if status.message() == "invalid-session" {
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
        let mut sub = Subscription::none();

        /*if let Some(client) = &self.client {
            if client.auth_status().is_authenticated()
                && client.should_subscribe_to_events.load(Ordering::Relaxed)
            {
                let sources = client.subscribe_to();
                let inner = client.inner().clone();
                let (tx, rx) = flume::unbounded();

                tokio::task::spawn_blocking(|| async move {
                    let mut events = inner.subscribe_events(sources).await.unwrap().0;

                    loop {
                        if let Some(event) = events.next().await {
                            tx.send(event.map_err(ClientError::Internal)).unwrap();
                        }
                    }
                });

                sub = Subscription::batch(vec![
                    sub,
                    Subscription::from_recipe(SyncRecipe { chan: rx }).map(|result| match result {
                        Ok(response) => Message::SyncResponse(response),
                        Err(err) => Message::MatrixError(Box::new(err)),
                    }),
                ]);

                client
                    .should_subscribe_to_events
                    .store(false, Ordering::Relaxed);
            }
        }*/

        sub
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
        }
    }
}

pub struct SyncRecipe {
    chan: Receiver<ClientResult<Event>>,
}

impl<H, I> iced_futures::subscription::Recipe<H, I> for SyncRecipe
where
    H: Hasher,
{
    type Output = ClientResult<Event>;

    fn hash(&self, state: &mut H) {
        std::any::TypeId::of::<Self>().hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<I>) -> BoxStream<Self::Output> {
        Box::pin(self.chan.into_stream())
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
                    Ok(raw) => Ok((thumbnail_url, ImageHandle::from_memory(raw))),
                    Err(_) => {
                        let download_task = harmony_rust_sdk::client::api::rest::download(
                            &inner,
                            thumbnail_url.clone(),
                        );
                        let resp = download_task.await?;
                        match resp.bytes().await {
                            Ok(raw_data) => tokio::fs::write(content_path, &raw_data)
                                .await
                                .map(|_| {
                                    (thumbnail_url, ImageHandle::from_memory(raw_data.to_vec()))
                                })
                                .map_err(Into::into),
                            Err(err) => Err(err)
                                .map_err(harmony_rust_sdk::client::error::ClientError::Reqwest)
                                .map_err(Into::into),
                        }
                    }
                }
            },
            |result| match result {
                Ok((thumbnail_id, thumbnail)) => Message::DownloadedThumbnail {
                    thumbnail_url: thumbnail_id,
                    thumbnail,
                },
                Err(err) => Message::Error(Box::new(err)),
            },
        )
    } else {
        Command::none()
    }
}
