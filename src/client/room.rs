use super::{member::Members, TimelineEvent};
use ahash::AHashMap;
use http::Uri;
use ruma::{events::AnySyncStateEvent, RoomAliasId, RoomId, RoomVersionId, UserId};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub type Rooms = AHashMap<RoomId, Room>;

pub struct Room {
    version: RoomVersionId,
    name: Option<String>,
    avatar_url: Option<Uri>,
    canonical_alias: Option<RoomAliasId>,
    alt_aliases: Vec<RoomAliasId>,
    timeline: Vec<TimelineEvent>,
    state: Vec<AnySyncStateEvent>,
    wait_for_duration: AHashMap<Uuid, Duration>,
    typing: (Vec<UserId>, Instant),
    members_cached: Members,
    pub prev_batch: Option<String>,
    pub loading_events_backward: bool,
    pub looking_at_event: usize,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            // FIXME: take this as arg
            version: RoomVersionId::Version5,
            name: None,
            avatar_url: None,
            canonical_alias: None,
            alt_aliases: Default::default(),
            timeline: Default::default(),
            state: Default::default(),
            wait_for_duration: Default::default(),
            typing: (Default::default(), Instant::now()),
            members_cached: Default::default(),
            prev_batch: Default::default(),
            loading_events_backward: false,
            looking_at_event: 0,
        }
    }
}

impl Room {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the name of this room.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn avatar_url(&self) -> Option<&Uri> {
        self.avatar_url.as_ref()
    }

    pub fn set_avatar(&mut self, url: Option<Uri>) {
        self.avatar_url = url;
    }

    /// Get the canonical alias of this room.
    pub fn canonical_alias(&self) -> Option<&str> {
        self.canonical_alias.as_ref().map(|s| s.as_str())
    }

    /// Get the alternative aliases of this room.
    pub fn alt_aliases(&self) -> &[RoomAliasId] {
        self.alt_aliases.as_slice()
    }

    /// Get all events in the timeline.
    pub fn timeline(&self) -> &[TimelineEvent] {
        self.timeline.as_slice()
    }

    pub fn queued_events(&self) -> impl Iterator<Item = &TimelineEvent> + '_ {
        self.timeline.iter().filter(|event| !event.is_ack())
    }

    /// Get all the displayable events in the timeline.
    pub fn displayable_events(&self) -> impl Iterator<Item = &TimelineEvent> + '_ {
        self.timeline
            .iter()
            .filter(|event| event.should_show_to_user())
    }

    /// Get the oldest event in the timeline.
    pub fn oldest_event(&self) -> Option<&TimelineEvent> {
        self.timeline.first()
    }

    /// Get the newest event in the timeline.
    pub fn newest_event(&self) -> Option<&TimelineEvent> {
        self.timeline.last()
    }

    pub(super) fn recalculate_members(&mut self) {
        let mut members = Members::default();

        for state in &self.state {
            if let AnySyncStateEvent::RoomMember(member_state) = state {
                let member_state = member_state.clone();
                let membership_change = member_state.membership_change();
                members.update_member(
                    member_state.prev_content.map(|c| c.displayname).flatten(),
                    member_state.content.displayname,
                    member_state
                        .content
                        .avatar_url
                        .map(|u| u.parse::<http::Uri>().ok())
                        .flatten(),
                    membership_change,
                    member_state.sender,
                );
            }
        }

        members.update_typing(self.typing.0.as_slice(), self.typing.1);

        self.members_cached = members;
    }

    pub fn members(&self) -> &Members {
        &self.members_cached
    }

    /// Get room display name formatted according to the specification.
    pub fn get_display_name(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => {
                let members = &self.members_cached;
                match &self.canonical_alias {
                    Some(alias) => alias.to_string(),
                    None => match members.len() {
                        // FIXME: Use "heroes" here according to the spec
                        // These unwraps are safe since we check the length beforehand
                        x if x > 2 => format!(
                            "{}, {} and {} others",
                            members.get_user_display_name(members.member_ids().next().unwrap()),
                            members.get_user_display_name(members.member_ids().nth(1).unwrap()),
                            members.len() - 2
                        ),
                        2 => format!(
                            "{} and {}",
                            members.get_user_display_name(members.member_ids().next().unwrap()),
                            members.get_user_display_name(members.member_ids().nth(1).unwrap()),
                        ),
                        _ => String::from("Empty room"),
                    },
                }
            }
        }
    }

    pub fn update_typing(&mut self, typing_ids: Vec<UserId>) {
        self.typing = (typing_ids, Instant::now());
        self.members_cached
            .update_typing(self.typing.0.as_slice(), self.typing.1);
    }

    pub fn add_state(&mut self, state: AnySyncStateEvent) {
        self.state.push(state);
        // We don't recalculate_members here and instead do it when we finish adding all of the state of a sync response.
    }

    pub fn add_state_backwards(&mut self, state: Vec<AnySyncStateEvent>) {
        for state_event in state {
            self.state.insert(0, state_event);
        }
        self.recalculate_members();
    }

    pub fn add_events_backwards(&mut self, events: Vec<TimelineEvent>) {
        for event in events {
            self.timeline.insert(0, event);
        }
    }

    pub fn add_event(&mut self, event: TimelineEvent) {
        if !event.is_ack() || !self.timeline.contains(&event) {
            self.timeline.push(event);
        }
    }

    pub fn redact_event(&mut self, redaction_event: &TimelineEvent) {
        if let Some(rid) = redaction_event.redacts() {
            if let Some(index) = self.timeline.iter().position(|tevent| tevent.id() == rid) {
                let redacted_tevent = self
                    .timeline
                    .remove(index)
                    .redact(redaction_event, self.version.clone());
                self.timeline.insert(index, redacted_tevent);
            }
        }
    }

    pub fn wait_for_duration(&mut self, duration: Duration, transaction_id: Uuid) {
        *self
            .wait_for_duration
            .entry(transaction_id)
            .or_insert(duration) = duration;
    }

    pub fn get_wait_for_duration(&self, transaction_id: &Uuid) -> Option<Duration> {
        self.wait_for_duration.get(transaction_id).copied()
    }

    pub fn ack_event(&mut self, transaction_id: &Uuid) {
        self.wait_for_duration.remove(transaction_id);
        if let Some(index) = self
            .timeline
            .iter()
            .position(|tevent| tevent.transaction_id() == Some(transaction_id))
        {
            self.timeline.remove(index);
        }
    }

    /// Set the name of this room.
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
    }

    /// Set the canonical alias of this room.
    pub fn set_canonical_alias(&mut self, canonical_alias: Option<RoomAliasId>) {
        self.canonical_alias = canonical_alias;
    }

    /// Set alternative aliases of this room.
    pub fn set_alt_aliases(&mut self, alt_aliases: Vec<RoomAliasId>) {
        self.alt_aliases = alt_aliases;
    }
}
