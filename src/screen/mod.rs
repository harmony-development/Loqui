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
    style::{Theme, AVATAR_WIDTH, PROFILE_AVATAR_WIDTH},
};

use client::{
    bool_ext::BoolExt,
    harmony_rust_sdk::{
        self,
        api::{
            chat::{
                event::{Event, GuildAddedToList, GuildUpdated, MessageSent, PermissionUpdated, ProfileUpdated},
                GetGuildListRequest, GetMessageRequest, GetUserResponse,
            },
            harmonytypes::UserStatus,
            rest::FileId,
        },
        client::{
            api::{
                auth::AuthStepResponse,
                chat::{
                    guild::{get_guild, get_guild_list},
                    message::get_message,
                    permissions::{self, query_has_permission, QueryPermissions, QueryPermissionsSelfBuilder},
                    profile::{self, get_user, get_user_bulk, ProfileUpdate},
                    EventSource, GuildId, UserId,
                },
                harmonytypes::Message as HarmonyMessage,
            },
            error::{ClientError as InnerClientError, InternalClientError as HrpcClientError},
            Client as InnerClient, EventsSocket,
        },
    },
    tracing::{debug, error, warn},
    OptionExt,
};
use iced::{
    executor,
    futures::future::{self, ready},
    Application, Command, Element, Subscription,
};
use std::{borrow::Cow, convert::identity, future::Future, ops::Not, sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub enum ScreenMessage {
    LoginScreen(login::Message),
    MainScreen(main::Message),
    GuildDiscovery(guild_discovery::Message),
    GuildSettings(guild_settings::Message),
}

#[derive(Debug, Clone)]
pub enum Message {
    ChildMessage(ScreenMessage),
    PopScreen,
    PushScreen(Box<Screen>),
    Logout(Box<Screen>),
    LoginComplete(Option<Client>, Option<GetUserResponse>),
    ClientCreated(Client),
    Nothing,
    DownloadedThumbnail {
        data: Attachment,
        thumbnail: Option<ImageHandle>,
        avatar: Option<(ImageHandle, ImageHandle)>,
        open: bool,
    },
    EventsReceived(Vec<Event>),
    InitialLoad {
        guild_id: u64,
        channel_id: Option<u64>,
        events: Result<Vec<Event>, Box<ClientError>>,
    },
    SocketEvent {
        socket: Box<EventsSocket>,
        event: Option<Result<Event, ClientError>>,
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
    /// Sent whenever an error occurs.
    Error(Box<ClientError>),
    Exit,
    ExitReady,
    WindowFocusChanged(bool),
}

impl Message {
    #[inline(always)]
    pub const fn main(msg: main::Message) -> Self {
        Self::ChildMessage(ScreenMessage::MainScreen(msg))
    }

    #[inline(always)]
    pub const fn login(msg: login::Message) -> Self {
        Self::ChildMessage(ScreenMessage::LoginScreen(msg))
    }

    #[inline(always)]
    pub const fn guild_discovery(msg: guild_discovery::Message) -> Self {
        Self::ChildMessage(ScreenMessage::GuildDiscovery(msg))
    }

    #[inline(always)]
    pub const fn guild_settings(msg: guild_settings::Message) -> Self {
        Self::ChildMessage(ScreenMessage::GuildSettings(msg))
    }
}

#[derive(Debug, Clone)]
pub enum Screen {
    Login(Box<LoginScreen>),
    Main(Box<MainScreen>),
    GuildDiscovery(Box<GuildDiscovery>),
    GuildSettings(Box<GuildSettings>),
}

impl Screen {
    #[inline(always)]
    fn on_error(&mut self, error: ClientError) -> Command<Message> {
        match self {
            Screen::Login(screen) => screen.on_error(error),
            Screen::GuildDiscovery(screen) => screen.on_error(error),
            Screen::Main(screen) => screen.on_error(error),
            Screen::GuildSettings(screen) => screen.on_error(error),
        }
    }

    #[inline(always)]
    fn subscription(&self) -> Subscription<Message> {
        match self {
            Screen::Main(screen) => screen.subscription(),
            _ => Subscription::none(),
        }
    }

    #[inline(always)]
    fn view<'a>(
        &'a mut self,
        theme: Theme,
        client: Option<&'a Client>,
        content_store: &'a Arc<ContentStore>,
        thumbnail_cache: &'a ThumbnailCache,
    ) -> Element<Message> {
        match self {
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
        }
        .map(Message::ChildMessage)
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
    theme: Theme,
    screens: ScreenStack,
    client: Option<Client>,
    content_store: Arc<ContentStore>,
    thumbnail_cache: ThumbnailCache,
    cur_socket: Option<Box<EventsSocket>>,
    socket_reset: bool,
    should_exit: bool,
    is_window_focused: bool,
}

impl ScreenManager {
    pub fn new(content_store: Arc<ContentStore>) -> Self {
        Self {
            theme: Theme::default(),
            screens: ScreenStack::new(Screen::Login(LoginScreen::new().into())),
            client: None,
            content_store,
            thumbnail_cache: ThumbnailCache::default(),
            cur_socket: None,
            socket_reset: false,
            should_exit: false,
            is_window_focused: true,
        }
    }

    fn process_post_event(&mut self, post: PostProcessEvent, clip: &mut iced::Clipboard) -> Command<Message> {
        if let Some(client) = self.client.as_mut() {
            match post {
                PostProcessEvent::SendNotification { content, .. } => {
                    if !self.is_window_focused {
                        // TODO: send notif
                    }
                    Command::none()
                }
                PostProcessEvent::CheckPermsForChannel(guild_id, channel_id) => client.mk_cmd(
                    |inner| async move {
                        let query = |check_for: &str| {
                            permissions::query_has_permission(
                                &inner,
                                QueryPermissions::new(guild_id, check_for.to_string()).channel_id(channel_id),
                            )
                        };
                        let manage_channel = query("channels.manage.change-information").await?.ok;
                        let send_msg = query("messages.send").await?.ok;
                        ClientResult::Ok(vec![
                            Event::PermissionUpdated(PermissionUpdated {
                                guild_id,
                                channel_id,
                                ok: manage_channel,
                                query: "channels.manage.change-information".to_string(),
                            }),
                            Event::PermissionUpdated(PermissionUpdated {
                                guild_id,
                                channel_id,
                                ok: send_msg,
                                query: "messages.send".to_string(),
                            }),
                        ])
                    },
                    Message::EventsReceived,
                ),
                PostProcessEvent::FetchThumbnail(id) => make_thumbnail_command(client, id, &self.thumbnail_cache),
                PostProcessEvent::FetchProfile(user_id) => client.mk_cmd(
                    |inner| async move {
                        get_user(&inner, UserId::new(user_id)).await.map(|profile| {
                            vec![Event::ProfileUpdated(ProfileUpdated {
                                user_id,
                                new_avatar: profile.user_avatar,
                                new_status: profile.user_status,
                                new_username: profile.user_name,
                                is_bot: profile.is_bot,
                                update_is_bot: true,
                                update_status: true,
                                update_avatar: true,
                                update_username: true,
                            })]
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
                        let ev = get_guild(&inner, GuildId::new(guild_id)).await.map(|guild_data| {
                            Event::EditedGuild(GuildUpdated {
                                guild_id,
                                metadata: guild_data.metadata,
                                name: guild_data.guild_name,
                                picture: guild_data.guild_picture,
                                update_name: true,
                                update_picture: true,
                                update_metadata: true,
                            })
                        })?;
                        let query = "guild.manage.change-information".to_string();
                        let perm = query_has_permission(&inner, QueryPermissions::new(guild_id, query.clone()))
                            .await
                            .map(|perm| {
                                Event::PermissionUpdated(PermissionUpdated {
                                    guild_id,
                                    channel_id: 0,
                                    ok: perm.ok,
                                    query,
                                })
                            })?;
                        ClientResult::Ok(vec![ev, perm])
                    },
                    Message::EventsReceived,
                ),
                PostProcessEvent::FetchMessage {
                    guild_id,
                    channel_id,
                    message_id,
                } => client.mk_cmd(
                    |inner| async move {
                        get_message(
                            &inner,
                            GetMessageRequest {
                                guild_id,
                                channel_id,
                                message_id,
                            },
                        )
                        .await
                        .map(|message| {
                            vec![Event::SentMessage(
                                (MessageSent {
                                    echo_id: 0,
                                    message: message.message,
                                })
                                .into(),
                            )]
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
                    get_user(client.inner(), UserId::new(client.user_id.unwrap()))
                        .await
                        .map(|user_profile| (client, user_profile))
                        .map_err(|err| {
                            let err = err.into();
                            try_convert_err_to_login_err(&err, &session).unwrap_or(err)
                        })
                },
                |result: ClientResult<_>| {
                    result.map_to_msg_def(|(client, profile)| Message::LoginComplete(Some(client), Some(profile)))
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

        match msg {
            Message::ChildMessage(msg) => {
                return self.screens.current_mut().update(
                    msg,
                    self.client.as_mut(),
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
                            let _ = profile::profile_update(
                                &inner,
                                ProfileUpdate::default().new_status(UserStatus::Offline),
                            )
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
                    |step| Message::ChildMessage(ScreenMessage::LoginScreen(login::Message::AuthStep(step))),
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
                                    let ev;
                                    loop {
                                        if let Some(event) = socket.get_event().await {
                                            ev = event;
                                            break;
                                        }
                                    }
                                    Message::SocketEvent {
                                        socket,
                                        event: Some(ev.map_err(Into::into)),
                                    }
                                },
                                identity,
                            ));
                        });

                    Command::batch(cmds)
                } else {
                    Command::perform(socket.close(), |_| Message::Nothing)
                };
            }
            Message::LoginComplete(maybe_client, maybe_profile) => {
                if let Some(client) = maybe_client {
                    self.client = Some(client); // This is the only place we set a main screen [tag:client_set_before_main_view]
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
                            get_user(&inner, UserId::new(self_id)).await?
                        };
                        let guilds = get_guild_list(&inner, GetGuildListRequest {}).await?.guilds;
                        let mut events = Vec::with_capacity(guilds.len() + 1);
                        events.extend(guilds.into_iter().map(|guild| {
                            Event::GuildAddedToList(GuildAddedToList {
                                guild_id: guild.guild_id,
                                homeserver: guild.host,
                            })
                        }));
                        events.push(Event::ProfileUpdated(ProfileUpdated {
                            update_avatar: true,
                            update_is_bot: true,
                            update_status: true,
                            update_username: true,
                            is_bot: self_profile.is_bot,
                            new_avatar: self_profile.user_avatar,
                            new_status: UserStatus::OnlineUnspecified.into(),
                            new_username: self_profile.user_name,
                            user_id: self_id,
                        }));
                        profile::profile_update(
                            &inner,
                            ProfileUpdate::default().new_status(UserStatus::OnlineUnspecified),
                        )
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
            Message::MessageSent {
                message_id,
                transaction_id,
                guild_id,
                channel_id,
            } => {
                self.client
                    .as_mut()
                    .and_then(|client| client.get_channel(guild_id, channel_id))
                    .and_then(|channel| channel.messages.get_mut(&MessageId::Unack(transaction_id)))
                    .and_do(|msg| msg.id = MessageId::Ack(message_id));
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
                    .and_then(|c| c.send_msg_cmd(guild_id, channel_id, retry_after, message));
                if let Some(cmd) = maybe_cmd {
                    return Command::perform(cmd, map_send_msg);
                }
            }
            Message::DownloadedThumbnail {
                data,
                thumbnail,
                avatar,
                open,
            } => {
                let path = self.content_store.content_path(&data.id);
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
                            let fetch_users_cmd = client.mk_cmd(
                                |inner| async move {
                                    get_user_bulk(&inner, chunk.clone()).await.map(|profiles| {
                                        profiles
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
            Message::GetEventsBackwardsResponse {
                messages,
                reached_top,
                guild_id,
                channel_id,
            } => {
                return self
                    .client
                    .as_mut()
                    .map(|client| {
                        client
                            .get_channel(guild_id, channel_id)
                            .unwrap()
                            .loading_messages_history = false;
                        client.process_get_message_history_response(guild_id, channel_id, messages, reached_top)
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
            Message::InitialLoad {
                guild_id,
                channel_id,
                events,
            } => {
                if let Some(client) = self.client.as_mut() {
                    channel_id
                        .and_do(|channel_id| {
                            client
                                .get_channel(guild_id, channel_id)
                                .and_do(|c| c.init_fetching = false);
                        })
                        .or_do(|| {
                            client.get_guild(guild_id).and_do(|g| g.init_fetching = false);
                        });
                }
                return match events {
                    Ok(events) => self.update(Message::EventsReceived(events), clip),
                    Err(err) => self.update(Message::Error(err), clip),
                };
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
            self.theme,
            self.client.as_ref(),
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
    let (guild_id, channel_id, transaction_id, message, retry_after, res) = data;
    res.map_or(
        Message::SendMessage {
            guild_id,
            channel_id,
            message,
            retry_after,
        },
        |message_id| Message::MessageSent {
            guild_id,
            channel_id,
            message_id,
            transaction_id,
        },
    )
}

fn make_thumbnail_command(client: &Client, data: Attachment, thumbnail_cache: &ThumbnailCache) -> Command<Message> {
    let is_thumbnailable = data.name == "avatar" || data.name == "guild";
    thumbnail_cache
        .thumbnails
        .contains_key(&data.id)
        .not()
        .map_or_else(Command::none, || {
            let content_path = client.content_store().content_path(&data.id);

            let inner = client.inner_arc();
            let process_image = move |data: &[u8]| {
                let image = image::load_from_memory(data).unwrap();
                let avatar = is_thumbnailable.then(|| {
                    const RES_LEN: u32 = AVATAR_WIDTH as u32 - 4;
                    const FILTER: FilterType = FilterType::Lanczos3;
                    const PRES_LEN: u32 = PROFILE_AVATAR_WIDTH as u32;

                    let bgra = image.resize(RES_LEN, RES_LEN, FILTER).into_bgra8();
                    let avatar = ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec());

                    let bgra = image.resize(PRES_LEN, PRES_LEN, FILTER).into_bgra8();
                    let profile_avatar = ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec());

                    (profile_avatar, avatar)
                });
                let content = is_thumbnailable.not().then(|| {
                    let bgra = image.into_bgra8();
                    ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec())
                });
                (content, avatar)
            };

            Command::perform(
                async move {
                    let (thumbnail, avatar) = match tokio::fs::read(&content_path).await {
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
                        open: false,
                    })
                },
                |msg: ClientResult<_>| msg.unwrap_or_else(Into::into),
            )
        })
}

async fn select_upload_files(
    inner: &InnerClient,
    content_store: Arc<ContentStore>,
    one: bool,
) -> ClientResult<Vec<Attachment>> {
    use crate::client::content;
    use harmony_rust_sdk::client::api::rest::upload_extract_id;

    let file_dialog = rfd::AsyncFileDialog::new();

    let handles = if one {
        file_dialog.pick_file().await.map(|f| vec![f])
    } else {
        file_dialog.pick_files().await
    }
    .ok_or_else(|| ClientError::Custom("File selection error (no file selected?)".to_string()))?;
    let mut ids = Vec::with_capacity(handles.len());

    for handle in handles {
        // TODO: don't load the files into memory
        // needs API for this in harmony_rust_sdk
        match tokio::fs::read(handle.path()).await {
            Ok(data) => {
                let file_mimetype = content::infer_type_from_bytes(&data);
                let filename = content::get_filename(handle.path()).to_string();
                let filesize = data.len();

                let send_result = upload_extract_id(inner, filename.clone(), file_mimetype.clone(), data).await;

                match send_result.map(FileId::Id) {
                    Ok(id) => {
                        if let Err(err) = tokio::fs::hard_link(handle.path(), content_store.content_path(&id)).await {
                            warn!("An IO error occured while hard linking a file you tried to upload (this may result in a duplication of the file): {}", err);
                        }
                        ids.push(Attachment {
                            id,
                            kind: file_mimetype,
                            name: filename,
                            size: filesize as u32,
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
