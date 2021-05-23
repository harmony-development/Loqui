use super::message::Messages;
use indexmap::IndexMap;

pub type Channels = IndexMap<u64, Channel>;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub is_category: bool,
    pub messages: Messages,
    pub looking_at_message: usize,
    pub loading_messages_history: bool,
    pub reached_top: bool,
}
