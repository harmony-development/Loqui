use crate::role::RolePerms;

use super::message::{Messages, PinnedMessages};
use harmony_rust_sdk::api::chat::{permission::has_permission, Permission};
use indexmap::IndexSet;
use smol_str::SmolStr;

#[derive(Default, Debug, Clone)]
pub struct Channel {
    pub name: SmolStr,
    pub is_category: bool,
    pub messages: Messages,
    pub pinned_messages: PinnedMessages,
    pub reached_top: bool,
    pub perms: Vec<Permission>,
    pub role_perms: RolePerms,
    pub fetched_msgs_pins: bool,
    pub private_channel_data: Option<PrivateChannel>,
}

impl Channel {
    pub fn has_perm(&self, query: &str) -> bool {
        has_permission(self.perms.iter().map(|p| (p.matches.as_str(), p.ok)), query).unwrap_or(false)
    }

    pub fn get_priv_mut(&mut self) -> &mut PrivateChannel {
        if self.private_channel_data.is_none() {
            self.private_channel_data = Some(PrivateChannel::default());
        }
        self.private_channel_data.as_mut().unwrap()
    }

    /// Get the private channel data.
    ///
    /// # Panics
    /// Panics if this is not a private channel.
    pub fn get_priv(&self) -> &PrivateChannel {
        self.private_channel_data
            .as_ref()
            .expect("not private channel -- this is a bug")
    }
}

#[derive(Default, Debug, Clone)]
pub struct PrivateChannel {
    pub server_id: Option<String>,
    pub members: IndexSet<u64, ahash::RandomState>,
    pub owner: u64,
    pub is_dm: bool,
    pub fetched: bool,
}
