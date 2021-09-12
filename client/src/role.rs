use ahash::AHashMap;
use harmony_rust_sdk::api::chat::{color, Permission, Role as HarmonyRole};
use smol_str::SmolStr;

use crate::IndexMap;

pub type Roles = IndexMap<u64, Role>;
pub type RolePerms = AHashMap<u64, Vec<Permission>>;

#[derive(Debug, Default, Clone)]
pub struct Role {
    pub name: SmolStr,
    pub color: [u8; 3],
    pub hoist: bool,
    pub pingable: bool,
}

impl From<Role> for HarmonyRole {
    fn from(r: Role) -> Self {
        HarmonyRole {
            name: r.name.into(),
            hoist: r.hoist,
            pingable: r.pingable,
            color: color::encode_rgb(r.color),
        }
    }
}

impl From<HarmonyRole> for Role {
    fn from(role: HarmonyRole) -> Self {
        Self {
            name: role.name.into(),
            hoist: role.hoist,
            pingable: role.pingable,
            color: color::decode_rgb(role.color),
        }
    }
}
