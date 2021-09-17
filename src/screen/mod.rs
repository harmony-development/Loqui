pub mod emote_management;
pub mod guild_discovery;
pub mod guild_settings;
pub mod login;
pub mod main;

pub use guild_discovery::GuildDiscovery;
pub use guild_settings::GuildSettings;
use image::imageops::FilterType;
pub use login::LoginScreen;
pub use main::MainScreen;

use crate::{
    client::{
        content::ContentStore,
        error::{ClientError, ClientResult},
        message::{Attachment, Message as IcyMessage, MessageId},
        Client, PostProcessEvent, Session,
    },
    component::*,
    style::{Theme, UserTheme, AVATAR_WIDTH, PROFILE_AVATAR_WIDTH},
};

use client::{
    bool_ext::BoolExt,
    content::ThemeRaw,
    harmony_rust_sdk::{
        self,
        api::{
            batch::BatchSameRequest,
            chat::{
                get_channel_messages_request::Direction,
                stream_event::{Event as ChatEvent, GuildAddedToList, GuildUpdated, PermissionUpdated},
                Event, GetGuildListRequest, GetGuildRequest, GetMessageRequest, Message as HarmonyMessage,
                QueryHasPermissionRequest,
            },
            emote::{
                stream_event::Event as EmoteEvent, EmotePackAdded, EmotePackEmotesUpdated, GetEmotePackEmotesRequest,
                GetEmotePacksRequest,
            },
            exports::{hrpc::encode_protobuf_message, prost::Message as ProstMessage},
            mediaproxy::{fetch_link_metadata_response::Data as FetchLinkData, FetchLinkMetadataRequest},
            profile::{stream_event::Event as ProfileEvent, GetProfileRequest, Profile, ProfileUpdated, UserStatus},
            rest::FileId,
            Endpoint,
        },
        client::{
            api::{auth::AuthStepResponse, chat::EventSource, profile::UpdateProfile, rest::download_extract_file},
            error::{ClientError as InnerClientError, InternalClientError as HrpcClientError},
            Client as InnerClient, EventsSocket,
        },
    },
    tracing::{debug, error, warn},
    OptionExt, Url,
};
use iced::{
    executor,
    futures::future::{self, ready},
    Application, Command, Element, Subscription,
};
use std::{
    array::IntoIter,
    borrow::Cow,
    convert::identity,
    future::Future,
    ops::Not,
    path::PathBuf,
    sync::{mpsc::Receiver, Arc},
    time::Duration,
};

use self::emote_management::ManageEmotesScreen;

#[derive(Debug, Clone)]
pub enum ScreenMessage {
    LoginScreen(login::Message),
    MainScreen(main::Message),
    GuildDiscovery(guild_discovery::Message),
    GuildSettings(guild_settings::Message),
    EmoteManagement(emote_management::Message),
}

#[derive(Debug, Clone)]
pub enum Message {
    ChildMessage(Box<ScreenMessage>),
    PopScreen,
    PushScreen(Box<Screen>),
    Logout(Box<Screen>),
    LoginComplete(Box<(Option<Client>, Option<Profile>)>),
    ClientCreated(Box<Client>),
    Nothing,
    DownloadedThumbnail {
        data: Attachment,
        thumbnail: Option<ImageHandle>,
        avatar: Option<(ImageHandle, ImageHandle)>,
        emote: Option<ImageHandle>,
        open: bool,
    },
    TryEventsReceived(Vec<ClientResult<Event>>),
    EventsReceived(Vec<Event>),
    InitialGuildLoad {
        guild_id: u64,
        events: ClientResult<Vec<ClientResult<Event>>>,
    },
    InitialChannelLoad {
        guild_id: u64,
        channel_id: u64,
        events: ClientResult<Box<Message>>,
    },
    SocketEvent {
        socket: Box<EventsSocket>,
        event: Option<Result<Event, ClientError>>,
    },
    GetChannelMessagesResponse {
        messages: Vec<(u64, HarmonyMessage)>,
        reached_top: bool,
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
        direction: Direction,
    },
    GetReplyMessage {
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
        message: HarmonyMessage,
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
    /// Sent whenever an error occurs.
    Error(Box<ClientError>),
    Exit,
    ExitReady,
    WindowFocusChanged(bool),
    FetchLinkDataReceived(FetchLinkData, Url),
}

impl Message {
    #[inline(always)]
    pub fn main(msg: main::Message) -> Self {
        Self::ChildMessage(ScreenMessage::MainScreen(msg).into())
    }

    #[inline(always)]
    pub fn login(msg: login::Message) -> Self {
        Self::ChildMessage(ScreenMessage::LoginScreen(msg).into())
    }

    #[inline(always)]
    pub fn guild_discovery(msg: guild_discovery::Message) -> Self {
        Self::ChildMessage(ScreenMessage::GuildDiscovery(msg).into())
    }

    #[inline(always)]
    pub fn guild_settings(msg: guild_settings::Message) -> Self {
        Self::ChildMessage(ScreenMessage::GuildSettings(msg).into())
    }

    #[inline(always)]
    pub fn emote_management(msg: emote_management::Message) -> Self {
        Self::ChildMessage(ScreenMessage::EmoteManagement(msg).into())
    }
}

#[derive(Debug, Clone)]
pub enum Screen {
    Login(Box<LoginScreen>),
    Main(Box<MainScreen>),
    GuildDiscovery(Box<GuildDiscovery>),
    GuildSettings(Box<GuildSettings>),
    EmoteManagement(Box<ManageEmotesScreen>),
}

impl Screen {
    #[inline(always)]
    fn on_error(&mut self, error: ClientError) -> Command<Message> {
        match self {
            Screen::Login(screen) => screen.on_error(error),
            Screen::GuildDiscovery(screen) => screen.on_error(error),
            Screen::Main(screen) => screen.on_error(error),
            Screen::GuildSettings(screen) => screen.on_error(error),
            Screen::EmoteManagement(screen) => screen.on_error(error),
        }
    }

    #[inline(always)]
    fn subscription(&self) -> Subscription<Message> {
        match self {
            Screen::Main(screen) => screen.subscription(),
            Screen::GuildSettings(screen) => screen.subscription(),
            Screen::GuildDiscovery(screen) => screen.subscription(),
            Screen::Login(screen) => screen.subscription(),
            Screen::EmoteManagement(screen) => screen.subscription(),
        }
    }

    #[inline(always)]
    fn view<'a>(
        &'a mut self,
        theme: &'a Theme,
        client: Option<&'a Client>,
        content_store: &'a Arc<ContentStore>,
        thumbnail_cache: &'a ThumbnailCache,
    ) -> Element<Message> {
        let element = match self {
            Screen::Login(screen) => screen.view(theme, content_store).map(ScreenMessage::LoginScreen),
            Screen::Main(screen) => screen
                .view(
                    theme,
                    client.unwrap(), // This will not panic cause [ref:client_set_before_main_view]
                    thumbnail_cache,
                )
                .map(ScreenMessage::MainScreen),
            Screen::GuildDiscovery(screen) => screen
                .view(theme, client.unwrap()) // This will not panic cause [ref:client_set_before_main_view]
                .map(ScreenMessage::GuildDiscovery),
            Screen::GuildSettings(screen) => screen
                .view(theme, client.unwrap(), thumbnail_cache) // This will not panic cause [ref:client_set_before_main_view]
                .map(ScreenMessage::GuildSettings),
            Screen::EmoteManagement(screen) => screen
                .view(theme, client.unwrap(), thumbnail_cache) // This will not panic cause [ref:client_set_before_main_view]
                .map(ScreenMessage::EmoteManagement),
        }
        .map(|msg| Message::ChildMessage(msg.into()));
        fill_container(element).style(theme.border_radius(0.0)).into()
    }

    #[inline(always)]
    fn update(
        &mut self,
        msg: ScreenMessage,
        client: Option<&mut Client>,
        content_store: &Arc<ContentStore>,
        thumbnail_cache: &ThumbnailCache,
        clip: &mut iced::Clipboard,
    ) -> Command<Message> {
        match msg {
            ScreenMessage::LoginScreen(msg) => {
                if let Screen::Login(screen) = self {
                    return screen.update(client.as_deref(), msg, content_store);
                }
            }
            ScreenMessage::MainScreen(msg) => {
                if let (Screen::Main(screen), Some(client)) = (self, client) {
                    return screen.update(msg, client, thumbnail_cache, clip);
                }
            }
            ScreenMessage::GuildDiscovery(msg) => {
                if let (Screen::GuildDiscovery(screen), Some(client)) = (self, client) {
                    return screen.update(msg, client);
                }
            }
            ScreenMessage::GuildSettings(msg) => {
                if let (Screen::GuildSettings(screen), Some(client)) = (self, client) {
                    return screen.update(msg, client, clip);
                }
            }
            ScreenMessage::EmoteManagement(msg) => {
                if let (Screen::EmoteManagement(screen), Some(client)) = (self, client) {
                    return screen.update(msg, client, clip);
                }
            }
        }
        Command::none()
    }

    fn push_screen_cmd(screen: Screen) -> Command<Message> {
        Command::perform(future::ready(Message::PushScreen(Box::new(screen))), identity)
    }

    fn pop_screen_cmd() -> Command<Message> {
        Command::perform(future::ready(Message::PopScreen), identity)
    }

    #[inline(always)]
    fn main_screen_mut(&mut self) -> Option<&mut MainScreen> {
        if let Screen::Main(screen) = self {
            Some(screen.as_mut())
        } else {
            None
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

    #[inline(always)]
    pub fn current(&self) -> &Screen {
        self.stack.last().unwrap() // this is safe cause of [ref:screenstack_cant_become_empty] [ref:screenstack_cant_start_empty]
    }

    #[inline(always)]
    pub fn current_mut(&mut self) -> &mut Screen {
        self.stack.last_mut().unwrap() // this is safe cause of [ref:screenstack_cant_become_empty] [ref:screenstack_cant_start_empty]
    }

    pub fn clear(&mut self, screen: Screen) {
        debug!("Clearing all screens in the stack and replacing it with {:?}", screen);

        self.stack.clear();
        self.stack.push(screen);
    }

    pub fn push(&mut self, screen: Screen) {
        debug!("Pushing a screen onto stack {:?}", screen);
        self.stack.push(screen)
    }

    pub fn pop(&mut self) -> Option<Screen> {
        // There must at least one screen remain to ensure [tag:screenstack_cant_become_empty]
        (self.stack.len() > 1).then(|| {
            let screen = self.stack.pop();
            debug!("Popping a screen {:?}", screen);
            screen.unwrap()
        })
    }

    pub fn find_map_mut<'a, B, F>(&'a mut self, f: F) -> Option<B>
    where
        F: FnMut(&'a mut Screen) -> Option<B>,
    {
        self.stack.iter_mut().find_map(f)
    }
}

pub struct ScreenManager {
    theme: Box<Theme>,
    screens: ScreenStack,
    client: Option<Box<Client>>,
    content_store: Arc<ContentStore>,
    thumbnail_cache: ThumbnailCache,
    cur_socket: Option<Box<EventsSocket>>,
    socket_reset: bool,
    should_exit: bool,
    is_window_focused: bool,
    theme_rx: Receiver<()>,
}

impl ScreenManager {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        let (ev_tx, ev_rx) = std::sync::mpsc::channel();

        let cstore = content_store.clone();
        std::thread::spawn(move || {
            use notify::{recommended_watcher, Event, RecursiveMode, Result, Watcher};

            let mut watcher = recommended_watcher(move |ev: Result<Event>| {
                if ev.is_ok() {
                    let _ = ev_tx.send(());
                }
            })
            .unwrap();

            watcher
                .watch(
                    cstore
                        .theme_file()
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(".")),
                    RecursiveMode::NonRecursive,
                )
                .unwrap();

            std::thread::park();
        });

        let mut this = Self {
            theme: Box::new(Theme::default()),
            screens: ScreenStack::new(Screen::Login(LoginScreen::new().into())),
            client: None,
            content_store,
            thumbnail_cache: ThumbnailCache::default(),
            cur_socket: None,
            socket_reset: false,
            should_exit: false,
            is_window_focused: true,
            theme_rx: ev_rx,
        };

        this.reload_user_theme();

        this
    }

    fn reload_user_theme(&mut self) {
        let user_theme = std::fs::read(self.content_store.theme_file())
            .ok()
            .and_then(|data| toml::from_slice::<ThemeRaw>(&data).ok())
            .map_or_else(Default::default, UserTheme::from);

        self.theme.user_theme = user_theme;
    }

    fn process_post_event(&mut self, post: PostProcessEvent, clip: &mut iced::Clipboard) -> Command<Message> {
        if let Some(client) = self.client.as_mut() {
            match post {
                PostProcessEvent::SendNotification { content, title, .. } => {
                    if !self.is_window_focused {
                        let _ = notify_rust::Notification::new()
                            .summary(&title)
                            .body(&truncate_string(&content, 50))
                            .auto_icon()
                            .show();
                    }
                    Command::none()
                }
                PostProcessEvent::CheckPermsForChannel(guild_id, channel_id) => client.mk_cmd(
                    |inner| async move {
                        let perm_queries = ["channels.manage.change-information", "messages.send"];
                        let mut events = Vec::with_capacity(perm_queries.len());
                        let batch_query = BatchSameRequest::new(
                            QueryHasPermissionRequest::ENDPOINT_PATH.to_string(),
                            IntoIter::new(perm_queries)
                                .map(|query| {
                                    let query = QueryHasPermissionRequest::new(
                                        guild_id,
                                        Some(channel_id),
                                        None,
                                        query.to_string(),
                                    );
                                    encode_protobuf_message(query).freeze()
                                })
                                .collect(),
                        );
                        events.extend(inner.call(batch_query).await.map(|resp| {
                            resp.responses
                                .into_iter()
                                .zip(IntoIter::new(perm_queries))
                                .filter_map(|(perm, query)| {
                                    let perm = <QueryHasPermissionRequest as Endpoint>::Response::decode(perm.as_ref())
                                        .ok()?;
                                    Some(Event::Chat(ChatEvent::PermissionUpdated(PermissionUpdated {
                                        guild_id,
                                        channel_id: Some(channel_id),
                                        ok: perm.ok,
                                        query: query.to_string(),
                                    })))
                                })
                        })?);
                        ClientResult::Ok(events)
                    },
                    Message::EventsReceived,
                ),
                PostProcessEvent::FetchThumbnail(id) => make_thumbnail_command(client, id, &mut self.thumbnail_cache),
                PostProcessEvent::FetchProfile(user_id) => client.mk_cmd(
                    |inner| async move {
                        inner.call(GetProfileRequest::new(user_id)).await.map(|resp| {
                            let profile = resp.profile.unwrap_or_default();
                            vec![Event::Profile(ProfileEvent::ProfileUpdated(ProfileUpdated {
                                user_id,
                                new_avatar: Some(profile.user_avatar),
                                new_status: Some(profile.user_status),
                                new_username: Some(profile.user_name),
                                new_is_bot: Some(profile.is_bot),
                            }))]
                        })
                    },
                    Message::EventsReceived,
                ),
                PostProcessEvent::GoToFirstMsgOnChannel(channel_id) => {
                    if let Some(s) = self.screens.find_map_mut(Screen::main_screen_mut) {
                        s.update(
                            main::Message::ScrollToBottom(channel_id),
                            client,
                            &self.thumbnail_cache,
                            clip,
                        )
                    } else {
                        Command::none()
                    }
                }
                PostProcessEvent::FetchGuildData(guild_id) => client.mk_cmd(
                    |inner| async move {
                        let mut events = Vec::with_capacity(10);
                        events.push(Ok(inner.call(GetGuildRequest::new(guild_id)).await.map(|resp| {
                            let guild = resp.guild.unwrap_or_default();
                            Event::Chat(ChatEvent::EditedGuild(GuildUpdated {
                                guild_id,
                                new_metadata: guild.metadata,
                                new_name: Some(guild.name),
                                new_picture: Some(guild.picture),
                            }))
                        })?));
                        let perm_queries = [
                            "guild.manage.change-information",
                            "user.manage.kick",
                            "user.manage.ban",
                            "user.manage.unban",
                            "invites.manage.create",
                            "invites.manage.delete",
                            "invites.view",
                            "channels.manage.move",
                            "channels.manage.create",
                            "channels.manage.delete",
                            "roles.manage",
                            "roles.get",
                            "roles.user.manage",
                            "roles.user.get",
                            "permissions.manage.set",
                            "permissions.manage.get",
                        ];
                        events.reserve(perm_queries.len());
                        let batch_query = BatchSameRequest::new(
                            QueryHasPermissionRequest::ENDPOINT_PATH.to_string(),
                            IntoIter::new(perm_queries)
                                .map(|query| {
                                    let query = QueryHasPermissionRequest::new(guild_id, None, None, query.to_string());
                                    encode_protobuf_message(query).freeze()
                                })
                                .collect(),
                        );
                        let batch_resp = inner.call(batch_query).await.map(|resp| {
                            resp.responses
                                .into_iter()
                                .zip(IntoIter::new(perm_queries))
                                .filter_map(|(perm, query)| {
                                    let perm = <QueryHasPermissionRequest as Endpoint>::Response::decode(perm.as_ref())
                                        .ok()?;
                                    Some(Ok(Event::Chat(ChatEvent::PermissionUpdated(PermissionUpdated {
                                        guild_id,
                                        channel_id: None,
                                        ok: perm.ok,
                                        query: query.to_string(),
                                    }))))
                                })
                        });
                        match batch_resp {
                            Ok(perms) => events.extend(perms),
                            Err(err) => events.push(Err(err.into())),
                        }
                        ClientResult::Ok(events)
                    },
                    Message::TryEventsReceived,
                ),
                PostProcessEvent::FetchMessage {
                    guild_id,
                    channel_id,
                    message_id,
                } => client.mk_cmd(
                    |inner| async move {
                        inner
                            .chat()
                            .await
                            .get_message(GetMessageRequest {
                                guild_id,
                                channel_id,
                                message_id,
                            })
                            .await
                            .map(|message| {
                                message
                                    .message
                                    .map_or(Message::Nothing, |message| Message::GetReplyMessage {
                                        guild_id,
                                        channel_id,
                                        message_id,
                                        message,
                                    })
                            })
                    },
                    identity,
                ),
                PostProcessEvent::FetchLinkMetadata(url) => client.mk_cmd(
                    |inner| async move {
                        inner
                            .mediaproxy()
                            .await
                            .fetch_link_metadata(FetchLinkMetadataRequest {
                                url: url.clone().into(),
                            })
                            .await
                            .map(|resp| (resp.data, url))
                    },
                    |(data, url)| data.map_or(Message::Nothing, |data| Message::FetchLinkDataReceived(data, url)),
                ),
                PostProcessEvent::FetchEmotes(pack_id) => client.mk_cmd(
                    |inner| async move {
                        inner.call(GetEmotePackEmotesRequest { pack_id }).await.map(|resp| {
                            vec![Event::Emote(EmoteEvent::EmotePackEmotesUpdated(
                                EmotePackEmotesUpdated {
                                    pack_id,
                                    added_emotes: resp.emotes,
                                    deleted_emotes: Vec::new(),
                                },
                            ))]
                        })
                    },
                    Message::EventsReceived,
                ),
            }
        } else {
            Command::none()
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
        let cmd = if content_store.latest_session_file().exists() {
            if let Screen::Login(screen) = manager.screens.current_mut() {
                screen.waiting = true;
            }
            Command::perform(
                async move {
                    let raw = tokio::fs::read(content_store.latest_session_file()).await?;
                    let session: Session = toml::de::from_slice(&raw).map_err(|_| ClientError::MissingLoginInfo)?;
                    let client = Client::new(
                        session.homeserver.parse().unwrap(),
                        Some(session.clone().into()),
                        content_store,
                    )
                    .await?;
                    client
                        .inner_arc()
                        .call(GetProfileRequest::new(client.user_id.unwrap()))
                        .await
                        .map(|resp| (client, resp))
                        .map_err(|err| {
                            let err = err.into();
                            try_convert_err_to_login_err(&err, &session).unwrap_or(err)
                        })
                },
                |result: ClientResult<_>| {
                    result
                        .map_to_msg_def(|(client, resp)| Message::LoginComplete(Box::new((Some(client), resp.profile))))
                },
            )
        } else {
            Command::none()
        };
        (manager, cmd)
    }

    fn title(&self) -> String {
        use std::fmt::Write;

        let mut title = String::from("Crust");
        if let (Screen::Main(screen), Some(client)) = (self.screens.current(), self.client.as_ref()) {
            if let Some(guild) = screen.current_guild_id.map(|id| client.guilds.get(&id)).flatten() {
                write!(&mut title, " | *{}", guild.name).unwrap();
                if let Some(channel) = screen.current_channel_id.map(|id| guild.channels.get(&id)).flatten() {
                    write!(&mut title, " | #{}", channel.name).unwrap();
                }
            }
        }
        title
    }

    fn update(&mut self, msg: Self::Message, clip: &mut iced::Clipboard) -> Command<Self::Message> {
        // TODO: move this to a subscription
        self.client.as_mut().and_do(|client| {
            client
                .members
                .values_mut()
                .for_each(|m| m.typing_in_channel = m.typing_in_channel.filter(|d| d.2.elapsed().as_secs() < 5))
        });

        if self.theme_rx.try_recv().is_ok() {
            self.reload_user_theme();
        }

        match msg {
            Message::ChildMessage(msg) => {
                return self.screens.current_mut().update(
                    *msg,
                    self.client.as_mut().map(Box::as_mut),
                    &self.content_store,
                    &self.thumbnail_cache,
                    clip,
                );
            }
            Message::Nothing => {}
            Message::Exit => {
                let sock = self.cur_socket.take();
                let inner = self.client.as_ref().map(|c| c.inner_arc());
                return Command::perform(
                    async move {
                        if let Some(sock) = sock {
                            let _ = sock.close().await;
                        }
                        if let Some(inner) = inner {
                            let _ = inner
                                .call(UpdateProfile::default().with_new_status(UserStatus::OfflineUnspecified))
                                .await;
                        }
                    },
                    |_| Message::ExitReady,
                );
            }
            Message::ExitReady => self.should_exit = true,
            Message::ClientCreated(client) => {
                let cmd = client.mk_cmd(
                    |inner| async move {
                        inner.begin_auth().await?;
                        inner.next_auth_step(AuthStepResponse::Initial).await
                    },
                    |step| Message::login(login::Message::AuthStep(step.map(|s| s.step).flatten())),
                );
                self.client = Some(client);
                return cmd;
            }
            Message::SocketEvent { mut socket, event } => {
                return if self.client.is_some() {
                    let mut cmds = Vec::with_capacity(2);

                    event
                        .and_do(|ev| {
                            debug!("event received from socket: {:?}", ev);
                            let cmd = match ev {
                                Ok(ev) => self.update(Message::EventsReceived(vec![ev]), clip),
                                Err(err) => self.update(err.into(), clip),
                            };
                            cmds.push(cmd);
                        })
                        .or_do(|| self.cur_socket = Some(socket.clone()));

                    self.socket_reset
                        .and_do(|| {
                            let client = self.client.as_ref().unwrap();
                            let sources = client.subscribe_to();
                            cmds.push(client.mk_cmd(
                                |inner| async move { inner.subscribe_events(sources).await },
                                |socket| Message::SocketEvent {
                                    socket: socket.into(),
                                    event: None,
                                },
                            ));
                            self.socket_reset = false;
                        })
                        .or_do(|| {
                            cmds.push(Command::perform(
                                async move {
                                    let event = socket.get_event().await.map_err(Into::into).transpose();
                                    Message::SocketEvent { socket, event }
                                },
                                identity,
                            ));
                        });

                    Command::batch(cmds)
                } else {
                    Command::perform(socket.close(), |_| Message::Nothing)
                };
            }
            Message::LoginComplete(res) => {
                let (maybe_client, maybe_profile) = *res;
                if let Some(client) = maybe_client {
                    self.client = Some(client.into()); // This is the only place we set a main screen [tag:client_set_before_main_view]
                }
                if let Screen::Login(screen) = self.screens.current_mut() {
                    screen.waiting = false;
                    screen.reset_to_first_step();
                }
                self.screens.push(Screen::Main(Box::new(MainScreen::default())));

                let client = self.client.as_mut().unwrap();
                let sources = client.subscribe_to();
                let ws_cmd = client.mk_cmd(
                    |inner| async move { inner.subscribe_events(sources).await },
                    |socket| Message::SocketEvent {
                        socket: socket.into(),
                        event: None,
                    },
                );
                client.user_id = Some(client.inner().auth_status().session().unwrap().user_id);
                let self_id = client.user_id.unwrap();
                let init = client.mk_cmd(
                    |inner| async move {
                        let self_profile = if let Some(profile) = maybe_profile {
                            profile
                        } else {
                            inner
                                .call(GetProfileRequest::new(self_id))
                                .await?
                                .profile
                                .unwrap_or_default()
                        };
                        let guilds = inner.chat().await.get_guild_list(GetGuildListRequest {}).await?.guilds;
                        let mut events = Vec::with_capacity(guilds.len() + 1);
                        events.extend(guilds.into_iter().map(|guild| {
                            Event::Chat(ChatEvent::GuildAddedToList(GuildAddedToList {
                                guild_id: guild.guild_id,
                                homeserver: guild.server_id,
                            }))
                        }));
                        events.push(Event::Profile(ProfileEvent::ProfileUpdated(ProfileUpdated {
                            new_is_bot: Some(self_profile.is_bot),
                            new_avatar: Some(self_profile.user_avatar),
                            new_status: Some(UserStatus::Online.into()),
                            new_username: Some(self_profile.user_name),
                            user_id: self_id,
                        })));
                        events.extend(inner.call(GetEmotePacksRequest {}).await.map(|resp| {
                            resp.packs.into_iter().map(|pack| {
                                Event::Emote(EmoteEvent::EmotePackAdded(EmotePackAdded { pack: Some(pack) }))
                            })
                        })?);
                        inner
                            .call(UpdateProfile::default().with_new_status(UserStatus::Online))
                            .await?;
                        ClientResult::Ok(events)
                    },
                    Message::EventsReceived,
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
            Message::MessageEdited {
                guild_id,
                channel_id,
                message_id,
                err,
            } => {
                self.client
                    .as_mut()
                    .and_then(|client| client.get_channel(guild_id, channel_id))
                    .and_then(|c| c.messages.get_mut(&MessageId::Ack(message_id)))
                    .and_do(|msg| msg.being_edited = None);

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
                let maybe_cmd = self
                    .client
                    .as_mut()
                    .and_then(|c| c.send_msg_cmd(guild_id, channel_id, retry_after, MessageId::default(), message));
                if let Some(cmd) = maybe_cmd {
                    return Command::perform(cmd, map_send_msg);
                }
            }
            Message::DownloadedThumbnail {
                data,
                thumbnail,
                avatar,
                emote,
                open,
            } => {
                let path = self.content_store.content_path(&data.id);
                emote.and_do(|emote| {
                    self.thumbnail_cache.put_emote_thumbnail(data.id.clone(), emote);
                });
                avatar.and_do(|(profile_avatar, avatar)| {
                    self.thumbnail_cache.put_avatar_thumbnail(data.id.clone(), avatar);
                    self.thumbnail_cache
                        .put_profile_avatar_thumbnail(data.id.clone(), profile_avatar);
                });
                thumbnail.and_do(|thumbnail| {
                    if let (Screen::Main(screen), true) = (self.screens.current_mut(), open) {
                        screen.image_viewer_modal.inner_mut().image_handle =
                            Some((thumbnail.clone(), (path, data.name)));
                        screen.image_viewer_modal.show(true);
                    }
                    self.thumbnail_cache.put_thumbnail(data.id, thumbnail);
                });
            }
            Message::EventsReceived(events) => {
                if self.client.is_some() {
                    let processed = {
                        let client = self.client.as_mut().unwrap();
                        events
                            .into_iter()
                            .flat_map(|event| client.process_event(event))
                            .collect::<Vec<_>>()
                    };

                    let mut cmds = Vec::with_capacity(processed.len() + 1);
                    cmds.push(Command::perform(ready(()), map_to_nothing));

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
                                ClientResult::Ok(())
                            },
                            ResultExt::map_to_nothing,
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

                    let client = self.client.as_ref().unwrap();
                    for chunk in fetch_users.chunks(64).map(|c| c.to_vec()) {
                        if !chunk.is_empty() {
                            let user_query = BatchSameRequest::new(
                                GetProfileRequest::ENDPOINT_PATH.to_string(),
                                chunk
                                    .iter()
                                    .map(|id| {
                                        let query = GetProfileRequest::new(*id);
                                        encode_protobuf_message(query).freeze()
                                    })
                                    .collect(),
                            );
                            let fetch_users_cmd = client.mk_cmd(
                                |inner| async move {
                                    inner.call(user_query).await.map(|batch_resp| {
                                        batch_resp
                                            .responses
                                            .into_iter()
                                            .zip(chunk.into_iter())
                                            .filter_map(|(resp, user_id)| {
                                                let profile =
                                                    <GetProfileRequest as Endpoint>::Response::decode(resp.as_ref())
                                                        .ok()?
                                                        .profile?;
                                                Some(Event::Profile(ProfileEvent::ProfileUpdated(ProfileUpdated {
                                                    user_id,
                                                    new_avatar: Some(profile.user_avatar),
                                                    new_status: Some(profile.user_status),
                                                    new_username: Some(profile.user_name),
                                                    new_is_bot: Some(profile.is_bot),
                                                })))
                                            })
                                            .collect()
                                    })
                                },
                                Message::EventsReceived,
                            );
                            cmds.push(fetch_users_cmd);
                        }
                    }

                    return Command::batch(cmds);
                }
            }
            Message::GetChannelMessagesResponse {
                messages,
                reached_top,
                guild_id,
                channel_id,
                message_id,
                direction,
            } => {
                return self
                    .client
                    .as_mut()
                    .map(|client| {
                        client
                            .get_channel(guild_id, channel_id)
                            .unwrap()
                            .loading_messages_history = false;
                        client.process_get_message_history_response(
                            guild_id,
                            channel_id,
                            message_id,
                            messages,
                            reached_top,
                            direction,
                        )
                    })
                    .map_or_else(Command::none, |posts| {
                        Command::batch(posts.into_iter().map(|post| self.process_post_event(post, clip)))
                    });
            }
            Message::Error(err) => {
                let err_disp = err.to_string();
                error!("{}\n{:?}", err_disp, err);

                // Reset socket if socket error happened
                matches!(
                    &*err,
                    ClientError::Internal(InnerClientError::Internal(HrpcClientError::SocketError(_)))
                )
                .and_do(|| self.socket_reset = true);

                // Return to login screen if its a connection error
                if err_disp.contains("invalid-session") || err_disp.contains("connect error") {
                    self.update(Message::Logout(Screen::Login(LoginScreen::new().into()).into()), clip);
                }

                return self.screens.current_mut().on_error(*err);
            }
            Message::WindowFocusChanged(focus) => self.is_window_focused = focus,
            Message::InitialGuildLoad { guild_id, events } => {
                if let Some(client) = self.client.as_mut() {
                    client.get_guild(guild_id).and_do(|g| g.init_fetching = false);
                }
                return match events {
                    Ok(events) => self.update(Message::TryEventsReceived(events), clip),
                    Err(err) => self.update(Message::Error(err.into()), clip),
                };
            }
            Message::InitialChannelLoad {
                guild_id,
                channel_id,
                events,
            } => {
                if let Some(client) = self.client.as_mut() {
                    client
                        .get_channel(guild_id, channel_id)
                        .and_do(|c| c.init_fetching = false);
                }
                return match events {
                    Ok(events) => self.update(*events, clip),
                    Err(err) => self.update(Message::Error(err.into()), clip),
                };
            }
            Message::TryEventsReceived(maybe_events) => {
                let mut cmds = Vec::with_capacity(maybe_events.len());
                let mut events = Vec::with_capacity(maybe_events.len());
                for maybe_event in maybe_events {
                    match maybe_event {
                        Ok(event) => events.push(event),
                        Err(err) => cmds.push(self.update(Message::Error(Box::new(err)), clip)),
                    }
                }
                cmds.push(self.update(Message::EventsReceived(events), clip));
                return Command::batch(cmds);
            }
            Message::FetchLinkDataReceived(data, url) => {
                let mut cmd = None;
                if let Some(client) = self.client.as_mut() {
                    if let FetchLinkData::IsSite(site) = &data {
                        if let Ok(url) = site.image.parse::<Url>() {
                            let id = FileId::External(url);
                            cmd = Some(client.mk_cmd(
                                |inner| async move {
                                    download_extract_file(&inner, id.clone()).await.map(|file| {
                                        Message::DownloadedThumbnail {
                                            open: false,
                                            data: Attachment {
                                                size: file.data().len() as u32,
                                                name: file.name().into(),
                                                kind: file.mimetype().into(),
                                                ..Attachment::new_unknown(id)
                                            },
                                            thumbnail: image::load_from_memory(file.data())
                                                .ok()
                                                .map(|image| image.into_bgra8())
                                                .map(|bgra| {
                                                    ImageHandle::from_pixels(
                                                        bgra.width(),
                                                        bgra.height(),
                                                        bgra.into_vec(),
                                                    )
                                                }),
                                            avatar: None,
                                            emote: None,
                                        }
                                    })
                                },
                                identity,
                            ));
                        }
                    }
                    client.link_datas.insert(url, data);
                }

                if let Some(cmd) = cmd {
                    return cmd;
                }
            }
            Message::GetReplyMessage {
                guild_id,
                channel_id,
                message_id,
                message,
            } => {
                if let Some(client) = self.client.as_mut() {
                    let posts = client.process_reply_message(guild_id, channel_id, message_id, message);
                    return Command::batch(posts.into_iter().map(|post| self.process_post_event(post, clip)));
                }
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        use iced_native::{window, Event};

        let sub = iced_native::subscription::events_with(|ev, _| {
            type We = window::Event;

            match ev {
                Event::Window(We::Unfocused) => Some(Message::WindowFocusChanged(false)),
                Event::Window(We::Focused) => Some(Message::WindowFocusChanged(true)),
                Event::Window(We::CloseRequested) => Some(Message::Exit),
                _ => None,
            }
        });

        Subscription::batch([self.screens.current().subscription(), sub])
    }

    fn view(&mut self) -> Element<Self::Message> {
        self.screens.current_mut().view(
            self.theme.as_ref(),
            self.client.as_ref().map(Box::as_ref),
            &self.content_store,
            &self.thumbnail_cache,
        )
    }

    fn should_exit(&self) -> bool {
        self.should_exit
    }
}

pub trait ClientExt {
    fn mk_cmd<T, Err, Fut, Cmd, Hndlr>(&self, cmd: Cmd, handler: Hndlr) -> Command<Message>
    where
        Err: Into<ClientError>,
        Fut: Future<Output = Result<T, Err>> + Send + 'static,
        Cmd: FnOnce(InnerClient) -> Fut,
        Hndlr: Fn(T) -> Message + Send + 'static;
}

impl ClientExt for Client {
    #[inline(always)]
    fn mk_cmd<T, Err, Fut, Cmd, Hndlr>(&self, cmd: Cmd, handler: Hndlr) -> Command<Message>
    where
        Err: Into<ClientError>,
        Fut: Future<Output = Result<T, Err>> + Send + 'static,
        Cmd: FnOnce(InnerClient) -> Fut,
        Hndlr: Fn(T) -> Message + Send + 'static,
    {
        let inner = self.inner_arc();
        Command::perform(cmd(inner), move |res| res.map_to_msg_def(|t| handler(t)))
    }
}

impl From<ClientError> for Message {
    #[inline(always)]
    fn from(err: ClientError) -> Self {
        Message::Error(Box::new(err))
    }
}

impl From<InnerClientError> for Message {
    #[inline(always)]
    fn from(err: InnerClientError) -> Self {
        Message::Error(Box::new(err.into()))
    }
}

pub fn map_to_nothing<T>(_: T) -> Message {
    Message::Nothing
}

pub trait ResultExt<T>: Sized {
    fn map_to_msg_def<F: FnOnce(T) -> Message>(self, f: F) -> Message;
    fn map_to_nothing(self) -> Message {
        self.map_to_msg_def(map_to_nothing)
    }
}

impl<T, Err: Into<ClientError>> ResultExt<T> for Result<T, Err> {
    #[inline(always)]
    fn map_to_msg_def<F: FnOnce(T) -> Message>(self, f: F) -> Message {
        self.map_or_else(|err| err.into().into(), f)
    }
}

fn map_send_msg(data: (u64, u64, u64, IcyMessage, Duration, Option<u64>)) -> Message {
    let (guild_id, channel_id, _, message, retry_after, res) = data;
    res.map_or(
        Message::SendMessage {
            guild_id,
            channel_id,
            message,
            retry_after,
        },
        |_| Message::Nothing,
    )
}

fn make_thumbnail_command(client: &Client, data: Attachment, thumbnail_cache: &mut ThumbnailCache) -> Command<Message> {
    const FILTER: FilterType = FilterType::Lanczos3;

    let is_thumbnailable = data.name == "avatar" || data.name == "guild";
    let is_emote = data.name == "emote";
    if let Some(m) = data.minithumbnail.as_ref() {
        if let Ok(image) = image::load_from_memory_with_format(&m.data, image::ImageFormat::Jpeg) {
            let (w, h) = data.resolution.unwrap_or((m.width, m.height));
            let (w, h) = scale_down(w, h, 400);
            let image = image.resize(w, h, FILTER);
            let image = image.blur(8.0);
            let bgra = image.into_bgra8();
            let image = ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec());
            thumbnail_cache.put_minithumbnail(data.id.clone(), image);
        }
    }
    thumbnail_cache
        .thumbnails
        .contains_key(&data.id)
        .not()
        .map_or_else(Command::none, || {
            let content_path = client.content_store().content_path(&data.id);

            let inner = client.inner_arc();
            let process_image = move |data: &[u8]| {
                let image = image::load_from_memory(data).ok();
                image
                    .map(|image| {
                        let avatar = is_thumbnailable.then(|| {
                            const RES_LEN: u32 = AVATAR_WIDTH as u32 - 4;
                            const PRES_LEN: u32 = PROFILE_AVATAR_WIDTH as u32;

                            let bgra = image.resize(RES_LEN, RES_LEN, FILTER).into_bgra8();
                            let avatar = ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec());

                            let bgra = image.resize(PRES_LEN, PRES_LEN, FILTER).into_bgra8();
                            let profile_avatar = ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec());

                            (profile_avatar, avatar)
                        });
                        let emote = is_emote.then(|| {
                            const RES_LEN: u32 = 48;

                            let bgra = image.resize(RES_LEN, RES_LEN, FILTER).into_bgra8();
                            ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec())
                        });
                        let content = is_thumbnailable.not().then(|| {
                            let bgra = image.into_bgra8();
                            ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec())
                        });
                        (content, avatar, emote)
                    })
                    .unwrap_or_default()
            };

            Command::perform(
                async move {
                    let (thumbnail, avatar, emote) = match tokio::fs::read(&content_path).await {
                        Ok(raw) => process_image(&raw),
                        Err(err) => {
                            warn!("couldn't read thumbnail for ID {} from disk: {}", data.id, err);
                            let file =
                                harmony_rust_sdk::client::api::rest::download_extract_file(&inner, data.id.clone())
                                    .await?;
                            tokio::fs::write(content_path, file.data()).await?;
                            process_image(file.data())
                        }
                    };
                    Ok(Message::DownloadedThumbnail {
                        data,
                        avatar,
                        thumbnail,
                        emote,
                        open: false,
                    })
                },
                |msg: ClientResult<_>| msg.unwrap_or_else(Into::into),
            )
        })
}

async fn select_files(one: bool) -> ClientResult<Vec<PathBuf>> {
    let file_dialog = rfd::AsyncFileDialog::new();

    if one {
        file_dialog.pick_file().await.map(|f| vec![f])
    } else {
        file_dialog.pick_files().await
    }
    .ok_or_else(|| ClientError::Custom("File selection error (no file selected?)".to_string()))
    .map(|files| files.into_iter().map(|a| a.path().to_path_buf()).collect())
}

async fn upload_files(
    inner: &InnerClient,
    content_store: Arc<ContentStore>,
    handles: Vec<PathBuf>,
) -> ClientResult<Vec<Attachment>> {
    use crate::client::content;
    use harmony_rust_sdk::client::api::rest::upload_extract_id;

    let mut ids = Vec::with_capacity(handles.len());

    for handle in handles {
        // TODO: don't load the files into memory
        // needs API for this in harmony_rust_sdk
        match tokio::fs::read(&handle).await {
            Ok(data) => {
                let file_mimetype = content::infer_type_from_bytes(&data);
                let filename = content::get_filename(&handle).to_string();
                let filesize = data.len();

                let send_result = upload_extract_id(inner, filename.clone(), file_mimetype.clone(), data).await;

                match send_result.map(FileId::Id) {
                    Ok(id) => {
                        let path = content_store.content_path(&id);
                        // Remove hard link if it exists
                        if path.exists() {
                            if let Err(err) = tokio::fs::remove_file(&path).await {
                                warn!("Couldn't remove file: {}", err);
                            }
                        }
                        // Hard link to file to save space
                        if let Err(err) = tokio::fs::hard_link(&handle, path).await {
                            warn!("An IO error occured while hard linking a file you tried to upload (this may result in a duplication of the file): {}", err);
                        }
                        ids.push(Attachment {
                            id,
                            kind: file_mimetype,
                            name: filename,
                            size: filesize as u32,
                            resolution: None,
                            minithumbnail: None,
                        });
                    }
                    Err(err) => {
                        error!("An error occured while trying to upload a file: {}", err);
                    }
                }
            }
            Err(err) => {
                error!("An IO error occured while trying to upload a file: {}", err);
            }
        }
    }

    Ok(ids)
}

async fn select_upload_files(
    inner: &InnerClient,
    content_store: Arc<ContentStore>,
    one: bool,
) -> ClientResult<Vec<Attachment>> {
    upload_files(inner, content_store, select_files(one).await?).await
}

fn try_convert_err_to_login_err(err: &ClientError, session: &Session) -> Option<ClientError> {
    let err_text = err.to_string();
    if err_text.contains("invalid-session") {
        Some(ClientError::Custom(format!(
            "This session ({} with ID {} on homeserver {}) is invalid, please login again!",
            session.user_name, session.user_id, session.homeserver
        )))
    } else {
        None
    }
}

pub fn truncate_string(value: &str, new_len: usize) -> Cow<'_, str> {
    if value.chars().count() > new_len {
        let mut value = value.to_string();
        value.truncate(value.chars().take(new_len).map(char::len_utf8).sum());
        value.push('…');
        Cow::Owned(value)
    } else {
        Cow::Borrowed(value)
    }
}

pub fn sub_escape_pop_screen() -> Subscription<Message> {
    iced_native::subscription::events_with(|ev, _| {
        use iced_native::{
            event::Event,
            keyboard::{Event as Ke, KeyCode},
        };

        match ev {
            Event::Keyboard(Ke::KeyPressed {
                key_code: KeyCode::Escape,
                ..
            }) => Some(Message::PopScreen),
            _ => None,
        }
    })
}

// scale down resolution while preserving ratio
pub fn scale_down(w: u32, h: u32, max_size: u32) -> (u32, u32) {
    let ratio = w / h;
    let new_w = max_size;
    let new_h = max_size / ratio;
    (new_w, new_h)
}
