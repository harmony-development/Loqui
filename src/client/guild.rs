use ahash::{AHashMap, AHashSet};
use harmony_rust_sdk::client::api::rest::FileId;

use super::channel::Channels;

pub type Guilds = AHashMap<u64, Guild>;

#[derive(Debug, Clone, Default)]
pub struct Guild {
    pub name: String,
    pub picture: Option<FileId>,
    pub channels: Channels,
    pub members: AHashSet<u64>,
}
