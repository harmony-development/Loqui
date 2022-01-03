use eframe::egui::RichText;

use crate::widgets::{seperated_collapsing, view_avatar};

use super::prelude::*;

pub struct Screen {
    guild_id: u64,
}

impl Screen {
    pub fn new(guild_id: u64) -> Self {
        Self { guild_id }
    }

    fn view_general(&mut self, state: &mut State, ui: &mut Ui) {
        guard!(let Some(guild) = state.cache.get_guild(self.guild_id) else { return });
        ui.horizontal(|ui| {
            ui.label(RichText::new(guild.name.as_str()).heading().strong());
            if ui.add(egui::Button::new("edit").small()).clicked() {
                // TODO: edit name
            }
        });
        if view_avatar(ui, state, guild.picture.as_ref(), guild.name.as_str(), 96.0).clicked() {
            // TODO: change avatar
        }
    }

    fn view_invites(&mut self, state: &mut State, ui: &mut Ui) {}

    fn view_roles(&mut self, state: &mut State, ui: &mut Ui) {}

    fn view_members(&mut self, state: &mut State, ui: &mut Ui) {}

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {}
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame, state: &mut State) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.group(|ui| {
                seperated_collapsing(ui, "general", true, |ui| self.view_general(state, ui));
                seperated_collapsing(ui, "invites", false, |ui| self.view_invites(state, ui));
                seperated_collapsing(ui, "roles", false, |ui| self.view_roles(state, ui));
                seperated_collapsing(ui, "members", false, |ui| self.view_members(state, ui));
                seperated_collapsing(ui, "channels", false, |ui| self.view_channels(state, ui));
            });
        });
    }
}
