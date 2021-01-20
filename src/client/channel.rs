use super::message::Messages;
use ahash::AHashMap;

pub type Channels = AHashMap<u64, Channel>;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub is_category: bool,
    pub messages: Messages,
    pub looking_at_message: usize,
    pub loading_messages_history: bool,
}
