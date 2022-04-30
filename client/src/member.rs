use instant::Instant;

use harmony_rust_sdk::api::profile::{AccountKind, UserStatus};
use smol_str::SmolStr;

#[derive(Debug, Clone)]
pub struct Member {
    pub avatar_url: Option<String>,
    pub username: SmolStr,
    pub display_user: bool,
    pub typing_in_channel: Option<(Option<u64>, u64, Instant)>,
    pub status: UserStatus,
    pub fetched: bool,
    pub kind: AccountKind,
}

impl Default for Member {
    fn default() -> Self {
        Self {
            avatar_url: None,
            username: SmolStr::default(),
            display_user: true,
            typing_in_channel: None,
            status: UserStatus::default(),
            fetched: false,
            kind: AccountKind::FullUnspecified,
        }
    }
}
