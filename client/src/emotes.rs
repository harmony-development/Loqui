use ahash::AHashMap;
use smol_str::SmolStr;

#[derive(Debug, Clone, Default)]
pub struct EmotePack {
    pub pack_owner: u64,
    pub pack_name: SmolStr,
    pub emotes: AHashMap<SmolStr, SmolStr>,
}
