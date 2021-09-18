use ahash::AHashMap;
use harmony_rust_sdk::{
    api::chat::{permission::has_permission, Permission, Place},
    client::api::rest::FileId,
};

use crate::{
    role::{Role, RolePerms, Roles},
    IndexMap,
};

use super::channel::Channels;

pub type Guilds = IndexMap<u64, Guild>;

#[derive(Debug, Clone, Default)]
pub struct Guild {
    pub name: String,
    pub picture: Option<FileId>,
    pub channels: Channels,
    pub roles: Roles,
    pub role_perms: RolePerms,
    pub members: AHashMap<u64, Vec<u64>>,
    pub homeserver: String,
    pub perms: Vec<Permission>,
    pub init_fetching: bool,
}

impl Guild {
    pub fn has_perm(&self, query: &str) -> bool {
        has_permission(self.perms.iter().map(|p| (p.matches.as_str(), p.ok)), query).unwrap_or(false)
    }

    pub fn update_channel_order(&mut self, pos: impl Into<Place>, channel_id: u64) {
        update_order(&mut self.channels, pos, channel_id)
    }

    pub fn update_role_order(&mut self, pos: impl Into<Place>, role_id: u64) {
        update_order(&mut self.roles, pos, role_id)
    }

    pub fn highest_role_for_member(&self, user_id: u64) -> Option<(&u64, &Role)> {
        self.members
            .get(&user_id)
            .and_then(|role_ids| self.roles.iter().find(|(id, role)| role.hoist && role_ids.contains(id)))
    }
}

fn update_order<V, P: Into<Place>>(map: &mut IndexMap<u64, V>, pos: P, id: u64) {
    let place = pos.into();

    if let Some(item_pos) = map.get_index_of(&id) {
        let prev_pos = place.after().and_then(|previous_id| map.get_index_of(&previous_id));
        let next_pos = place.before().and_then(|next_id| map.get_index_of(&next_id));

        if let Some(pos) = prev_pos {
            let pos = pos + 1;
            if pos != item_pos && pos < map.len() {
                map.swap_indices(pos, item_pos);
            }
        } else if let Some(pos) = next_pos {
            if pos != 0 {
                map.swap_indices(pos - 1, item_pos);
            } else {
                let (k, v) = map.pop().unwrap();
                map.reverse();
                map.insert(k, v);
                map.reverse();
            }
        }
    }
}
