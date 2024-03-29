use indexmap::IndexSet;

use crate::role::RolePerms;

use super::message::Messages;
use harmony_rust_sdk::api::chat::{permission::has_permission, Permission};
use smol_str::SmolStr;

#[derive(Default, Debug, Clone)]
pub struct Channel {
    pub name: SmolStr,
    pub is_category: bool,
    pub messages: Messages,
    pub pinned_messages: IndexSet<u64>,
    pub reached_top: bool,
    pub perms: Vec<Permission>,
    pub role_perms: RolePerms,
    pub fetched_msgs_pins: bool,
}

impl Channel {
    pub fn has_perm(&self, query: &str) -> bool {
        has_permission(self.perms.iter().map(|p| (p.matches.as_str(), p.ok)), query).unwrap_or(false)
    }
}
