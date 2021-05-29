pub mod guild_discovery;
pub mod guild_settings;
pub mod login;
pub mod main;

pub use guild_discovery::GuildDiscovery;
pub use guild_settings::GuildSettings;
pub use login::LoginScreen;
pub use main::MainScreen;

use crate::{
    client::{
        channel::ChanPerms,
        content::{ContentStore, ImageHandle, ThumbnailCache},
        error::{ClientError, ClientResult},
        message::{Attachment, Message as IcyMessage, MessageId},
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
        rest::FileId,
    },
    client::{
        api::{
            auth::AuthStepResponse,
            chat::{
                guild::{get_guild, get_guild_list},
                permissions::{self, QueryPermissions, QueryPermissionsSelfBuilder},
                profile::{get_user, get_user_bulk},
                EventSource, GuildId, UserId,
            },
            harmonytypes::Message as HarmonyMessage,
        },
        Client as InnerClient, EventsSocket,
    },
};
use iced::{executor, Application, Command, Element, Subscription};
use std::{sync::Arc, time::Duration};

#[derive(Debug)]
pub enum Message {
    LoginScreen(login::Message),
    MainScreen(main::Message),
    GuildDiscovery(guild_discovery::Message),
    GuildSettings(guild_settings::Message),
    PopScreen,
    PushScreen(Box<Screen>),
    Logout(Box<Screen>),
    LoginComplete(Option<Client>),
    ClientCreated(Client),
    Nothing,
    DownloadedThumbnail {
        data: Attachment,
        thumbnail: ImageHandle,
        open: bool,
    },
    EventsReceived(Vec<Event>),
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
    MessageEdited {
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
        err: Option<Box<ClientError>>,
    },
    UpdateChanPerms(ChanPerms, u64, u64),
    /// Sent whenever an error occurs.
    Error(Box<ClientError>),
    Exit,
    ExitReady,
}

#[derive(Debug)]
pub enum Screen {
    Login(LoginScreen),
    Main(Box<MainScreen>),
    GuildDiscovery(GuildDiscovery),
    GuildSettings(GuildSettings),
}

impl Screen {
    fn on_error(&mut self, error: ClientError) -> Command<Message> {
        match self {
            Screen::Login(screen) => screen.on_error(error),
            Screen::GuildDiscovery(screen) => screen.on_error(error),
            Screen::Main(screen) => screen.on_error(error),
            Screen::GuildSettings(screen) => screen.on_error(error),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        match self {
            Screen::Main(screen) => screen.subscription(),
            _ => Subscription::none(),
        }
    }

    fn push_screen_cmd(screen: Screen) -> Command<Message> {
        Command::perform(
            async move { Message::PushScreen(Box::new(screen)) },
            |msg| msg,
        )
    }

    fn pop_screen_cmd() -> Command<Message> {
        Command::perform(async { Message::PopScreen }, |msg| msg)
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
        tracing::debug!(
            "Clearing all screens in the stack and replacing it with {:?}",
            screen
        );

        let mut temp = Vec::with_capacity(self.stack.len());
        temp.append(&mut self.stack);

        self.stack.push(screen);

        temp
    }

    pub fn push(&mut self, screen: Screen) {
        tracing::debug!("Pushing a screen onto stack {:?}", screen);
        self.stack.push(screen)
    }

    pub fn pop(&mut self) -> Option<Screen> {
        // There must at least one screen remain to ensure [tag:screenstack_cant_become_empty]
        (self.stack.len() > 1).then(|| {
            let screen = self.stack.pop();
            tracing::debug!("Popping a screen {:?}", screen);
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
    cur_socket: Option<Box<EventsSocket>>,
    socket_reset: bool,
    should_exit: bool,
}

impl ScreenManager {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        Self {
            theme: Theme::default(),
            screens: ScreenStack::new(Screen::Login(LoginScreen::new(content_store.clone()))),
            client: None,
            content_store,
            thumbnail_cache: ThumbnailCache::default(),
            cur_socket: None,
            socket_reset: false,
            should_exit: false,
        }
    }

    fn process_post_event(
        &mut self,
        post: PostProcessEvent,
        clip: &mut iced::Clipboard,
    ) -> Command<Message> {
        if let Some(client) = self.client.as_mut() {
            match post {
                PostProcessEvent::CheckPermsForChannel(guild_id, channel_id) => {
                    let inner = client.inner().clone();
                    return Command::perform(
                        async move {
                            let query = |check_for: &str| {
                                permissions::query_has_permission(
                                    &inner,
                                    QueryPermissions::new(guild_id, check_for.to_string())
                                        .channel_id(channel_id),
                                )
                            };
                            let manage = query("channels.manage.change-information").await?.ok;
                            let send_msg = query("messages.send").await?.ok;
                            Ok(ChanPerms {
                                manage_channel: manage,
                                send_msg,
                            })
                        },
                        move |res| {
                            res.map_or_else(
                                |err| Message::Error(Box::new(err)),
                                |perms| Message::UpdateChanPerms(perms, guild_id, channel_id),
                            )
                        },
                    );
                }
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
                            clip,
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
        "Crust".into()
    }

    fn update(&mut self, msg: Self::Message, clip: &mut iced::Clipboard) -> Command<Self::Message> {
        if let Some(client) = self.client.as_mut() {
            for member in client.members.values_mut() {
                member.typing_in_channel = member
                    .typing_in_channel
                    .filter(|(_, _, since)| since.elapsed().as_secs() < 5);
            }
        }

        match msg {
            Message::Nothing => {}
            Message::Exit => {
                let sock = self.cur_socket.take();
                return Command::perform(
                    async move {
                        if let Some(sock) = sock {
                            let _ = sock.close().await;
                        }
                    },
                    |_| Message::ExitReady,
                );
            }
            Message::ExitReady => self.should_exit = true,
            Message::LoginScreen(msg) => {
                if let Screen::Login(screen) = self.screens.current_mut() {
                    return screen.update(self.client.as_ref(), msg, &self.content_store);
                }
            }
            Message::MainScreen(msg) => {
                if let (Screen::Main(screen), Some(client)) =
                    (self.screens.current_mut(), &mut self.client)
                {
                    return screen.update(msg, client, &self.thumbnail_cache, clip);
                }
            }
            Message::GuildDiscovery(msg) => {
                if let (Screen::GuildDiscovery(screen), Some(client)) =
                    (self.screens.current_mut(), &self.client)
                {
                    return screen.update(msg, client);
                }
            }
            Message::GuildSettings(msg) => {
                if let (Screen::GuildSettings(screen), Some(client)) =
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
            Message::UpdateChanPerms(perms, guild_id, channel_id) => {
                if let Some(client) = self.client.as_mut() {
                    // [ref:channel_added_to_client]
                    client.get_channel(guild_id, channel_id).unwrap().user_perms = perms;
                }
            }
            Message::SocketEvent { mut socket, event } => {
                if self.client.is_some() {
                    let mut cmds = Vec::with_capacity(2);

                    if let Some(ev) = event {
                        tracing::debug!("event received from socket: {:?}", ev);
                        let cmd = match ev {
                            Ok(ev) => self.update(Message::EventsReceived(vec![ev]), clip),
                            Err(err) => self.update(Message::Error(Box::new(err.into())), clip),
                        };
                        cmds.push(cmd);
                    } else {
                        self.cur_socket = Some(socket.clone());
                    }

                    if !self.socket_reset {
                        cmds.push(Command::perform(
                            async move {
                                let ev;
                                loop {
                                    if let Some(event) = socket.get_event().await {
                                        ev = event;
                                        break;
                                    }
                                }
                                Message::SocketEvent {
                                    socket,
                                    event: Some(ev),
                                }
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
            Message::MessageEdited {
                guild_id,
                channel_id,
                message_id,
                err,
            } => {
                let client = self.client.as_mut().unwrap();

                if let Some(msg) = client
                    .get_channel(guild_id, channel_id)
                    .map(|c| {
                        c.messages
                            .iter_mut()
                            .find(|m| m.id.id() == Some(message_id))
                    })
                    .flatten()
                {
                    msg.being_edited = None;
                }

                if let Some(err) = err {
                    return self.update(Message::Error(err), clip);
                }
            }
            Message::SendMessage {
                message,
                retry_after,
                guild_id,
                channel_id,
            } => {
                if let Some(cmd) = self
                    .client
                    .as_mut()
                    .map(|c| c.send_msg_cmd(guild_id, channel_id, retry_after, message))
                    .flatten()
                {
                    return cmd;
                }
            }
            Message::DownloadedThumbnail {
                data,
                thumbnail,
                open,
            } => {
                let path = self.content_store.content_path(&data.id);
                self.thumbnail_cache
                    .put_thumbnail(data.id, thumbnail.clone());
                if open {
                    if let Screen::Main(screen) = self.screens.current_mut() {
                        screen.image_viewer_modal.inner_mut().image_handle =
                            Some((thumbnail, (path, data.name)));
                        screen.image_viewer_modal.show(true);
                    }
                }
            }
            Message::EventsReceived(events) => {
                if self.client.is_some() {
                    let processed = events
                        .into_iter()
                        .flat_map(|event| self.client.as_mut().unwrap().process_event(event))
                        .collect::<Vec<_>>();

                    let mut cmds = Vec::with_capacity(processed.len());

                    let sources_to_add = processed
                        .iter()
                        .flat_map(|post| {
                            if let PostProcessEvent::FetchGuildData(id) = post {
                                Some(EventSource::Guild(*id))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    if let Some(mut sock) = self.cur_socket.clone() {
                        cmds.push(Command::perform(
                            async move {
                                for source in sources_to_add {
                                    sock.add_source(source).await?;
                                }
                                Ok(())
                            },
                            |res| {
                                res.map_or_else(
                                    |err| Message::Error(Box::new(err)),
                                    |_| Message::Nothing,
                                )
                            },
                        ));
                    }

                    let mut fetch_users = Vec::with_capacity(64);

                    for post in processed {
                        if let PostProcessEvent::FetchProfile(user_id) = post {
                            fetch_users.push(user_id);
                        } else {
                            cmds.push(self.process_post_event(post, clip));
                        }
                    }

                    for chunk in fetch_users.chunks(64).map(|c| c.to_vec()) {
                        if !chunk.is_empty() {
                            let inner = self.client.as_ref().unwrap().inner().clone();
                            let fetch_users_cmd = Command::perform(
                                async move {
                                    let profiles = get_user_bulk(&inner, chunk.clone()).await?;
                                    Ok(profiles
                                        .users
                                        .into_iter()
                                        .zip(chunk.into_iter())
                                        .map(|(profile, user_id)| {
                                            Event::ProfileUpdated(ProfileUpdated {
                                                user_id,
                                                new_avatar: profile.user_avatar,
                                                new_status: profile.user_status,
                                                new_username: profile.user_name,
                                                is_bot: profile.is_bot,
                                                update_is_bot: true,
                                                update_status: true,
                                                update_avatar: true,
                                                update_username: true,
                                            })
                                        })
                                        .collect::<Vec<_>>())
                                },
                                |result| {
                                    result.map_or_else(
                                        |err| Message::Error(Box::new(err)),
                                        Message::EventsReceived,
                                    )
                                },
                            );
                            cmds.push(fetch_users_cmd);
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

                let cmds = posts
                    .into_iter()
                    .map(|post| self.process_post_event(post, clip));

                return Command::batch(cmds);
            }
            Message::Error(err) => {
                let err_disp = err.to_string();
                tracing::error!("{}\n{:?}", err_disp, err);

                if matches!(
                    &*err,
                    ClientError::Internal(harmony_rust_sdk::client::error::ClientError::Internal(
                        harmony_rust_sdk::api::exports::hrpc::client::ClientError::SocketError(_)
                    ))
                ) {
                    self.socket_reset = true;
                }

                if err_disp.contains("invalid-session") || err_disp.contains("connect error") {
                    self.update(
                        Message::Logout(
                            Screen::Login(LoginScreen::new(self.content_store.clone())).into(),
                        ),
                        clip,
                    );
                }

                return self.screens.current_mut().on_error(*err);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let time_sub = iced::time::every(Duration::from_secs(5)).map(|_| Message::Nothing);
        let main_sub = self.screens.current().subscription();

        Subscription::batch(vec![time_sub, main_sub])
    }

    fn view(&mut self) -> Element<Self::Message> {
        match self.screens.current_mut() {
            Screen::Login(screen) => screen.view(self.theme).map(Message::LoginScreen),
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
            Screen::GuildSettings(screen) => screen
                .view(self.theme, self.client.as_ref().unwrap())
                .map(Message::GuildSettings),
        }
    }

    fn should_exit(&self) -> bool {
        self.should_exit
    }
}

fn make_thumbnail_command(
    client: &Client,
    data: Attachment,
    thumbnail_cache: &ThumbnailCache,
) -> Command<Message> {
    if !thumbnail_cache.has_thumbnail(&data.id) {
        let content_path = client.content_store().content_path(&data.id);

        let inner = client.inner().clone();

        Command::perform(
            async move {
                match tokio::fs::read(&content_path).await {
                    Ok(raw) => {
                        let bgra = image::load_from_memory(&raw).unwrap().into_bgra8();
                        Ok(Message::DownloadedThumbnail {
                            data,
                            thumbnail: ImageHandle::from_pixels(
                                bgra.width(),
                                bgra.height(),
                                bgra.into_vec(),
                            ),
                            open: false,
                        })
                    }
                    Err(err) => {
                        tracing::warn!(
                            "couldn't read thumbnail for ID {} from disk: {}",
                            data.id,
                            err
                        );
                        let file = harmony_rust_sdk::client::api::rest::download_extract_file(
                            &inner,
                            data.id.clone(),
                        )
                        .await?;
                        tokio::fs::write(content_path, file.data()).await?;
                        let bgra = image::load_from_memory(file.data()).unwrap().into_bgra8();
                        Ok(Message::DownloadedThumbnail {
                            data,
                            thumbnail: ImageHandle::from_pixels(
                                bgra.width(),
                                bgra.height(),
                                bgra.into_vec(),
                            ),
                            open: false,
                        })
                    }
                }
            },
            |msg| msg.unwrap_or_else(|err| Message::Error(Box::new(err))),
        )
    } else {
        Command::none()
    }
}

async fn select_upload_files(
    inner: &InnerClient,
    content_store: Arc<ContentStore>,
) -> ClientResult<Vec<(FileId, String, String, usize)>> {
    use crate::client::content;
    use harmony_rust_sdk::client::api::rest::upload_extract_id;

    let handles = rfd::AsyncFileDialog::new()
        .pick_files()
        .await
        .ok_or_else(|| ClientError::Custom("File selection error".to_string()))?;
    let mut ids = Vec::with_capacity(handles.len());

    for handle in handles {
        match tokio::fs::read(handle.path()).await {
            Ok(data) => {
                let file_mimetype = content::infer_type_from_bytes(&data);
                let filename = content::get_filename(handle.path()).to_string();
                let filesize = data.len();

                let send_result =
                    upload_extract_id(inner, filename.clone(), file_mimetype.clone(), data).await;

                match send_result.map(FileId::Id) {
                    Ok(id) => {
                        if let Err(err) =
                            tokio::fs::hard_link(handle.path(), content_store.content_path(&id))
                                .await
                        {
                            tracing::warn!("An IO error occured while hard linking a file you tried to upload (this may result in a duplication of the file): {}", err);
                        }
                        ids.push((id, file_mimetype, filename, filesize));
                    }
                    Err(err) => {
                        tracing::error!("An error occured while trying to upload a file: {}", err);
                    }
                }
            }
            Err(err) => {
                tracing::error!("An IO error occured while trying to upload a file: {}", err);
            }
        }
    }
    Ok(ids)
}
