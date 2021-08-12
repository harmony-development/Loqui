use crate::IndexMap;

#[derive(Debug, Clone, Default)]
pub struct EmotePack {
    pub pack_owner: u64,
    pub pack_name: String,
    pub emotes: IndexMap<String, String>,
}

pub type EmotePacks = IndexMap<u64, EmotePack>;
