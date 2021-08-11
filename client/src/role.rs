use ahash::AHashMap;
use harmony_rust_sdk::api::chat::{Permission, Role as HarmonyRole};
use smol_str::SmolStr;

use crate::{color, IndexMap};

pub type Roles = IndexMap<u64, Role>;
pub type RolePerms = AHashMap<u64, Vec<Permission>>;

#[derive(Debug, Default, Clone)]
pub struct Role {
    pub name: SmolStr,
    pub color: (u8, u8, u8),
    pub hoist: bool,
    pub pingable: bool,
}

impl Role {
    pub fn to_harmony_role(self, role_id: u64) -> HarmonyRole {
        HarmonyRole {
            role_id,
            name: self.name.into(),
            hoist: self.hoist,
            pingable: self.pingable,
            color: color::encode_rgb(self.color) as i32,
        }
    }
}

impl From<HarmonyRole> for Role {
    fn from(role: HarmonyRole) -> Self {
        Self {
            name: role.name.into(),
            hoist: role.hoist,
            pingable: role.pingable,
            color: color::decode_rgb(role.color as i64),
        }
    }
}
