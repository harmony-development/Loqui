pub mod login;
pub mod logout;
pub mod main;
pub mod room_discovery;

pub use login::LoginScreen;
pub use logout::Logout as LogoutScreen;
pub use main::MainScreen;
pub use room_discovery::RoomDiscovery as RoomDiscoveryScreen;

use crate::{
    client::{
        content::{ContentStore, ImageHandle, ThumbnailCache},
        error::ClientError,
        ActionRetry, AuthMethod, Client, InnerClient, TimelineEvent,
    },
    ui::style::Theme,
};
use iced::{executor, Application, Command, Element, Subscription};
use iced_futures::BoxStream;
use ruma::{
    api::{
        client::r0::{message::get_message_events, sync::sync_events},
        exports::http::Uri,
    },
    events::{room::message::MessageEventContent, AnySyncRoomEvent},
    presence::PresenceState,
    RoomId,
};
use std::{hash::Hash, hash::Hasher, sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Debug)]
pub enum Message {
    LoginScreen(login::Message),
    LogoutScreen(logout::Message),
    MainScreen(main::Message),
    RoomDiscoveryScreen(room_discovery::Message),
    PopScreen,
    PushScreen(Box<Screen>),
    /// Sent when the "login" is complete, ie. establishing a session and performing an initial sync.
    LoginComplete(Client),
    /// Do nothing.
    Nothing,
    DownloadedThumbnail {
        thumbnail_url: Uri,
        thumbnail: ImageHandle,
    },
    /// Sent when a sync response is received from the server.
    SyncResponse(Box<sync_events::Response>),
    /// Sent when a "get context" (get events around an event) is received from the server.
    GetEventsBackwardsResponse(Box<get_message_events::Response>),
    SendMessage {
        content: Vec<MessageEventContent>,
        room_id: RoomId,
    },
    SendMessageResult(RetrySendEventResult),
    /// Sent whenever an error occurs.
    MatrixError(Box<ClientError>),
}

#[derive(Debug)]
pub enum Screen {
    Logout(LogoutScreen),
    Login(LoginScreen),
    Main(MainScreen),
    RoomDiscovery(RoomDiscoveryScreen),
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
    pub fn new(content_store: ContentStore) -> Self {
        let content_store = Arc::new(content_store);

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
        if content_store.session_file().exists() {
            let mut manager = ScreenManager::new(content_store);
            let cmd = manager.update(Message::LoginScreen(login::Message::AuthWith(
                AuthMethod::RestoringSession,
            )));
            (manager, cmd)
        } else {
            (ScreenManager::new(content_store), Command::none())
        }
    }

    fn title(&self) -> String {
        String::from("Icy Matrix")
    }

    fn update(&mut self, msg: Self::Message) -> Command<Self::Message> {
        match msg {
            Message::Nothing => {}
            Message::LoginScreen(msg) => {
                if let Screen::Login(screen) = self.screens.current_mut() {
                    return screen.update(msg);
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
            Message::RoomDiscoveryScreen(msg) => {
                if let (Screen::RoomDiscovery(screen), Some(client)) =
                    (self.screens.current_mut(), &self.client)
                {
                    return screen.update(msg, client);
                }
            }
            Message::LoginComplete(client) => {
                let inner = client.inner();

                self.client = Some(client); // This is the only place we set a main screen [tag:client_set_before_main_view]
                self.screens.push(Screen::Main(MainScreen::default()));

                // TODO: Display "syncing with homeserver" when initial sync is running
                // TODO: Errors also need to be shown on the main screen as well (a "toast" would do probably)
                return Command::perform(Client::initial_sync(inner), |result| match result {
                    Ok(response) => Message::SyncResponse(Box::new(response)),
                    // TODO: If initial sync errors out, we should go back to the login screen probably (?)
                    Err(e) => Message::MatrixError(Box::new(e)),
                });
            }
            Message::PopScreen => {
                self.screens.pop();
            }
            Message::PushScreen(screen) => {
                self.screens.push(*screen);
            }
            Message::SendMessage { content, room_id } => {
                if let Some(client) = self.client.as_mut() {
                    if let Some(room) = client.rooms.get_mut(&room_id) {
                        for content in content {
                            room.add_event(TimelineEvent::new_unacked_message(
                                content,
                                Uuid::new_v4(),
                            ));
                        }
                    }
                }
            }
            Message::SendMessageResult(errors) => {
                if let Some(client) = self.client.as_mut() {
                    use ruma::{
                        api::client::error::ErrorKind as ClientAPIErrorKind, api::error::*,
                    };
                    use ruma_client::Error as InnerClientError;

                    for (room_id, errors) in errors {
                        for (transaction_id, error) in errors {
                            if let ClientError::Internal(InnerClientError::FromHttpResponse(
                                FromHttpResponseError::Http(ServerError::Known(err)),
                            )) = error
                            {
                                if let ClientAPIErrorKind::LimitExceeded { retry_after_ms } =
                                    err.kind
                                {
                                    if let Some(retry_after) = retry_after_ms {
                                        if let Some(room) = client.rooms.get_mut(&room_id) {
                                            room.wait_for_duration(retry_after, transaction_id);
                                        }
                                        log::error!(
                                            "Send message after: {}",
                                            retry_after.as_secs()
                                        );
                                    }
                                }
                            } else {
                                log::error!("Error while sending message: {}", error);
                            }
                        }
                    }
                }
            }
            Message::DownloadedThumbnail {
                thumbnail_url,
                thumbnail,
            } => {
                self.thumbnail_cache.put_thumbnail(thumbnail_url, thumbnail);
            }
            Message::SyncResponse(response) => {
                if let Some(client) = self.client.as_mut() {
                    let thumbnail_urls = client.process_sync_response(*response);
                    return make_thumbnail_commands(client, thumbnail_urls, &self.thumbnail_cache);
                }
            }
            Message::GetEventsBackwardsResponse(response) => {
                if let Some(client) = self.client.as_mut() {
                    if let Some((room_id, thumbnail_urls)) =
                        client.process_events_backwards_response(*response)
                    {
                        // Safe unwrap
                        client
                            .rooms
                            .get_mut(&room_id)
                            .unwrap()
                            .loading_events_backward = false;
                        return make_thumbnail_commands(
                            client,
                            thumbnail_urls,
                            &self.thumbnail_cache,
                        );
                    }
                }
            }
            Message::MatrixError(err) => {
                use ruma::{api::client::error::ErrorKind as ClientAPIErrorKind, api::error::*};
                use ruma_client::Error as InnerClientError;

                log::error!("{}", err);

                if let ClientError::Internal(err) = &*err {
                    if let InnerClientError::FromHttpResponse(err) = err {
                        if let FromHttpResponseError::Http(err) = err {
                            if let ServerError::Known(err) = err {
                                // Return to login screen since the users session has expired.
                                if let ClientAPIErrorKind::UnknownToken { soft_logout: _ } =
                                    err.kind
                                {
                                    self.screens.clear(Screen::Login(LoginScreen::new(
                                        self.content_store.clone(),
                                    )));
                                }
                            }
                        }
                    }
                }

                return self.screens.current_mut().on_error(*err);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut sub = Subscription::none();

        if let Some(client) = &self.client {
            let rooms_queued_events = client.rooms_queued_events();
            sub = Subscription::from_recipe(RetrySendEventRecipe {
                inner: client.inner(),
                rooms_queued_events,
            })
            .map(Message::SendMessageResult);

            if let Some(since) = client.next_batch() {
                sub = Subscription::batch(vec![
                    sub,
                    Subscription::from_recipe(SyncRecipe {
                        inner: client.inner(),
                        since: since.to_string(),
                    })
                    .map(|result| match result {
                        Ok(response) => Message::SyncResponse(Box::from(response)),
                        Err(err) => Message::MatrixError(Box::new(err)),
                    }),
                ]);
            }
        }

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
            Screen::RoomDiscovery(screen) => screen
                .view(self.theme, self.client.as_ref().unwrap()) // This will not panic cause [ref:client_set_before_main_view]
                .map(Message::RoomDiscoveryScreen),
        }
    }
}

pub type RetrySendEventResult = Vec<(RoomId, Vec<(Uuid, ClientError)>)>;
pub struct RetrySendEventRecipe {
    inner: InnerClient,
    rooms_queued_events: Vec<(RoomId, Vec<ActionRetry>)>,
}

impl<H, I> iced_futures::subscription::Recipe<H, I> for RetrySendEventRecipe
where
    H: Hasher,
{
    type Output = RetrySendEventResult;

    fn hash(&self, state: &mut H) {
        std::any::TypeId::of::<Self>().hash(state);

        for (id, events) in &self.rooms_queued_events {
            id.hash(state);
            for (transaction_id, _, retry_after) in events {
                transaction_id.hash(state);
                retry_after.hash(state);
            }
        }

        self.inner.session().hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<I>) -> BoxStream<Self::Output> {
        let future = async move {
            let mut room_errors = Vec::new();

            for (room_id, events) in self.rooms_queued_events {
                let mut transaction_errors = Vec::new();
                for (transaction_id, event, retry_after) in events {
                    if let Some(dur) = retry_after {
                        tokio::time::sleep(dur).await;
                    }

                    let result = match event {
                        AnySyncRoomEvent::Message(ev) => {
                            Client::send_message(
                                self.inner.clone(),
                                ev.content(),
                                room_id.clone(),
                                transaction_id,
                            )
                            .await
                        }
                        _ => unimplemented!(),
                    };

                    if let Err(e) = result {
                        transaction_errors.push((transaction_id, e));
                    }
                }
                room_errors.push((room_id, transaction_errors));
            }

            room_errors
        };

        Box::pin(iced_futures::futures::stream::once(future))
    }
}

pub type SyncResult = Result<sync_events::Response, ClientError>;
pub struct SyncRecipe {
    inner: InnerClient,
    since: String,
}

impl<H, I> iced_futures::subscription::Recipe<H, I> for SyncRecipe
where
    H: Hasher,
{
    type Output = SyncResult;

    fn hash(&self, state: &mut H) {
        std::any::TypeId::of::<Self>().hash(state);

        self.since.hash(state);
        self.inner.session().hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<I>) -> BoxStream<Self::Output> {
        use iced_futures::futures::TryStreamExt;

        Box::pin(
            self.inner
                .sync(
                    None,
                    self.since,
                    &PresenceState::Online,
                    Some(Duration::from_secs(20)),
                )
                .map_err(ClientError::Internal),
        )
    }
}

fn make_thumbnail_commands(
    client: &Client,
    thumbnail_urls: Vec<(bool, Uri)>,
    thumbnail_cache: &ThumbnailCache,
) -> Command<Message> {
    Command::batch(
        thumbnail_urls
            .into_iter()
            .flat_map(|(is_on_disk, thumbnail_url)| {
                if !thumbnail_cache.has_thumbnail(&thumbnail_url) {
                    let content_path = client.content_store().content_path(&thumbnail_url);

                    Some(if is_on_disk {
                        Command::perform(
                            async move {
                                (
                                    async {
                                        Ok(ImageHandle::from_memory(
                                            tokio::fs::read(content_path).await?,
                                        ))
                                    }
                                    .await,
                                    thumbnail_url,
                                )
                            },
                            |(result, thumbnail_url)| match result {
                                Ok(thumbnail) => Message::DownloadedThumbnail {
                                    thumbnail,
                                    thumbnail_url,
                                },
                                Err(err) => Message::MatrixError(Box::new(err)),
                            },
                        )
                    } else {
                        let download_task =
                            Client::download_content(client.inner(), thumbnail_url.clone());

                        Command::perform(
                            async move {
                                match download_task.await {
                                    Ok(raw_data) => {
                                        tokio::fs::write(content_path, raw_data.as_slice())
                                            .await
                                            .map(|_| (thumbnail_url, raw_data))
                                            .map_err(Into::into)
                                    }
                                    Err(err) => Err(err),
                                }
                            },
                            |result| match result {
                                Ok((thumbnail_url, raw_data)) => Message::DownloadedThumbnail {
                                    thumbnail_url,
                                    thumbnail: ImageHandle::from_memory(raw_data),
                                },
                                Err(err) => Message::MatrixError(Box::new(err)),
                            },
                        )
                    })
                } else {
                    None
                }
            }),
    )
}
