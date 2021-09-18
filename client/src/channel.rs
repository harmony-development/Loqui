use crate::role::RolePerms;

use super::message::Messages;
use ahash::RandomState;
use harmony_rust_sdk::api::chat::{permission::has_permission, Permission};
use indexmap::IndexMap;
use smol_str::SmolStr;

pub type Channels = IndexMap<u64, Channel, RandomState>;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: SmolStr,
    pub is_category: bool,
    pub messages: Messages,
    pub last_known_message_id: u64,
    pub looking_at_message: usize,
    pub loading_messages_history: bool,
    pub reached_top: bool,
    pub perms: Vec<Permission>,
    pub role_perms: RolePerms,
    pub has_unread: bool,
    pub looking_at_channel: bool,
    pub init_fetching: bool,
    pub uploading_files: Vec<String>,
}

impl Channel {
    pub fn has_perm(&self, query: &str) -> bool {
        has_permission(self.perms.iter().map(|p| (p.matches.as_str(), p.ok)), query).unwrap_or(false)
    }
}
