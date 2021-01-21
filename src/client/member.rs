use ahash::AHashMap;
use harmony_rust_sdk::{api::harmonytypes::UserStatus, client::api::rest::FileId};

pub type Members = AHashMap<u64, Member>;

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
