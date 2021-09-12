use std::time::Instant;

use ahash::AHashMap;
use harmony_rust_sdk::{api::profile::UserStatus, client::api::rest::FileId};
use smol_str::SmolStr;

pub type Members = AHashMap<u64, Member>;

#[derive(Debug, Clone)]
pub struct Member {
    pub avatar_url: Option<FileId>,
    pub username: SmolStr,
    pub display_user: bool,
    pub typing_in_channel: Option<(u64, u64, Instant)>,
    pub status: UserStatus,
    pub is_bot: bool,
}

impl Default for Member {
    fn default() -> Self {
        Self {
            avatar_url: None,
            username: SmolStr::default(),
            display_user: true,
            typing_in_channel: None,
            status: UserStatus::OfflineUnspecified,
            is_bot: false,
        }
    }
}
