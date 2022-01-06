use eframe::egui::CollapsingHeader;

use crate::widgets::{seperated_collapsing, view_egui_settings};

use super::prelude::*;

#[derive(Default)]
pub struct Screen {}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame, app: &mut State) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.group(|ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    seperated_collapsing(ui, "app", false, |ui| {});
                    seperated_collapsing(ui, "profile", false, |ui| {});
                    seperated_collapsing(ui, "egui settings (advanced)", false, |ui| {
                        view_egui_settings(ctx, ui);
                    });
                });
            });
        });
    }
}
