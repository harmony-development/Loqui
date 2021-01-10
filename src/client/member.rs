use ahash::AHashMap;
use ruma::{api::exports::http::Uri, events::room::member::MembershipChange, identifiers::UserId};
use std::collections::hash_map::{Keys, Values};
use std::time::Instant;

pub struct Member {
    avatar_url: Option<Uri>,
    display_name: Option<String>,
    display_user: bool,
    typing_received: Option<Instant>,
}

impl Default for Member {
    fn default() -> Self {
        Self {
            avatar_url: None,
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

    pub fn avatar_url(&self) -> Option<&Uri> {
        self.avatar_url.as_ref()
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn display(&self) -> bool {
        self.display_user
    }

    pub fn is_typing(&self) -> bool {
        self.typing_received.is_some()
    }

    pub fn set_avatar_url(&mut self, new_avatar_url: Option<Uri>) {
        self.avatar_url = new_avatar_url;
    }

    pub fn set_display_name(&mut self, new_display_name: Option<String>) {
        self.display_name = new_display_name;
    }

    pub fn set_typing(&mut self, new_typing_recieved: Option<Instant>) {
        self.typing_received = new_typing_recieved;
    }
}

#[derive(Default)]
pub struct Members {
    members: AHashMap<UserId, Member>,
    display_name_to_user_id: AHashMap<String, Vec<UserId>>,
}

impl Members {
    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() < 1
    }

    /// Get all members in this room.
    pub fn members(&self) -> &AHashMap<UserId, Member> {
        &self.members
    }

    pub fn member_ids(&self) -> Keys<UserId, Member> {
        self.members.keys()
    }

    pub fn member_datas(&self) -> Values<UserId, Member> {
        self.members.values()
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
            .filter_map(|(id, member)| if member.is_typing() { Some(id) } else { None })
            .collect()
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

    pub fn update_typing(&mut self, typing_member_ids: &[UserId], recv_time: Instant) {
        for member in self.members.values_mut() {
            member.set_typing(None);
        }

        for member_id in typing_member_ids {
            if let Some(member) = self.members.get_mut(&member_id) {
                member.set_typing(Some(recv_time))
            }
        }
    }

    pub fn update_member(
        &mut self,
        prev_displayname: Option<String>,
        displayname: Option<String>,
        avatar_url: Option<Uri>,
        membership_change: MembershipChange,
        user_id: UserId,
    ) {
        let member = self.members.entry(user_id.clone()).or_default();

        match membership_change {
            MembershipChange::Left
            | MembershipChange::Banned
            | MembershipChange::Kicked
            | MembershipChange::KickedAndBanned => {
                if let Some(name) = &displayname {
                    if let Some(ids) = self.display_name_to_user_id.get_mut(name) {
                        if let Some(index) = ids.iter().position(|id| id == &user_id) {
                            ids.remove(index);
                        }
                    }
                }
                member.display_user = false;
            }
            MembershipChange::Joined | MembershipChange::Unbanned => {
                if let Some(name) = member.display_name() {
                    let ids = self
                        .display_name_to_user_id
                        .entry(name.to_string())
                        .or_default();

                    ids.push(user_id);
                }
                member.avatar_url = avatar_url;
                member.display_name = displayname;
                member.display_user = true;
            }
            MembershipChange::ProfileChanged {
                displayname_changed,
                avatar_url_changed,
            } => {
                if displayname_changed {
                    if let Some(name) = &prev_displayname {
                        if let Some(ids) = self.display_name_to_user_id.get_mut(name) {
                            if let Some(index) = ids.iter().position(|id| id == &user_id) {
                                ids.remove(index);
                            }
                        }
                    }
                    if let Some(name) = &displayname {
                        if let Some(ids) = self.display_name_to_user_id.get_mut(name) {
                            ids.push(user_id);
                        }
                    }
                    member.display_name = displayname;
                }
                if avatar_url_changed {
                    member.avatar_url = avatar_url;
                }
            }
            _ => {}
        }
    }
}
