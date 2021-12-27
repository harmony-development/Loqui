use super::prelude::*;

pub struct Screen {
    guild_id: u64,
}

impl Screen {
    pub fn new(guild_id: u64) -> Self {
        Self { guild_id }
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame, app: &mut State) {
        todo!()
    }
}
