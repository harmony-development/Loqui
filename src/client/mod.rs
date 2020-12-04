use assign::assign;
use error::{ClientError, ClientResult};
pub use room::{Room, Rooms};
use ruma::{
    api::client::r0::media::{create_content, get_content},
    api::{
        client::r0::{
            context::get_context,
            filter::{FilterDefinition, LazyLoadOptions, RoomEventFilter, RoomFilter},
            membership::join_room_by_id_or_alias,
            message::send_message_event,
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
        room::{aliases::AliasesEventContent, canonical_alias::CanonicalAliasEventContent},
        typing::TypingEventContent,
        AnyEphemeralRoomEventContent, AnyMessageEventContent, AnyRoomEvent, AnySyncRoomEvent,
        AnySyncStateEvent, SyncStateEvent,
    },
    presence::PresenceState,
    serde::Raw,
    DeviceId, EventId, RoomId, UserId,
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

pub mod error;
pub mod media;
pub mod member;
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
    pub async fn new(homeserver: &str, username: &str, password: &str) -> ClientResult<Self> {
        // Make sure the data/content directory exists
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

    pub fn new_with_session(session: Session) -> ClientResult<Self> {
        // Make sure the data/content directory exists
        std::fs::create_dir_all(format!("{}content", data_dir!()))?;

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

    pub async fn logout(inner: InnerClient) -> ClientResult<()> {
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

    pub async fn initial_sync(&mut self) -> ClientResult<()> {
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

    pub fn remove_room(&mut self, room_id: &RoomId) {
        self.rooms.remove(room_id);
    }

    pub fn has_room(&self, room_id: &RoomId) -> bool {
        self.rooms.contains_key(room_id)
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

    pub fn rooms_queued_events(
        &self,
    ) -> Vec<(RoomId, Vec<(Uuid, AnySyncRoomEvent, Option<Duration>)>)> {
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
                                event.event_content().clone(),
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
                .map(|a| a.as_str().try_into().map_or(None, Some))
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
        current_user_id: UserId,
    ) -> ClientResult<create_typing_event::Response> {
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
    ) -> ClientResult<send_message_event::Response> {
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
    ) -> ClientResult<get_context::Response> {
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
    ) -> Vec<(bool, Uri)> {
        let mut thumbnails = vec![];

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
                        .map(Into::into)
                        .collect()
                }

                let (room_id, event_id) = convert_room_to_sync_room_with_id(event);

                if let Some(room) = self.get_room_mut(&room_id) {
                    let events_before = convert_to_timeline_event(events_before);
                    let events_after = convert_to_timeline_event(events_after);
                    thumbnails = events_after
                        .iter()
                        .chain(events_before.iter())
                        .flat_map(|tevent| tevent.download_or_read_thumbnail())
                        .collect::<Vec<_>>();
                    room.add_chunk_of_events(events_before, events_after, &event_id);
                }
            }
        }
        thumbnails
    }

    pub fn process_sync_response(&mut self, response: sync_events::Response) -> Vec<(bool, Uri)> {
        let mut thumbnails = vec![];

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
                    room.update_typing(user_ids.as_slice(), std::time::Instant::now());
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
                            member_state.prev_content.map(|c| c.displayname).flatten(),
                            member_state.content.displayname,
                            member_state
                                .content
                                .avatar_url
                                .map(|u| u.parse::<Uri>().map_or(None, Some))
                                .flatten(),
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
                if let Some(transaction_id) = tevent.acks_transaction() {
                    room.ack_event(&transaction_id);
                }
                room.redact_event(&tevent);
                if let Some(thumbnail_data) = tevent.download_or_read_thumbnail() {
                    thumbnails.push(thumbnail_data);
                }

                room.add_event(tevent);
            }
        }
        for (room_id, _) in response.rooms.leave {
            self.remove_room(&room_id);
        }

        self.next_batch = Some(response.next_batch);
        thumbnails
    }
}
