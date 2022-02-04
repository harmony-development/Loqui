use ahash::AHashMap;
use harmony_rust_sdk::{
    api::{
        chat::{permission::has_permission, Invite, Permission},
        harmonytypes::{item_position::Position, ItemPosition},
    },
    client::api::rest::FileId,
};
use smol_str::SmolStr;

use crate::role::{Role, RolePerms, Roles};

#[derive(Debug, Clone, Default)]
pub struct Guild {
    pub name: SmolStr,
    pub owners: Vec<u64>,
    pub picture: Option<FileId>,
    pub channels: Vec<u64>,
    pub roles: Roles,
    pub role_perms: RolePerms,
    pub members: AHashMap<u64, Vec<u64>>,
    pub homeserver: SmolStr,
    pub perms: Vec<Permission>,
    pub invites: AHashMap<String, Invite>,
    pub fetched: bool,
    pub fetched_invites: bool,
}

impl Guild {
    pub fn has_perm(&self, query: &str) -> bool {
        has_permission(self.perms.iter().map(|p| (p.matches.as_str(), p.ok)), query).unwrap_or(false)
    }

    pub fn update_channel_order(&mut self, position: ItemPosition, id: u64) {
        let ordering = &mut self.channels;

        let maybe_ord_index = |id: u64| ordering.iter().position(|oid| id.eq(oid));
        let maybe_replace_with = |ordering: &mut Vec<u64>, index| {
            ordering.insert(index, 0);
            if let Some(channel_index) = ordering.iter().position(|oid| id.eq(oid)) {
                ordering.remove(channel_index);
            }
            *ordering.iter_mut().find(|oid| 0.eq(*oid)).unwrap() = id;
        };

        let item_id = position.item_id;
        match position.position() {
            Position::After => {
                if let Some(index) = maybe_ord_index(item_id) {
                    maybe_replace_with(ordering, index.saturating_add(1));
                }
            }
            Position::BeforeUnspecified => {
                if let Some(index) = maybe_ord_index(item_id) {
                    maybe_replace_with(ordering, index);
                }
            }
        }
    }

    pub fn update_role_order(&mut self, position: ItemPosition, role_id: u64) {
        let map = &mut self.roles;
        if let (Some(item_pos), Some(pos)) = (map.get_index_of(&role_id), map.get_index_of(&position.item_id)) {
            match position.position() {
                Position::BeforeUnspecified => {
                    let pos = pos + 1;
                    if pos != item_pos && pos < map.len() {
                        map.swap_indices(pos, item_pos);
                    }
                }
                Position::After => {
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
    }

    pub fn highest_role_for_member(&self, user_id: u64) -> Option<(&u64, &Role)> {
        self.members
            .get(&user_id)
            .and_then(|role_ids| self.roles.iter().find(|(id, _)| role_ids.contains(id)))
    }
}
