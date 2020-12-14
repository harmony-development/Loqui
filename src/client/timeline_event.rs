use super::{
    content::{ContentStore, ContentType},
    member::Members,
};
use ruma::{
    api::exports::http::Uri,
    events::{
        room::{
            member::MembershipChange, message::MessageEventContent, redaction::SyncRedactionEvent,
        },
        AnyMessageEventContent, AnyRoomEvent, AnyStateEventContent, AnySyncMessageEvent,
        AnySyncRoomEvent, AnySyncStateEvent, SyncMessageEvent, Unsigned,
    },
    EventId, RoomVersionId, UserId,
};
use std::{convert::TryFrom, time::SystemTime};
use uuid::Uuid;

pub struct TimelineEvent {
    inner: AnySyncRoomEvent,
    transaction_id: Option<Uuid>,
}

impl From<AnySyncRoomEvent> for TimelineEvent {
    fn from(ev: AnySyncRoomEvent) -> Self {
        TimelineEvent::new(ev)
    }
}

impl From<AnyRoomEvent> for TimelineEvent {
    fn from(ev: AnyRoomEvent) -> Self {
        TimelineEvent::new(match ev {
            AnyRoomEvent::Message(ev) => AnySyncRoomEvent::Message(ev.into()),
            AnyRoomEvent::State(ev) => AnySyncRoomEvent::State(ev.into()),
            AnyRoomEvent::RedactedMessage(ev) => AnySyncRoomEvent::RedactedMessage(ev.into()),
            AnyRoomEvent::RedactedState(ev) => AnySyncRoomEvent::RedactedState(ev.into()),
        })
    }
}

impl TimelineEvent {
    pub fn new(event: AnySyncRoomEvent) -> Self {
        Self {
            inner: event,
            transaction_id: None,
        }
    }

    pub fn new_unacked_message(content: MessageEventContent, transaction_id: Uuid) -> Self {
        Self {
            inner: AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomMessage(SyncMessageEvent {
                content,
                // FIXME: Dirty hack, replace this whole thing with an enum
                event_id: ruma::event_id!("$Rqnc-F-dvnEYJTyHq_iKxU2bZ1CI92-kuZq3a5lr5Zg"),
                sender: ruma::user_id!("@default:default.com"),
                origin_server_ts: SystemTime::now(),
                unsigned: Unsigned {
                    age: None,
                    transaction_id: None,
                },
            })),
            transaction_id: Some(transaction_id),
        }
    }

    /// Get a formatted string representation of this event.
    /// It is recommended to first check if this event should be displayed to the user.
    pub fn formatted(&self, members: &Members) -> String {
        match self.message_content() {
            Some(content) => match content {
                AnyMessageEventContent::RoomMessage(msg) => match msg {
                    MessageEventContent::Image(image) => format!("sent an image: {}", image.body),
                    MessageEventContent::Video(video) => format!("sent a video: {}", video.body),
                    MessageEventContent::Audio(audio) => {
                        format!("sent an audio file: {}", audio.body)
                    }
                    MessageEventContent::File(file) => format!("sent a file: {}", file.body),
                    MessageEventContent::Location(location) => {
                        format!("sent a location: {}", location.body)
                    }
                    MessageEventContent::Notice(notice) => notice.body,
                    MessageEventContent::ServerNotice(server_notice) => server_notice.body,
                    MessageEventContent::Text(text) => text.body,
                    MessageEventContent::Emote(emote) => format!("* {} *", emote.body),
                    _ => String::from("Unknown message content"),
                },
                _ => String::from("Unknown message type"),
            },
            None => match self.message_redacted_because() {
                Some(because) => format!(
                    "Message deleted by [{}]",
                    members.get_user_display_name(&because.sender)
                ),
                None => {
                    if let AnySyncRoomEvent::State(AnySyncStateEvent::RoomMember(member_state)) =
                        &self.inner
                    {
                        let affected_user_name = UserId::try_from(member_state.state_key.as_str())
                            .map(|id| members.get_user_display_name(&id))
                            .unwrap_or_else(|_| member_state.state_key.to_string());
                        let banned_kicked_msg = |action: &str| -> String {
                            format!(
                                // TODO: implement reason
                                "{} [{}]",
                                action,
                                affected_user_name,
                            )
                        };

                        match member_state.membership_change() {
                            MembershipChange::Banned => banned_kicked_msg("banned"),
                            MembershipChange::KickedAndBanned => {
                                banned_kicked_msg("kicked and banned")
                            }
                            MembershipChange::Kicked => banned_kicked_msg("kicked"),
                            MembershipChange::Joined => String::from("joined the room"),
                            MembershipChange::Left => String::from("left the room"),
                            MembershipChange::ProfileChanged {
                                displayname_changed,
                                avatar_url_changed,
                            } => {
                                let mut msg = String::new();
                                if displayname_changed {
                                    if let (Some(Some(prev_display_name)), Some(cur_display_name)) = (
                                        member_state.prev_content.as_ref().map(|c| &c.displayname),
                                        &member_state.content.displayname,
                                    ) {
                                        msg = format!(
                                            "changed their display name from \"{}\" to \"{}\"",
                                            prev_display_name, cur_display_name
                                        );
                                        if avatar_url_changed {
                                            msg = format!("{}\n", msg);
                                        }
                                    }
                                }
                                if avatar_url_changed {
                                    msg = format!("{}changed their profile picture", msg);
                                }
                                msg
                            }
                            _ => String::from("Unknown membership information change"),
                        }
                    } else if let Some(content) = self.state_content() {
                        fn format_room_content_change(
                            new_data: Option<String>,
                            change_type: &str,
                        ) -> String {
                            if let Some(c) = new_data {
                                format!("changed room {} to \"{}\"", change_type, c)
                            } else {
                                format!("removed room {}", change_type)
                            }
                        }

                        match content {
                            AnyStateEventContent::RoomName(room_name) => {
                                format_room_content_change(
                                    room_name.name().map(|n| n.to_string()),
                                    "name",
                                )
                            }
                            AnyStateEventContent::RoomTopic(room_topic) => {
                                format_room_content_change(
                                    if room_topic.topic.is_empty() {
                                        None
                                    } else {
                                        Some(room_topic.topic)
                                    },
                                    "topic",
                                )
                            }
                            AnyStateEventContent::RoomCanonicalAlias(room_canonical_alias) => {
                                format_room_content_change(
                                    room_canonical_alias.alias.map(|a| a.to_string()),
                                    "canonical alias",
                                )
                            }
                            AnyStateEventContent::RoomHistoryVisibility(
                                room_history_visibility,
                            ) => format_room_content_change(
                                Some(room_history_visibility.history_visibility.to_string()),
                                "history visibility",
                            ),
                            AnyStateEventContent::RoomJoinRules(room_join_rules) => {
                                format_room_content_change(
                                    Some(room_join_rules.join_rule.to_string()),
                                    "join rule",
                                )
                            }
                            AnyStateEventContent::RoomCreate(_) => {
                                String::from("created and configured the room")
                            }
                            _ => String::from("Unknown state type"),
                        }
                    } else {
                        String::from("Unknown event type")
                    }
                }
            },
        }
    }

    pub fn content_type(&self) -> Option<ContentType> {
        if let Some(content) = self.message_content() {
            if let AnyMessageEventContent::RoomMessage(content) = content {
                return match content {
                    MessageEventContent::Image(_) => Some(ContentType::Image),
                    MessageEventContent::Video(_) => Some(ContentType::Video),
                    MessageEventContent::Audio(_) => Some(ContentType::Audio),
                    MessageEventContent::File(_) => Some(ContentType::Other),
                    _ => None,
                };
            }
        }
        None
    }

    pub fn content_url(&self) -> Option<Uri> {
        if let Some(content) = self.message_content() {
            if let AnyMessageEventContent::RoomMessage(content) = content {
                return match content {
                    MessageEventContent::Image(image) => image.url,
                    MessageEventContent::Video(video) => video.url,
                    MessageEventContent::Audio(audio) => audio.url,
                    MessageEventContent::File(file) => file.url,
                    _ => None,
                }
                .map(|u| u.parse::<Uri>().map_or_else(|_| None, Some))
                .unwrap_or(None);
            }
        }
        None
    }

    pub fn content_size(&self) -> Option<usize> {
        if let Some(content) = self.message_content() {
            if let AnyMessageEventContent::RoomMessage(content) = content {
                return match content {
                    MessageEventContent::Image(image) => {
                        image.info.map(|i| i.size.map(|s| u64::from(s) as usize))
                    }
                    MessageEventContent::Video(video) => {
                        video.info.map(|i| i.size.map(|s| u64::from(s) as usize))
                    }
                    MessageEventContent::Audio(audio) => {
                        audio.info.map(|i| i.size.map(|s| u64::from(s) as usize))
                    }
                    MessageEventContent::File(file) => {
                        file.info.map(|i| i.size.map(|s| u64::from(s) as usize))
                    }
                    _ => None,
                }
                .flatten();
            }
        }
        None
    }

    pub fn thumbnail_url(&self) -> Option<Uri> {
        if let Some(content) = self.message_content() {
            if let AnyMessageEventContent::RoomMessage(content) = content {
                return match content {
                    MessageEventContent::Image(image) => {
                        image.info.map(|i| i.thumbnail_url).flatten()
                    }
                    MessageEventContent::Video(video) => {
                        video.info.map(|i| i.thumbnail_url).flatten()
                    }
                    MessageEventContent::File(file) => file.info.map(|i| i.thumbnail_url).flatten(),
                    _ => None,
                }
                .map(|u| u.parse::<Uri>().map_or_else(|_| None, Some))
                .unwrap_or(None);
            }
        }
        None
    }

    pub fn download_or_read_thumbnail(&self, content_store: &ContentStore) -> Option<(bool, Uri)> {
        if let Some(thumbnail_url) = self.thumbnail_url() {
            Some((
                if content_store.content_exists(&thumbnail_url.to_string()) {
                    true
                } else {
                    false
                },
                thumbnail_url,
            ))
        } else if let (Some(ContentType::Image), Some(content_size), Some(content_url)) =
            (self.content_type(), self.content_size(), self.content_url())
        {
            if content_store.content_exists(&content_url.to_string()) {
                Some((true, content_url))
            } else if content_size < 1000 * 1000 {
                Some((false, content_url))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if we should show this event to the user.
    pub fn should_show_to_user(&self) -> bool {
        match self.message_content() {
            Some(content) => matches!(content, AnyMessageEventContent::RoomMessage(_)),
            None => match self.message_redacted_because() {
                Some(_) => true,
                None => {
                    if let AnySyncRoomEvent::State(AnySyncStateEvent::RoomMember(member_state)) =
                        &self.inner
                    {
                        matches!(
                            member_state.membership_change(),
                            MembershipChange::Kicked
                                | MembershipChange::Banned
                                | MembershipChange::KickedAndBanned
                                | MembershipChange::Left
                                | MembershipChange::Joined
                                | MembershipChange::ProfileChanged {
                                    displayname_changed: true,
                                    avatar_url_changed: true,
                                }
                                | MembershipChange::ProfileChanged {
                                    displayname_changed: true,
                                    avatar_url_changed: false,
                                }
                                | MembershipChange::ProfileChanged {
                                    displayname_changed: false,
                                    avatar_url_changed: true,
                                }
                        )
                    } else if let Some(content) = self.state_content() {
                        matches!(
                            content,
                            AnyStateEventContent::RoomName(_)
                                | AnyStateEventContent::RoomTopic(_)
                                | AnyStateEventContent::RoomCanonicalAlias(_)
                                | AnyStateEventContent::RoomHistoryVisibility(_)
                                | AnyStateEventContent::RoomJoinRules(_)
                                | AnyStateEventContent::RoomCreate(_)
                        )
                    } else {
                        false
                    }
                }
            },
        }
    }

    pub fn id(&self) -> &EventId {
        match &self.inner {
            AnySyncRoomEvent::Message(ev) => ev.event_id(),
            AnySyncRoomEvent::RedactedMessage(ev) => ev.event_id(),
            AnySyncRoomEvent::State(ev) => ev.event_id(),
            AnySyncRoomEvent::RedactedState(ev) => ev.event_id(),
        }
    }

    pub fn transaction_id(&self) -> Option<&Uuid> {
        self.transaction_id.as_ref()
    }

    pub fn sender(&self) -> &UserId {
        match &self.inner {
            AnySyncRoomEvent::Message(ev) => ev.sender(),
            AnySyncRoomEvent::RedactedMessage(ev) => ev.sender(),
            AnySyncRoomEvent::State(ev) => ev.sender(),
            AnySyncRoomEvent::RedactedState(ev) => ev.sender(),
        }
    }

    pub fn origin_server_timestamp(&self) -> &SystemTime {
        match &self.inner {
            AnySyncRoomEvent::Message(ev) => ev.origin_server_ts(),
            AnySyncRoomEvent::RedactedMessage(ev) => ev.origin_server_ts(),
            AnySyncRoomEvent::State(ev) => ev.origin_server_ts(),
            AnySyncRoomEvent::RedactedState(ev) => ev.origin_server_ts(),
        }
    }

    pub fn is_message(&self) -> bool {
        matches!(&self.inner, AnySyncRoomEvent::Message(_))
    }

    pub fn is_emote_message(&self) -> bool {
        matches!(
            &self.inner,
            AnySyncRoomEvent::Message(
                AnySyncMessageEvent::RoomMessage(SyncMessageEvent {
                    content: MessageEventContent::Emote(_),
                    ..
                }),
            )
        )
    }

    pub fn is_redacted_message(&self) -> bool {
        matches!(&self.inner, AnySyncRoomEvent::RedactedMessage(_))
    }

    pub fn message_content(&self) -> Option<AnyMessageEventContent> {
        if let AnySyncRoomEvent::Message(ev) = &self.inner {
            Some(ev.content())
        } else {
            None
        }
    }

    pub fn message_redacted_because(&self) -> Option<&SyncRedactionEvent> {
        if let AnySyncRoomEvent::RedactedMessage(ev) = &self.inner {
            ev.unsigned().redacted_because.as_deref()
        } else {
            None
        }
    }

    pub fn is_state(&self) -> bool {
        matches!(&self.inner, AnySyncRoomEvent::State(_))
    }

    pub fn is_redacted_state(&self) -> bool {
        matches!(&self.inner, AnySyncRoomEvent::RedactedState(_))
    }

    pub fn state_content(&self) -> Option<AnyStateEventContent> {
        if let AnySyncRoomEvent::State(ev) = &self.inner {
            Some(ev.content())
        } else {
            None
        }
    }

    pub fn state_redacted_because(&self) -> Option<&SyncRedactionEvent> {
        if let AnySyncRoomEvent::RedactedState(ev) = &self.inner {
            ev.unsigned().redacted_because.as_deref()
        } else {
            None
        }
    }

    pub fn event(&self) -> &AnySyncRoomEvent {
        &self.inner
    }

    /// Check if this event is acknowledged by the server.
    pub fn is_ack(&self) -> bool {
        self.transaction_id.is_none()
    }

    /// Returns which transaction this event acknowledges.
    pub fn acks_transaction(&self) -> Option<Uuid> {
        if let AnySyncRoomEvent::Message(ref msg_event) = self.inner {
            if let AnySyncMessageEvent::RoomMessage(event) = msg_event {
                if let Some(Ok(uuid)) = event.unsigned.transaction_id.as_ref().map(|s| s.parse()) {
                    return Some(uuid);
                }
            }
        }
        None
    }

    /// Return which event this event redacts.
    pub fn redacts(&self) -> Option<&EventId> {
        match &self.inner {
            AnySyncRoomEvent::Message(ev) => {
                if let AnySyncMessageEvent::RoomRedaction(ev) = ev {
                    Some(&ev.redacts)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Redact the inner Matrix event.
    pub fn redact(self, redaction_event: &TimelineEvent, room_version: RoomVersionId) -> Self {
        let mut redacted = self;
        redacted.inner = if let AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomRedaction(rev)) =
            &redaction_event.inner
        {
            match redacted.inner {
                AnySyncRoomEvent::Message(ev) => {
                    AnySyncRoomEvent::RedactedMessage(ev.redact(rev.clone(), room_version))
                }
                AnySyncRoomEvent::State(ev) => {
                    AnySyncRoomEvent::RedactedState(ev.redact(rev.clone(), room_version))
                }
                _ => redacted.inner,
            }
        } else {
            redacted.inner
        };
        redacted
    }
}

impl PartialEq for TimelineEvent {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for TimelineEvent {}
