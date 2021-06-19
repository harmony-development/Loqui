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
        content::ContentStore,
        error::{ClientError, ClientResult},
        message::{Attachment, Message as IcyMessage, MessageId},
        Client, PostProcessEvent, Session,
    },
    component::*,
    style::Theme,
};

use client::{
    bool_ext::BoolExt,
    harmony_rust_sdk::{
        self,
        api::{
            chat::{
                event::{Event, GuildAddedToList, GuildUpdated, ProfileUpdated},
                GetGuildListRequest, GetUserResponse, QueryPermissionsResponse,
            },
            harmonytypes::UserStatus,
            rest::FileId,
        },
        client::{
            api::{
                auth::AuthStepResponse,
                chat::{
                    guild::{get_guild, get_guild_list},
                    permissions::{self, QueryPermissions, QueryPermissionsSelfBuilder},
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
};
use iced::{executor, Application, Command, Element, Subscription};
use std::{convert::identity, future::Future, sync::Arc, time::Duration};

#[derive(Debug)]
pub enum Message {
    LoginScreen(login::Message),
    MainScreen(main::Message),
    GuildDiscovery(guild_discovery::Message),
    GuildSettings(guild_settings::Message),
    PopScreen,
    PushScreen(Box<Screen>),
    Logout(Box<Screen>),
    LoginComplete(Option<Client>, Option<GetUserResponse>),
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
        Command::perform(async move { Message::PushScreen(Box::new(screen)) }, identity)
    }

    fn pop_screen_cmd() -> Command<Message> {
        Command::perform(async { Message::PopScreen }, identity)
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
        debug!("Clearing all screens in the stack and replacing it with {:?}", screen);

        let mut temp = Vec::with_capacity(self.stack.len());
        temp.append(&mut self.stack);

        self.stack.push(screen);

        temp
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
            screens: ScreenStack::new(Screen::Login(LoginScreen::new())),
            client: None,
            content_store,
            thumbnail_cache: ThumbnailCache::default(),
            cur_socket: None,
            socket_reset: false,
            should_exit: false,
        }
    }

    fn process_post_event(&mut self, post: PostProcessEvent, clip: &mut iced::Clipboard) -> Command<Message> {
        if let Some(client) = self.client.as_mut() {
            match post {
                PostProcessEvent::CheckPermsForChannel(guild_id, channel_id) => {
                    return client.mk_cmd(
                        |inner| async move {
                            let query = |check_for: &str| {
                                permissions::query_has_permission(
                                    &inner,
                                    QueryPermissions::new(guild_id, check_for.to_string()).channel_id(channel_id),
                                )
                            };
                            let manage_channel = query("channels.manage.change-information").await?.ok;
                            let send_msg = query("messages.send").await?.ok;
                            ClientResult::Ok(Message::UpdateChanPerms(
                                ChanPerms {
                                    send_msg,
                                    manage_channel,
                                },
                                guild_id,
                                channel_id,
                            ))
                        },
                        identity,
                    );
                }
                PostProcessEvent::FetchThumbnail(id) => {
                    return make_thumbnail_command(client, id, &self.thumbnail_cache);
                }
                PostProcessEvent::FetchProfile(user_id) => {
                    return client.mk_cmd(
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
                    return client.mk_cmd(
                        |inner| async move {
                            get_guild(&inner, GuildId::new(guild_id)).await.map(|guild_data| {
                                vec![Event::EditedGuild(GuildUpdated {
                                    guild_id,
                                    metadata: guild_data.metadata,
                                    name: guild_data.guild_name,
                                    picture: guild_data.guild_picture,
                                    update_name: true,
                                    update_picture: true,
                                    update_metadata: true,
                                })]
                            })
                        },
                        Message::EventsReceived,
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
        let cmd = if content_store.latest_session_file().exists() {
            if let Screen::Login(screen) = manager.screens.current_mut() {
                screen.waiting = true;
            }
            Command::perform(
                async move {
                    let raw = tokio::fs::read(content_store.latest_session_file()).await?;
                    let session: Session = toml::de::from_slice(&raw).map_err(|_| ClientError::MissingLoginInfo)?;
                    let client =
                        Client::new(session.homeserver.parse().unwrap(), Some(session.into()), content_store).await?;
                    let user_profile = get_user(client.inner(), UserId::new(client.user_id.unwrap())).await?;
                    Ok((client, user_profile))
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
            Message::LoginScreen(msg) => {
                if let Screen::Login(screen) = self.screens.current_mut() {
                    return screen.update(self.client.as_ref(), msg, &self.content_store);
                }
            }
            Message::MainScreen(msg) => {
                if let (Screen::Main(screen), Some(client)) = (self.screens.current_mut(), &mut self.client) {
                    return screen.update(msg, client, &self.thumbnail_cache, clip);
                }
            }
            Message::GuildDiscovery(msg) => {
                if let (Screen::GuildDiscovery(screen), Some(client)) = (self.screens.current_mut(), &self.client) {
                    return screen.update(msg, client);
                }
            }
            Message::GuildSettings(msg) => {
                if let (Screen::GuildSettings(screen), Some(client)) = (self.screens.current_mut(), &self.client) {
                    return screen.update(msg, client);
                }
            }
            Message::ClientCreated(client) => {
                self.client = Some(client);
                return self.client.as_ref().unwrap().mk_cmd(
                    |inner| async move {
                        inner.begin_auth().await?;
                        inner.next_auth_step(AuthStepResponse::Initial).await
                    },
                    |step| Message::LoginScreen(login::Message::AuthStep(step)),
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
                        debug!("event received from socket: {:?}", ev);
                        let cmd = match ev {
                            Ok(ev) => self.update(Message::EventsReceived(vec![ev]), clip),
                            Err(err) => self.update(err.into(), clip),
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
                            identity,
                        ));
                    } else {
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
                    }

                    return Command::batch(cmds);
                } else {
                    return Command::perform(
                        async move {
                            let _ = socket.close().await;
                        },
                        |_| Message::Nothing,
                    );
                }
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
                if let Some(msg) = self
                    .client
                    .as_mut()
                    .map(|client| client.get_channel(guild_id, channel_id))
                    .flatten()
                    .map(|channel| channel.messages.get_mut(&MessageId::Unack(transaction_id)))
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
                    .map(|c| c.messages.get_mut(&MessageId::Ack(message_id)))
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
                    return Command::perform(cmd, map_send_msg);
                }
            }
            Message::DownloadedThumbnail { data, thumbnail, open } => {
                let path = self.content_store.content_path(&data.id);
                self.thumbnail_cache.put_thumbnail(data.id, thumbnail.clone());
                if let (Screen::Main(screen), true) = (self.screens.current_mut(), open) {
                    screen.image_viewer_modal.inner_mut().image_handle = Some((thumbnail, (path, data.name)));
                    screen.image_viewer_modal.show(true);
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

                    for chunk in fetch_users.chunks(64).map(|c| c.to_vec()) {
                        if !chunk.is_empty() {
                            let fetch_users_cmd = self.client.as_ref().unwrap().mk_cmd(
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
                let posts = self.client.as_mut().map_or_else(Vec::new, |client| {
                    client
                        .get_channel(guild_id, channel_id)
                        .unwrap()
                        .loading_messages_history = false;
                    client.process_get_message_history_response(guild_id, channel_id, messages, reached_top)
                });
                let cmds = posts.into_iter().map(|post| self.process_post_event(post, clip));
                return Command::batch(cmds);
            }
            Message::Error(err) => {
                let err_disp = err.to_string();
                error!("{}\n{:?}", err_disp, err);

                matches!(
                    &*err,
                    ClientError::Internal(InnerClientError::Internal(HrpcClientError::SocketError(_)))
                )
                .and_do(|| self.socket_reset = true);

                if err_disp.contains("invalid-session") || err_disp.contains("connect error") {
                    self.update(Message::Logout(Screen::Login(LoginScreen::new()).into()), clip);
                }

                return self.screens.current_mut().on_error(*err);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        self.screens.current().subscription()
    }

    fn view(&mut self) -> Element<Self::Message> {
        match self.screens.current_mut() {
            Screen::Login(screen) => screen.view(self.theme, &self.content_store).map(Message::LoginScreen),
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

pub trait ClientExt {
    fn mk_cmd<T, Err, Fut, Cmd, Hndlr>(&self, cmd: Cmd, handler: Hndlr) -> Command<Message>
    where
        Err: Into<ClientError>,
        Fut: Future<Output = Result<T, Err>> + Send + 'static,
        Cmd: FnOnce(InnerClient) -> Fut,
        Hndlr: Fn(T) -> Message + Send + 'static;
}

impl ClientExt for Client {
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
    fn from(err: ClientError) -> Self {
        Message::Error(Box::new(err))
    }
}

impl From<InnerClientError> for Message {
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
    fn map_to_msg_def<F: FnOnce(T) -> Message>(self, f: F) -> Message {
        self.map_or_else(|err| err.into().into(), f)
    }
}

fn map_send_msg(data: (u64, u64, u64, IcyMessage, Duration, Option<u64>)) -> Message {
    let (guild_id, channel_id, transaction_id, message, retry_after, res) = data;
    match res {
        Some(message_id) => Message::MessageSent {
            guild_id,
            channel_id,
            message_id,
            transaction_id,
        },
        None => Message::SendMessage {
            guild_id,
            channel_id,
            message,
            retry_after,
        },
    }
}

fn make_query_perm(
    client: &Client,
    guild_id: u64,
    channel_id: u64,
    check_for: &str,
    f: impl FnOnce(QueryPermissionsResponse, u64, u64) -> Message + Send + 'static,
) -> Command<Message> {
    let query = permissions::QueryPermissions::new(guild_id, check_for.to_string()).channel_id(channel_id);
    client.mk_cmd(
        |inner| async move {
            permissions::query_has_permission(&inner, query)
                .await
                .map(|p| f(p, guild_id, channel_id))
        },
        identity,
    )
}

fn make_thumbnail_command(client: &Client, data: Attachment, thumbnail_cache: &ThumbnailCache) -> Command<Message> {
    if !thumbnail_cache.has_thumbnail(&data.id) {
        let content_path = client.content_store().content_path(&data.id);

        let inner = client.inner_arc();

        Command::perform(
            async move {
                match tokio::fs::read(&content_path).await {
                    Ok(raw) => {
                        let bgra = image::load_from_memory(&raw).unwrap().into_bgra8();
                        Ok(Message::DownloadedThumbnail {
                            data,
                            thumbnail: ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec()),
                            open: false,
                        })
                    }
                    Err(err) => {
                        warn!("couldn't read thumbnail for ID {} from disk: {}", data.id, err);
                        let file =
                            harmony_rust_sdk::client::api::rest::download_extract_file(&inner, data.id.clone()).await?;
                        tokio::fs::write(content_path, file.data()).await?;
                        let bgra = image::load_from_memory(file.data()).unwrap().into_bgra8();
                        Ok(Message::DownloadedThumbnail {
                            data,
                            thumbnail: ImageHandle::from_pixels(bgra.width(), bgra.height(), bgra.into_vec()),
                            open: false,
                        })
                    }
                }
            },
            |msg: ClientResult<_>| msg.unwrap_or_else(Into::into),
        )
    } else {
        Command::none()
    }
}

async fn select_upload_files(
    inner: &InnerClient,
    content_store: Arc<ContentStore>,
    one: bool,
) -> ClientResult<Vec<(FileId, String, String, usize)>> {
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
                        ids.push((id, file_mimetype, filename, filesize));
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
