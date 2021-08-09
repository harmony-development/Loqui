use ahash::AHashSet;
use harmony_rust_sdk::client::api::rest::FileId;

use crate::IndexMap;

use super::channel::Channels;

pub type Guilds = IndexMap<u64, Guild>;

#[derive(Debug, Clone, Default)]
pub struct Guild {
    pub name: String,
    pub picture: Option<FileId>,
    pub channels: Channels,
    pub members: AHashSet<u64>,
    pub homeserver: String,
    pub user_perms: GuildPerms,
    pub init_fetching: bool,
}

impl Guild {
    pub fn update_channel_order(&mut self, previous_id: u64, next_id: u64, channel_id: u64) {
        if let Some(chan_pos) = self.channels.get_index_of(&channel_id) {
            let prev_pos = self.channels.get_index_of(&previous_id);
            let next_pos = self.channels.get_index_of(&next_id);

            if let Some(pos) = prev_pos {
                let pos = pos + 1;
                if pos != chan_pos && pos < self.channels.len() {
                    self.channels.swap_indices(pos, chan_pos);
                }
            } else if let Some(pos) = next_pos {
                if pos != 0 {
                    self.channels.swap_indices(pos - 1, chan_pos);
                } else {
                    let (k, v) = self.channels.pop().unwrap();
                    self.channels.reverse();
                    self.channels.insert(k, v);
                    self.channels.reverse();
                }
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct GuildPerms {
    pub change_info: bool,
}
