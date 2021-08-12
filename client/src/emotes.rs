use crate::IndexMap;

#[derive(Debug, Clone)]
pub struct EmotePack {
    pub pack_owner: u64,
    pub pack_name: String,
    pub emotes: IndexMap<String, String>,
}