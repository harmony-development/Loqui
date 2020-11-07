use super::timeline_event::TimelineEvent;
use ruma::{
    events::room::member::MembershipChange, EventId, RoomAliasId, RoomId, RoomVersionId, UserId,
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub type Members = HashMap<UserId, Member>;

pub struct Member {
    display_name: Option<String>,
    display_user: bool,
    typing_received: Option<Duration>,
}

impl Default for Member {
    fn default() -> Self {
        Self {
            display_name: None,
            display_user: true,
            typing_received: None,
        }
    }
}

impl Member {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn is_typing(&self) -> bool {
        self.typing_received.is_some()
    }

    pub fn set_display_name(&mut self, new_display_name: Option<String>) {
        self.display_name = new_display_name;
    }

    pub fn set_display(&mut self, display: bool) {
        self.display_user = display;
    }

    pub fn set_typing(&mut self, typing_recieved: Option<Duration>) {
        self.typing_received = typing_recieved;
    }
}

pub type Rooms = HashMap<RoomId, Room>;

pub struct Room {
    version: RoomVersionId,
    name: Option<String>,
    canonical_alias: Option<RoomAliasId>,
    alt_aliases: Vec<RoomAliasId>,
    timeline: Vec<TimelineEvent>,
    members: Members,
    display_name_to_user_id: HashMap<String, Vec<UserId>>,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            // FIXME: take this as arg
            version: RoomVersionId::Version5,
            name: None,
            canonical_alias: None,
            alt_aliases: vec![],
            timeline: vec![],
            members: Members::new(),
            display_name_to_user_id: HashMap::new(),
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

    /// Get all the displayable events in the timeline.
    pub fn displayable_events(&self) -> Vec<&TimelineEvent> {
        self.timeline()
            .iter()
            .filter(|event| event.should_show_to_user())
            .collect()
    }

    /// Get the oldest event in the timeline.
    pub fn oldest_event(&self) -> Option<&TimelineEvent> {
        self.timeline.first()
    }

    /// Get the newest event in the timeline.
    pub fn newest_event(&self) -> Option<&TimelineEvent> {
        self.timeline.last()
    }

    /// Get all members in this room.
    pub fn members(&self) -> &Members {
        &self.members
    }

    pub fn get_member(&self, user_id: &UserId) -> Option<&Member> {
        self.members.get(user_id)
    }

    pub fn get_member_mut(&mut self, user_id: &UserId) -> Option<&mut Member> {
        self.members.get_mut(user_id)
    }

    /// Get all typing members in this room.
    pub fn typing_members(&self) -> Vec<&UserId> {
        self.members
            .iter()
            .filter(|(_, member)| member.is_typing())
            .map(|(id, _)| id)
            .collect()
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
                        self.get_user_display_name(self.members.keys().next().unwrap()),
                        self.get_user_display_name(self.members.keys().nth(1).unwrap()),
                        self.members.len() - 2
                    ),
                    2 => format!(
                        "{} and {}",
                        self.get_user_display_name(self.members.keys().next().unwrap()),
                        self.get_user_display_name(self.members.keys().nth(1).unwrap()),
                    ),
                    _ => String::from("Empty room"),
                },
            },
        }
    }

    /// Get a user's display name (disambugiated name) according to the specification.
    pub fn get_user_display_name(&self, user_id: &UserId) -> String {
        if let Some(member) = self.members.get(user_id) {
            if let Some(name) = member.display_name() {
                if let Some(ids) = self.display_name_to_user_id.get(name) {
                    if ids.len() > 1 {
                        return format!("{} ({})", name, user_id);
                    }
                }
                return name.to_string();
            }
        }
        user_id.to_string()
    }

    pub fn update_typing(&mut self, typing_member_ids: &[UserId]) {
        for member in self.members.values_mut() {
            member.set_typing(None);
        }

        for member_id in typing_member_ids {
            if let Some(member) = self.members.get_mut(member_id) {
                member.set_typing(Some(Instant::now().elapsed()))
            }
        }
    }

    pub fn update_member(
        &mut self,
        prev_displayname: Option<Option<String>>,
        displayname: Option<String>,
        membership_change: MembershipChange,
        user_id: UserId,
    ) {
        self.members
            .entry(user_id.clone())
            .and_modify(|member| member.set_display_name(displayname.clone()))
            .or_insert_with(move || {
                let mut new_member = Member::new();
                new_member.set_display_name(displayname);
                new_member
            });

        if let MembershipChange::Left = membership_change {
            for ids in self.display_name_to_user_id.values_mut() {
                if let Some(index) = ids.iter().position(|id| id == &user_id) {
                    ids.remove(index);
                }
            }

            if let Some(member) = self.get_member_mut(&user_id) {
                member.set_display(false);
            }
        } else if let MembershipChange::Joined = membership_change {
            // This unwrap is safe since we add a member if they don't exist beforehand
            if let Some(name) = self.members.get(&user_id).unwrap().display_name() {
                let ids = self
                    .display_name_to_user_id
                    .entry(name.to_string())
                    .or_insert_with(|| vec![user_id.clone()]);

                if !ids.contains(&user_id) {
                    ids.push(user_id.clone());
                }
            }

            if let Some(member) = self.get_member_mut(&user_id) {
                member.set_display(true);
            }
        } else if let MembershipChange::ProfileChanged {
            displayname_changed,
            avatar_url_changed: _,
        } = membership_change
        {
            if displayname_changed {
                if let Some(Some(name)) = prev_displayname {
                    if let Some(ids) = self.display_name_to_user_id.get_mut(&name) {
                        if let Some(index) = ids.iter().position(|id| id == &user_id) {
                            ids.remove(index);
                        }
                    }
                }
                // This unwrap is safe since we add a member if they don't exist beforehand
                if let Some(name) = self.members.get(&user_id).unwrap().display_name() {
                    if let Some(ids) = self.display_name_to_user_id.get_mut(name) {
                        ids.push(user_id);
                    }
                }
            }
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

    pub fn ack_event(&mut self, ack_event: &TimelineEvent) {
        if let Some(index) = self
            .timeline
            .iter()
            .position(|tevent| tevent.transaction_id() == ack_event.acks_transaction().as_ref())
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
