use assign::assign;
pub use room::{Room, Rooms};
use ruma::{
    api::client::r0::media::get_content,
    api::{
        client::r0::{
            context::get_context,
            filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter, RoomFilter},
            message::send_message_event,
            session::logout,
            sync::sync_events,
            typing::create_typing_event,
        },
        exports::{
            http::{self, Uri},
            serde::{Deserialize, Serialize},
        },
    },
    events::{
        room::{aliases::AliasesEventContent, canonical_alias::CanonicalAliasEventContent},
        typing::TypingEventContent,
        AnyEphemeralRoomEventContent, AnyMessageEventContent, AnyRoomEvent, AnySyncStateEvent,
        SyncStateEvent,
    },
    presence::PresenceState,
    DeviceId, EventId, Raw, RoomId, UserId,
};
pub use ruma_client::{
    Client as InnerClient, Identification as InnerIdentification, Session as InnerSession,
};
use std::{
    convert::TryFrom,
    convert::TryInto,
    fmt::{self, Debug, Display, Formatter},
    time::Duration,
};
pub use timeline_event::TimelineEvent;
use uuid::Uuid;

use self::media::make_content_path;

pub mod media;
pub mod room;
pub mod timeline_event;

#[macro_export]
macro_rules! data_dir {
    () => {
        "data/"
    };
}
pub const SESSION_ID_PATH: &str = concat!(data_dir!(), "session");

#[cfg(target_os = "linux")]
pub const CLIENT_ID: &str = "icy_matrix Linux";
#[cfg(target_os = "windows")]
pub const CLIENT_ID: &str = "icy_matrix Windows";
#[cfg(target_os = "macos")]
pub const CLIENT_ID: &str = "icy_matrix MacOS";

/// A sesssion struct with our requirements (unlike the `InnerSession` type)
#[derive(Clone, Deserialize, Serialize)]
pub struct Session {
    pub access_token: String,
    pub user_id: UserId,
    pub device_id: Box<DeviceId>,
}

impl Debug for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user_id.to_string())
            .field("device_id", &self.device_id.to_string())
            .finish()
    }
}

impl Display for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Session for user {} on device {}",
            self.user_id, self.device_id
        )
    }
}

impl Into<InnerSession> for Session {
    fn into(self) -> InnerSession {
        InnerSession {
            identification: Some(InnerIdentification {
                user_id: self.user_id,
                device_id: self.device_id,
            }),
            access_token: self.access_token,
        }
    }
}

impl TryFrom<InnerSession> for Session {
    type Error = ClientError;

    fn try_from(value: InnerSession) -> Result<Self, Self::Error> {
        let (access_token, user_id, device_id) = if let Some(id) = value.identification {
            (value.access_token, id.user_id, id.device_id)
        } else {
            return Err(ClientError::MissingLoginInfo);
        };

        Ok(Self {
            access_token,
            user_id,
            device_id,
        })
    }
}

pub struct Client {
    /* The inner client stores the session (with our requirements,
    since we only allow `Client` creation when they are met),
    so we don't need to store it here again. */
    inner: InnerClient,
    rooms: Rooms,
    next_batch: Option<String>,
}

impl Debug for Client {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Client")
            .field("user_id", &self.current_user_id().to_string())
            .finish()
    }
}

impl Client {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, ClientError> {
        tokio::fs::create_dir_all(format!("{}content", data_dir!())).await?;

        let homeserver_url = homeserver
            .parse::<Uri>()
            .map_err(|e| ClientError::URLParse(homeserver.to_owned(), e))?;

        let inner = InnerClient::new(homeserver_url, None);

        let mut device_id = None;
        if let Ok(s) = tokio::fs::read_to_string(SESSION_ID_PATH).await {
            if let Ok(session) = toml::from_str::<Session>(&s) {
                device_id = Some(session.device_id);
            }
        }

        let session = {
            Session::try_from(
                inner
                    .log_in(username, password, device_id.as_deref(), Some(CLIENT_ID))
                    .await?,
            )?
        };

        // Save the session
        if let Ok(encoded_session) = toml::to_vec(&session) {
            // Do not abort the sync if we can't save the session data
            if let Err(err) = tokio::fs::write(SESSION_ID_PATH, encoded_session).await {
                log::error!("Could not save session data: {}", err);
            } else {
                use std::os::unix::fs::PermissionsExt;

                if let Err(err) = tokio::fs::set_permissions(
                    SESSION_ID_PATH,
                    std::fs::Permissions::from_mode(0o600),
                )
                .await
                {
                    log::error!("Could not set permissions of session file: {}", err);
                }
            }
        }

        Ok(Self {
            inner,
            rooms: Rooms::new(),
            next_batch: None,
        })
    }

    pub fn new_with_session(session: Session) -> Result<Self, ClientError> {
        let homeserver = format!("https://{}", session.user_id.server_name());
        let homeserver_url = homeserver
            .parse::<Uri>()
            .map_err(|e| ClientError::URLParse(homeserver, e))?;

        let inner = InnerClient::new(homeserver_url, Some(session.into()));

        Ok(Self {
            inner,
            rooms: Rooms::new(),
            next_batch: None,
        })
    }

    pub async fn logout(inner: InnerClient) -> Result<(), ClientError> {
        inner.request(logout::Request::new()).await?;

        tokio::fs::remove_file(SESSION_ID_PATH).await?;

        Ok(())
    }

    pub fn current_user_id(&self) -> UserId {
        self.inner
            .session()
            // This unwrap is safe since we check if there is a session beforehand
            .unwrap()
            .identification
            // This unwrap is safe since we check if there is a user_id beforehand
            .unwrap()
            .user_id
    }

    pub fn next_batch(&self) -> Option<String> {
        self.next_batch.clone()
    }

    pub async fn initial_sync(&mut self) -> Result<(), ClientError> {
        let lazy_load_filter = Client::member_lazy_load_sync_filter();

        let initial_sync_response = self
            .inner
            .request(assign!(sync_events::Request::new(), {
                filter: Some(
                    // Lazy load room members here to ensure a fast login
                    // FIXME: Some members do not load properly after this
                    &lazy_load_filter
                ),
                since: self.next_batch.as_deref(),
                full_state: false,
                set_presence: &PresenceState::Online,
                timeout: None,
            }))
            .await?;

        self.process_sync_response(initial_sync_response);

        Ok(())
    }

    fn member_lazy_load_sync_filter<'a>() -> sync_events::Filter<'a> {
        sync_events::Filter::FilterDefinition(assign!(FilterDefinition::default(), {
            room: assign!(RoomFilter::default(), {
                state: Client::member_lazy_load_room_event_filter()
            }),
        }))
    }

    fn member_lazy_load_room_event_filter<'a>() -> RoomEventFilter<'a> {
        assign!(RoomEventFilter::default(), {
            lazy_load_options: LazyLoadOptions::Enabled {
                    include_redundant_members: false,
                }
            }
        )
    }

    pub fn inner(&self) -> InnerClient {
        self.inner.clone()
    }

    pub fn rooms(&self) -> &Rooms {
        &self.rooms
    }

    /// Removes a room from the stored rooms.
    pub fn remove_room(&mut self, room_id: &RoomId) {
        self.rooms.remove(room_id);
    }

    pub fn get_room(&self, room_id: &RoomId) -> Option<&Room> {
        self.rooms.get(room_id)
    }

    pub fn get_room_mut(&mut self, room_id: &RoomId) -> Option<&mut Room> {
        self.rooms.get_mut(room_id)
    }

    pub fn get_room_mut_or_create(&mut self, room_id: RoomId) -> &mut Room {
        self.rooms.entry(room_id).or_insert_with(Room::new)
    }

    pub async fn download_content(
        inner: InnerClient,
        content_url: Uri,
    ) -> Result<Vec<u8>, ClientError> {
        Ok(inner
            .request(get_content::Request::new(
                content_url.path().trim_matches('/'),
                content_url
                    .authority()
                    .unwrap()
                    .as_str()
                    .try_into()
                    .unwrap(),
            ))
            .await?)
        .map(|response| response.file)
    }

    pub async fn send_typing(
        inner: InnerClient,
        room_id: RoomId,
        current_user_id: UserId,
    ) -> Result<create_typing_event::Response, ClientError> {
        let response = inner
            .request(create_typing_event::Request::new(
                &current_user_id,
                &room_id,
                create_typing_event::Typing::Yes(Duration::from_secs(1)),
            ))
            .await?;

        Ok(response)
    }

    pub async fn send_message(
        inner: InnerClient,
        content: AnyMessageEventContent,
        room_id: RoomId,
        txn_id: Uuid,
    ) -> Result<send_message_event::Response, ClientError> {
        inner
            .request(send_message_event::Request::new(
                &room_id,
                txn_id.to_string().as_str(),
                &content,
            ))
            .await
            .map_err(ClientError::Internal)
    }

    pub async fn get_events_around(
        inner: InnerClient,
        room_id: RoomId,
        event_id: EventId,
    ) -> Result<get_context::Response, ClientError> {
        let rooms = [room_id];
        inner
            .request(assign!(get_context::Request::new(&rooms[0], &event_id), {
                // We lazy load members here since they will be incrementally sent by the sync response after initial sync
                filter: Some(assign!(Client::member_lazy_load_room_event_filter(), {
                    rooms: Some(&rooms),
                })),
            }))
            .await
            .map_err(ClientError::Internal)
    }

    pub fn process_events_around_response(
        &mut self,
        response: get_context::Response,
    ) -> (Vec<Uri>, Vec<Uri>) {
        let mut download_urls = vec![];
        let mut read_urls = vec![];

        let get_context::Response {
            events_before,
            event: maybe_raw_event,
            events_after,
            ..
        } = response;

        if let Some(raw_event) = maybe_raw_event {
            if let Ok(event) = raw_event.deserialize() {
                fn convert_room_to_sync_room_with_id(event: AnyRoomEvent) -> (RoomId, EventId) {
                    match event {
                        AnyRoomEvent::Message(ev) => (ev.room_id().clone(), ev.event_id().clone()),
                        AnyRoomEvent::State(ev) => (ev.room_id().clone(), ev.event_id().clone()),
                        AnyRoomEvent::RedactedMessage(ev) => {
                            (ev.room_id().clone(), ev.event_id().clone())
                        }
                        AnyRoomEvent::RedactedState(ev) => {
                            (ev.room_id().clone(), ev.event_id().clone())
                        }
                    }
                }

                fn convert_to_timeline_event(
                    raw_events: Vec<Raw<AnyRoomEvent>>,
                ) -> Vec<TimelineEvent> {
                    raw_events
                        .into_iter()
                        .flat_map(|r| r.deserialize())
                        .map(|e| e.into())
                        .collect()
                }

                let (room_id, event_id) = convert_room_to_sync_room_with_id(event);

                if let Some(room) = self.get_room_mut(&room_id) {
                    let events_before = convert_to_timeline_event(events_before);
                    let events_after = convert_to_timeline_event(events_after);
                    for ev in events_after.iter().chain(events_before.iter()) {
                        if let Some(content_url) = ev.thumbnail_url() {
                            if make_content_path(&content_url).exists() {
                                read_urls.push(content_url)
                            } else {
                                download_urls.push(content_url);
                            }
                        }
                    }
                    room.add_chunk_of_events(events_before, events_after, &event_id);
                }
            }
        }
        (download_urls, read_urls)
    }

    pub fn process_sync_response(
        &mut self,
        response: sync_events::Response,
    ) -> (Vec<Uri>, Vec<Uri>) {
        let mut download_urls = vec![];
        let mut read_urls = vec![];

        for (room_id, joined_room) in response.rooms.join {
            let room = self.get_room_mut_or_create(room_id);
            for event in joined_room
                .ephemeral
                .events
                .iter()
                .flat_map(|r| r.deserialize())
            {
                if let AnyEphemeralRoomEventContent::Typing(TypingEventContent { user_ids }) =
                    event.content()
                {
                    room.update_typing(user_ids.as_slice());
                }
            }
            for event in joined_room
                .state
                .events
                .iter()
                .flat_map(|r| r.deserialize())
            {
                match event {
                    AnySyncStateEvent::RoomAliases(SyncStateEvent {
                        content: AliasesEventContent { aliases, .. },
                        ..
                    }) => {
                        room.set_alt_aliases(aliases);
                    }
                    AnySyncStateEvent::RoomName(SyncStateEvent { content, .. }) => {
                        room.set_name(content.name().map(|s| s.to_string()));
                    }
                    AnySyncStateEvent::RoomCanonicalAlias(SyncStateEvent {
                        content:
                            CanonicalAliasEventContent {
                                alias, alt_aliases, ..
                            },
                        ..
                    }) => {
                        room.set_canonical_alias(alias);
                        room.set_alt_aliases(alt_aliases);
                    }
                    // TODO: Make UI to show users
                    AnySyncStateEvent::RoomMember(member_state) => {
                        let membership_change = member_state.membership_change();
                        room.update_member(
                            member_state.prev_content.map(|c| c.displayname),
                            member_state.content.displayname,
                            membership_change,
                            member_state.sender,
                        );
                    }
                    _ => {}
                }
            }
            for event in joined_room
                .timeline
                .events
                .iter()
                .flat_map(|r| r.deserialize())
            {
                let tevent = TimelineEvent::new(event);
                room.ack_event(&tevent);
                room.redact_event(&tevent);
                if let Some(content_url) = tevent.thumbnail_url() {
                    if make_content_path(&content_url).exists() {
                        read_urls.push(content_url)
                    } else {
                        download_urls.push(content_url);
                    }
                }
                room.add_event(tevent);
            }
        }
        for (room_id, _) in response.rooms.leave {
            self.remove_room(&room_id);
        }

        self.next_batch = Some(response.next_batch);
        (download_urls, read_urls)
    }
}

#[derive(Debug)]
pub enum ClientError {
    /// Error occurred during an IO operation.
    IOError(std::io::Error),
    /// Error occurred while parsing a string as URL.
    URLParse(String, http::uri::InvalidUri),
    /// Error occurred in the Matrix client library.
    Internal(ruma_client::Error<ruma::api::client::Error>),
    /// The user is already logged in.
    AlreadyLoggedIn,
    /// Not all required login information was provided.
    MissingLoginInfo,
}

impl From<ruma_client::Error<ruma::api::client::Error>> for ClientError {
    fn from(other: ruma_client::Error<ruma::api::client::Error>) -> Self {
        Self::Internal(other)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(other: std::io::Error) -> Self {
        Self::IOError(other)
    }
}

impl Display for ClientError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use ruma::{api::client::error::ErrorKind as ClientAPIErrorKind, api::error::*};
        use ruma_client::Error as InnerClientError;

        match self {
            ClientError::URLParse(string, err) => {
                write!(fmt, "Could not parse URL '{}': {}", string, err)
            }
            ClientError::Internal(err) => {
                match err {
                    InnerClientError::FromHttpResponse(FromHttpResponseError::Http(
                        ServerError::Known(err),
                    )) => match err.kind {
                        ClientAPIErrorKind::Forbidden => {
                            return write!(
                                fmt,
                                "The server rejected your login information: {}",
                                err.message
                            );
                        }
                        ClientAPIErrorKind::Unauthorized => {
                            return write!(
                                fmt,
                                "You are unauthorized to perform an operation: {}",
                                err.message
                            );
                        }
                        ClientAPIErrorKind::UnknownToken { soft_logout: _ } => {
                            return write!(
                                fmt,
                                "Your session has expired, please login again: {}",
                                err.message
                            );
                        }
                        _ => {}
                    },
                    InnerClientError::Response(_) => {
                        return write!(
                            fmt,
                            "Please check if you can connect to the internet and try again: {}",
                            err,
                        );
                    }
                    InnerClientError::AuthenticationRequired => {
                        return write!(
                            fmt,
                            "Authentication is required for an operation, please login (again)",
                        );
                    }
                    _ => {}
                }
                write!(fmt, "An internal error occurred: {}", err.to_string())
            }
            ClientError::IOError(err) => write!(fmt, "An IO error occurred: {}", err),
            ClientError::AlreadyLoggedIn => write!(fmt, "Already logged in with another user."),
            ClientError::MissingLoginInfo => {
                write!(fmt, "Missing required login information, can't login.")
            }
        }
    }
}
