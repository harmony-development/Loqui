use super::{member::Members, TimelineEvent};
use ahash::AHashMap;
use ruma::{EventId, RoomAliasId, RoomId, RoomVersionId};
use std::time::Duration;
use uuid::Uuid;

pub type Rooms = AHashMap<RoomId, Room>;

pub struct Room {
    version: RoomVersionId,
    name: Option<String>,
    canonical_alias: Option<RoomAliasId>,
    alt_aliases: Vec<RoomAliasId>,
    timeline: Vec<TimelineEvent>,
    pub members: Members,
    wait_for_duration: AHashMap<Uuid, Duration>,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            // FIXME: take this as arg
            version: RoomVersionId::Version5,
            name: None,
            canonical_alias: None,
            alt_aliases: Default::default(),
            timeline: Default::default(),
            members: Default::default(),
            wait_for_duration: Default::default(),
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

    /// Get room display name formatted according to the specification.
    pub fn get_display_name(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => match &self.canonical_alias {
                Some(alias) => alias.to_string(),
                None => match self.members.len() {
                    // FIXME: Use "heroes" here according to the spec
                    // These unwraps are safe since we check the length beforehand
                    x if x > 2 => format!(
                        "{}, {} and {} others",
                        self.members
                            .get_user_display_name(self.members.member_ids().next().unwrap()),
                        self.members
                            .get_user_display_name(self.members.member_ids().nth(1).unwrap()),
                        self.members.len() - 2
                    ),
                    2 => format!(
                        "{} and {}",
                        self.members
                            .get_user_display_name(self.members.member_ids().next().unwrap()),
                        self.members
                            .get_user_display_name(self.members.member_ids().nth(1).unwrap()),
                    ),
                    _ => String::from("Empty room"),
                },
            },
        }
    }

    pub fn add_chunk_of_events(
        &mut self,
        events_before: Vec<TimelineEvent>,
        events_after: Vec<TimelineEvent>,
        point_event_id: &EventId,
    ) {
        if let Some(point_index) = self
            .timeline
            .iter()
            .position(|tevent| tevent.id() == point_event_id)
        {
            let mut point_index_offset = 0;
            let mut i = point_index;
            for event in events_before {
                if let Some(ci) = i.checked_sub(1) {
                    if event != self.timeline[ci] {
                        self.timeline.insert(i, event);
                        point_index_offset += 1;
                    } else {
                        i -= 1;
                    }
                } else {
                    self.timeline.insert(0, event);
                    point_index_offset += 1;
                }
            }

            i = point_index + point_index_offset;
            for event in events_after {
                if i + 1 < self.timeline.len() {
                    if event != self.timeline[i + 1] {
                        self.timeline.insert(i + 1, event);
                    } else {
                        i += 1;
                    }
                } else {
                    self.timeline.insert(self.timeline.len(), event);
                    i += 1;
                }
            }
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
