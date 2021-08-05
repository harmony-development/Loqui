use super::message::Messages;
use ahash::RandomState;
use indexmap::IndexMap;
use smol_str::SmolStr;

pub type Channels = IndexMap<u64, Channel, RandomState>;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: SmolStr,
    pub is_category: bool,
    pub messages: Messages,
    pub looking_at_message: usize,
    pub loading_messages_history: bool,
    pub reached_top: bool,
    pub user_perms: ChanPerms,
    pub has_unread: bool,
    pub looking_at_channel: bool,
}

#[derive(Debug, Clone)]
pub struct ChanPerms {
    pub send_msg: bool,
    pub manage_channel: bool,
}
