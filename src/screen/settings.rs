use std::ops::Not;

use client::content;
use eframe::egui::RichText;

use crate::{
    config::BgImage,
    widgets::{view_egui_settings, Avatar, SeperatedCollapsingHeader},
};

use super::prelude::*;

pub struct Screen {
    user_name_edit_text: String,
    uploading_user_pic: AtomBool,
    scale_factor: f32,
}

impl Screen {
    pub fn new(ctx: &egui::Context, state: &State) -> Self {
        Self {
            user_name_edit_text: state
                .client()
                .this_user(&state.cache)
                .map_or_else(String::new, |m| m.username.to_string()),
            uploading_user_pic: AtomBool::new(false),
            scale_factor: ctx.pixels_per_point(),
        }
    }

    fn view_app(&mut self, state: &mut State, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = 90.0;

            ui.add(
                egui::Slider::new(&mut self.scale_factor, 0.5..=5.0)
                    .logarithmic(true)
                    .clamp_to_range(true)
                    .text("Scale"),
            )
            .on_hover_text("Physical pixels per point.");

            if let Some(native_pixels_per_point) = state
                .integration_info
                .as_ref()
                .and_then(|info| info.native_pixels_per_point)
            {
                let enabled = self.scale_factor != native_pixels_per_point;
                if ui
                    .add_enabled(enabled, egui::Button::new("Reset"))
                    .on_hover_text(format!("Reset scale to native value ({:.1})", native_pixels_per_point))
                    .clicked()
                {
                    self.scale_factor = native_pixels_per_point;
                }
            }

            if ui.ctx().is_using_pointer().not() {
                state.local_config.scale_factor = self.scale_factor;
                ui.ctx().set_pixels_per_point(state.local_config.scale_factor);
            }
        });

        ui.horizontal(|ui| {
            ui.label("background image:");

            let chosen_image = match &state.config.bg_image {
                BgImage::External(s) => format!("▼ external: {}", s),
                BgImage::Local(s) => format!("▼ local: {}", s),
                BgImage::Default => "▼ default".to_string(),
                BgImage::None => "▼ none".to_string(),
            };

            ui.menu_button(chosen_image, |ui| {
                if ui.button("choose file").clicked() {
                    ui.close_menu();
                }

                if ui.button("none").clicked() {
                    state.config.bg_image = BgImage::None;
                    ui.close_menu();
                }

                if ui.button("default").clicked() {
                    state.config.bg_image = BgImage::Default;
                    ui.close_menu();
                }
            });
        });
    }

    fn view_profile(&mut self, state: &mut State, ui: &mut Ui) {
        let Some(member) = state.client().this_user(&state.cache) else {
            ui.label("loading...");
            return;
        };

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("username").small());
            ui.text_edit_singleline(&mut self.user_name_edit_text);
            if ui.add(egui::Button::new("edit").small()).clicked() {
                let new_name = self.user_name_edit_text.clone();
                spawn_client_fut!(state, |client| client.update_profile(Some(new_name), None, None).await);
            }
        });

        if self.uploading_user_pic.get().not() {
            let avatar = Avatar::new(member.avatar_url.as_ref(), member.username.as_str(), state).size(64.0);
            let avatar_but = ui.add(avatar).on_hover_text("set picture");
            if avatar_but.clicked() {
                let uploading_user_pic = self.uploading_user_pic.clone();
                spawn_client_fut!(state, |client| {
                    let maybe_file = rfd::AsyncFileDialog::new().pick_file().await;
                    if let Some(file) = maybe_file {
                        uploading_user_pic.set(true);
                        let name = file.file_name();
                        let data = file.read().await;
                        let mimetype = content::infer_type_from_bytes(&data);
                        let id = client.upload_file(name, mimetype, data).await?;
                        client.update_profile(None, Some(id), None).await?;
                        uploading_user_pic.set(false);
                    }
                    ClientResult::Ok(())
                });
            }
        } else {
            ui.add(egui::Spinner::new().size(64.0))
                .on_hover_text("uploading avatar");
        }
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::Context, _frame: &epi::Frame, state: &mut State) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.group(|ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    SeperatedCollapsingHeader::new("profile")
                        .with(|h| h.default_open(true))
                        .show(ui, |ui| {
                            self.view_profile(state, ui);
                        });
                    SeperatedCollapsingHeader::new("app")
                        .with(|h| h.default_open(true))
                        .show(ui, |ui| {
                            self.view_app(state, ui);
                        });
                    SeperatedCollapsingHeader::new("egui settings (advanced)").show(ui, |ui| {
                        ui.label("note: these settings are not persisted");
                        view_egui_settings(ctx, ui);
                    });
                });
            });
        });
    }

    fn on_pop(&mut self, _: &egui::Context, _: &epi::Frame, state: &mut State) {
        state.save_config();
    }

    fn on_push(&mut self, _: &egui::Context, _: &epi::Frame, state: &mut State) {
        state.save_config();
    }
}
