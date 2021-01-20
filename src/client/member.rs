use ahash::AHashMap;
use harmony_rust_sdk::{api::harmonytypes::UserStatus, client::api::rest::FileId};
use std::collections::hash_map::{Keys, Values};

#[derive(Debug, Clone)]
pub struct Member {
    pub avatar_url: Option<FileId>,
    pub username: String,
    pub display_user: bool,
    pub typing_in_channel: Option<u64>,
    pub status: UserStatus,
}

impl Default for Member {
    fn default() -> Self {
        Self {
            avatar_url: None,
            username: String::default(),
            display_user: true,
            typing_in_channel: None,
            status: UserStatus::Offline,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Members {
    members: AHashMap<u64, Member>,
}

impl Members {
    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() < 1
    }

    /// Get all members in this room.
    pub fn members(&self) -> &AHashMap<u64, Member> {
        &self.members
    }

    pub fn member_ids(&self) -> Keys<u64, Member> {
        self.members.keys()
    }

    pub fn member_datas(&self) -> Values<u64, Member> {
        self.members.values()
    }

    pub fn get_member(&self, user_id: &u64) -> Option<&Member> {
        self.members.get(user_id)
    }

    pub fn get_member_mut(&mut self, user_id: &u64) -> Option<&mut Member> {
        self.members.get_mut(user_id)
    }

    pub fn typing_members(&self, channel_id: u64) -> Vec<u64> {
        self.members
            .iter()
            .flat_map(|(user_id, member)| {
                if member.typing_in_channel == Some(channel_id) {
                    Some(*user_id)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_user_display_name(&self, user_id: &u64) -> String {
        self.members
            .get(user_id)
            .map_or_else(|| user_id.to_string(), |member| member.username.clone())
    }

    pub fn update_member(&mut self, member: Member, user_id: u64) {
        self.members.insert(user_id, member);
    }
}
