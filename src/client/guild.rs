use ahash::AHashMap;
use harmony_rust_sdk::client::api::rest::FileId;

use super::{channel::Channels, member::Members};

pub type Guilds = AHashMap<u64, Guild>;

#[derive(Debug, Clone)]
pub struct Guild {
    pub name: String,
    pub picture: Option<FileId>,
    pub channels: Channels,
    pub members: Members,
}
