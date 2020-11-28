use ruma::{api::exports::http::Uri, identifiers::UserId};
use std::time::Instant;

pub type Members = ahash::AHashMap<UserId, Member>;

pub struct Member {
    avatar_url: Option<Uri>,
    display_name: Option<String>,
    display_user: bool,
    typing_received: Option<Instant>,
}

impl Default for Member {
    fn default() -> Self {
        Self {
            avatar_url: None,
            display_name: None,
            display_user: true,
            typing_received: None,
        }
    }
}

impl Member {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn avatar_url(&self) -> Option<&Uri> {
        self.avatar_url.as_ref()
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn display(&self) -> bool {
        self.display_user
    }

    pub fn is_typing(&self) -> bool {
        self.typing_received.is_some()
    }

    pub fn set_avatar_url(&mut self, new_avatar_url: Option<Uri>) {
        self.avatar_url = new_avatar_url;
    }

    pub fn set_display_name(&mut self, new_display_name: Option<String>) {
        self.display_name = new_display_name;
    }

    pub fn set_display(&mut self, new_display: bool) {
        self.display_user = new_display;
    }

    pub fn set_typing(&mut self, new_typing_recieved: Option<Instant>) {
        self.typing_received = new_typing_recieved;
    }
}
