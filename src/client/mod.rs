#![allow(clippy::field_reassign_with_default)]

pub mod content;
pub mod error;
pub mod member;
pub mod room;
pub mod timeline_event;

pub use room::{Room, Rooms};
pub use ruma_client::{
    Client as InnerClient, Identification as InnerIdentification, Session as InnerSession,
};
pub use timeline_event::TimelineEvent;

use assign::assign;
use content::ContentStore;
use error::{ClientError, ClientResult};
use ruma::{
    api::client::r0::media::{create_content, get_content},
    api::{
        client::r0::{
            context::get_context,
            filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter, RoomFilter},
            membership::join_room_by_id_or_alias,
            message::{get_message_events, send_message_event},
            session::logout,
            sync::sync_events,
            typing::create_typing_event,
        },
        exports::{
            http::Uri,
            serde::{Deserialize, Serialize},
        },
    },
    events::{
        room::{
            aliases::AliasesEventContent, avatar::AvatarEventContent,
            canonical_alias::CanonicalAliasEventContent,
        },
        typing::TypingEventContent,
        AnyEphemeralRoomEventContent, AnyMessageEventContent, AnyRoomEvent, AnySyncRoomEvent,
        AnySyncStateEvent, SyncStateEvent,
    },
    presence::PresenceState,
    DeviceId, EventId, RoomId, UserId,
};
use std::{
    convert::TryFrom,
    convert::TryInto,
    fmt::{self, Debug, Display, Formatter},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

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
        let (access_token, user_id, device_id) = match value.identification {
            Some(id) => (value.access_token, id.user_id, id.device_id),
            None => return Err(ClientError::MissingLoginInfo),
        };

        Ok(Self {
            access_token,
            user_id,
            device_id,
        })
    }
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    LoginOrRegister { info: AuthInfo, register: bool },
    Guest { homeserver_domain: String },
    RestoringSession,
}

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub homeserver_domain: String,
    pub username: String,
    pub password: String,
}

impl Default for AuthInfo {
    fn default() -> Self {
        Self {
            homeserver_domain: String::from("matrix.org"),
            username: String::default(),
            password: String::default(),
        }
    }
}

pub type ActionRetry = (Uuid, AnySyncRoomEvent, Option<Duration>);

pub struct Client {
    /* The inner client stores the session (with our requirements,
    since we only allow `Client` creation when they are met),
    so we don't need to store it here again. */
    inner: InnerClient,
    pub rooms: Rooms,
    next_batch: Option<String>,
    content_store: Arc<ContentStore>,
}

impl Debug for Client {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Client")
            .field("user_id", &format!("{:?}", self.current_user_id()))
            .field("session_file", &self.content_store.session_file())
            .finish()
    }
}

impl Client {
    pub async fn new(
        auth_method: AuthMethod,
        content_store: Arc<ContentStore>,
    ) -> ClientResult<Self> {
        fn get_homeserver(homeserver_domain: &str) -> ClientResult<Uri> {
            let homeserver = format!("https://{}", homeserver_domain);
            homeserver
                .parse::<Uri>()
                .map_err(|e| ClientError::URLParse(homeserver, e))
        }

        let (session, inner) = {
            match auth_method {
                AuthMethod::Guest { homeserver_domain } => {
                    let inner = InnerClient::new(get_homeserver(&homeserver_domain)?, None);

                    (inner.register_guest().await?.try_into()?, inner)
                }
                AuthMethod::LoginOrRegister { info, register } => {
                    let AuthInfo {
                        homeserver_domain,
                        username,
                        password,
                    } = info;
                    let inner = InnerClient::new(get_homeserver(&homeserver_domain)?, None);

                    let session = if register {
                        inner.register_user(Some(&username), &password).await?
                    } else {
                        inner
                            .log_in(&username, &password, None, Some(CLIENT_ID))
                            .await?
                    };

                    (session.try_into()?, inner)
                }
                AuthMethod::RestoringSession => {
                    let session: Session = tokio::fs::read_to_string(content_store.session_file())
                        .await
                        .map(|s| toml::from_str(&s))?
                        .map_err(|e| ClientError::Custom(e.to_string()))?;

                    let inner = InnerClient::new(
                        get_homeserver(session.user_id.server_name().as_str())?,
                        Some(session.clone().into()),
                    );

                    (session, inner)
                }
            }
        };

        // Save the session
        if let Ok(encoded_session) = toml::to_vec(&session) {
            // Do not abort the sync if we can't save the session data
            if let Err(err) = tokio::fs::write(content_store.session_file(), encoded_session).await
            {
                log::error!("Could not save session data: {}", err);
            } else {
                use std::os::unix::fs::PermissionsExt;

                if let Err(err) = tokio::fs::set_permissions(
                    content_store.session_file(),
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
            content_store,
        })
    }

    pub async fn logout(inner: InnerClient, session_file: PathBuf) -> ClientResult<()> {
        inner.request(logout::Request::new()).await?;
        tokio::fs::remove_file(session_file).await?;
        Ok(())
    }

    pub fn content_store(&self) -> &ContentStore {
        &self.content_store
    }

    pub fn content_store_arc(&self) -> Arc<ContentStore> {
        self.content_store.clone()
    }

    pub fn current_user_id(&self) -> UserId {
        self.inner
            .session()
            .map(|s| s.identification.map(|id| id.user_id))
            .flatten()
            // This unwrap is safe since it is impossible to construct a `Client`
            // without a session (which MUST contain a user id)
            .unwrap()
    }

    pub fn next_batch(&self) -> Option<&str> {
        self.next_batch.as_deref()
    }

    pub fn inner(&self) -> InnerClient {
        self.inner.clone()
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

    pub async fn initial_sync(inner: InnerClient) -> ClientResult<sync_events::Response> {
        let lazy_load_filter = Client::member_lazy_load_sync_filter();

        inner
            .request(assign!(sync_events::Request::new(), {
                filter: Some(
                    // Lazy load room members here to ensure a fast login
                    &lazy_load_filter
                ),
                since: None,
                full_state: false,
                set_presence: &PresenceState::Online,
                timeout: None,
            }))
            .await
            .map_err(Into::into)
    }

    pub fn rooms_queued_events(&self) -> Vec<(RoomId, Vec<ActionRetry>)> {
        self.rooms
            .iter()
            .map(|(id, room)| {
                (
                    id.clone(),
                    room.queued_events()
                        .map(|event| {
                            let txn_id = event.transaction_id().copied().unwrap();
                            (
                                txn_id,
                                event.event().clone(),
                                room.get_wait_for_duration(&txn_id),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    pub async fn join_room(
        inner: InnerClient,
        room_id_or_alias: ruma::RoomIdOrAliasId,
    ) -> ClientResult<join_room_by_id_or_alias::Response> {
        inner
            .request(join_room_by_id_or_alias::Request::new(&room_id_or_alias))
            .await
            .map_err(Into::into)
    }

    pub async fn download_content(inner: InnerClient, content_url: Uri) -> ClientResult<Vec<u8>> {
        if let (Some(server_address), Some(content_id)) = (
            content_url
                .authority()
                .map(|a| a.as_str().try_into().ok())
                .flatten(),
            if content_url.path().is_empty() {
                None
            } else {
                Some(content_url.path().trim_matches('/'))
            },
        ) {
            inner
                .request(get_content::Request::new(content_id, server_address))
                .await
                .map_or_else(|err| Err(err.into()), |response| Ok(response.file))
        } else {
            Err(ClientError::Custom(String::from(
                "Could not make server address or content ID",
            )))
        }
    }

    pub async fn send_content(
        inner: InnerClient,
        data: Vec<u8>,
        content_type: Option<String>,
        filename: Option<String>,
    ) -> ClientResult<Uri> {
        let content_url = inner
                .request(assign!(create_content::Request::new(data), { content_type: content_type.as_deref(), filename: filename.as_deref() }))
                .await?
                .content_uri;

        content_url
            .parse::<Uri>()
            .map_err(|err| ClientError::URLParse(content_url, err))
    }

    pub async fn send_typing(
        inner: InnerClient,
        room_id: RoomId,
        user_id: UserId,
    ) -> ClientResult<create_typing_event::Response> {
        use create_typing_event::*;

        inner
            .request(Request::new(
                &user_id,
                &room_id,
                Typing::Yes(Duration::from_secs(1)),
            ))
            .await
            .map_err(Into::into)
    }

    pub async fn send_message(
        inner: InnerClient,
        content: AnyMessageEventContent,
        room_id: RoomId,
        txn_id: Uuid,
    ) -> ClientResult<send_message_event::Response> {
        inner
            .request(send_message_event::Request::new(
                &room_id,
                txn_id.to_string().as_str(),
                &content,
            ))
            .await
            .map_err(Into::into)
    }

    pub async fn get_events_around(
        inner: InnerClient,
        room_id: RoomId,
        event_id: EventId,
    ) -> ClientResult<get_context::Response> {
        inner
            .request(assign!(get_context::Request::new(&room_id, &event_id), {
                filter: Some(Client::member_lazy_load_room_event_filter()),
            }))
            .await
            .map_err(Into::into)
    }

    pub async fn get_events_backwards(
        inner: InnerClient,
        room_id: RoomId,
        from: String,
    ) -> ClientResult<get_message_events::Response> {
        inner
            .request(assign!(get_message_events::Request::backward(&room_id, &from), { limit: ruma::uint!(5_u32), filter: Some(Client::member_lazy_load_room_event_filter()) }))
            .await
            .map_err(Into::into)
    }

    pub fn process_events_backwards_response(
        &mut self,
        response: get_message_events::Response,
    ) -> Option<(RoomId, Vec<(bool, Uri)>)> {
        let get_message_events::Response {
            end: prev_batch,
            chunk,
            state,
            ..
        } = response;

        if let Some(Ok(event)) = chunk.first().map(|re| re.deserialize()) {
            let room_id = match event {
                AnyRoomEvent::Message(ev) => ev.room_id().clone(),
                AnyRoomEvent::State(ev) => ev.room_id().clone(),
                AnyRoomEvent::RedactedMessage(ev) => ev.room_id().clone(),
                AnyRoomEvent::RedactedState(ev) => ev.room_id().clone(),
            };

            if self.rooms.contains_key(&room_id) {
                let events: Vec<TimelineEvent> = chunk
                    .into_iter()
                    .flat_map(|r| r.deserialize().map(|e| e.into()))
                    .collect();

                let mut thumbnails = events
                    .iter()
                    .flat_map(|ev| ev.download_or_read_thumbnail(&self.content_store))
                    .collect::<Vec<_>>();

                let room = self.rooms.get_mut(&room_id).unwrap();

                room.add_events_backwards(events);
                room.add_state_backwards(
                    state
                        .into_iter()
                        .flat_map(|r| r.deserialize().map(|s| s.into()))
                        .collect(),
                );
                // TODO: move all thumbnail operations to the update method in main screen?
                // since update will get called anyways when we receive something
                for avatar_url in room.members().member_datas().flat_map(|m| m.avatar_url()) {
                    thumbnails.push((
                        self.content_store.content_exists(&avatar_url),
                        avatar_url.clone(),
                    ));
                }

                room.prev_batch = prev_batch;

                return Some((room_id, thumbnails));
            }
        }

        None
    }

    pub fn process_sync_response(&mut self, response: sync_events::Response) -> Vec<(bool, Uri)> {
        let mut thumbnails = vec![];
        let content_store = self.content_store_arc();

        for (room_id, joined_room) in response.rooms.join {
            if !joined_room.is_empty() {
                let room = self.rooms.entry(room_id).or_default();

                if let Some(token) = joined_room.timeline.prev_batch {
                    room.prev_batch = Some(token);
                }

                for event in joined_room
                    .ephemeral
                    .events
                    .iter()
                    .flat_map(|r| r.deserialize())
                {
                    if let AnyEphemeralRoomEventContent::Typing(TypingEventContent { user_ids }) =
                        event.content()
                    {
                        room.update_typing(user_ids);
                    }
                }
                // TODO: state redacts
                if !joined_room.state.is_empty() {
                    for event in joined_room
                        .state
                        .events
                        .iter()
                        .flat_map(|r| r.deserialize())
                    {
                        room.add_state(event.clone());
                        match event {
                            AnySyncStateEvent::RoomAvatar(SyncStateEvent {
                                content: AvatarEventContent { url, .. },
                                ..
                            }) => {
                                room.set_avatar(url.parse::<Uri>().ok());
                            }
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
                            _ => {}
                        }
                    }
                    room.recalculate_members();
                    // TODO: move all thumbnail operations to the update method in main screen?
                    // since update will get called anyways when we receive something
                    for avatar_url in room.members().member_datas().flat_map(|m| m.avatar_url()) {
                        thumbnails.push((
                            content_store.content_exists(&avatar_url),
                            avatar_url.clone(),
                        ));
                    }
                    if let Some(avatar_url) = room.avatar_url() {
                        thumbnails.push((
                            content_store.content_exists(&avatar_url),
                            avatar_url.clone(),
                        ));
                    }
                }
                for event in joined_room
                    .timeline
                    .events
                    .iter()
                    .flat_map(|r| r.deserialize())
                {
                    let tevent = TimelineEvent::new(event);
                    if let Some(transaction_id) = tevent.acks_transaction() {
                        room.ack_event(&transaction_id);
                    }
                    room.redact_event(&tevent);
                    if let Some(thumbnail_data) = tevent.download_or_read_thumbnail(&content_store)
                    {
                        thumbnails.push(thumbnail_data);
                    }
                    room.add_event(tevent);
                }
            }
        }
        for (room_id, _) in response.rooms.leave {
            self.rooms.remove(&room_id);
        }

        self.next_batch = Some(response.next_batch);
        thumbnails
    }
}
