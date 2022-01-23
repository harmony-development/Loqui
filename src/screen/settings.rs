use std::ops::Not;

use client::content;
use eframe::egui::RichText;

use crate::{
    config::BgImage,
    widgets::{seperated_collapsing, view_egui_settings, Avatar},
};

use super::prelude::*;

pub struct Screen {
    user_name_edit_text: String,
    uploading_user_pic: AtomBool,
    is_saving_config: AtomBool,
}

impl Screen {
    pub fn new(state: &State) -> Self {
        Self {
            user_name_edit_text: state
                .client()
                .this_user(&state.cache)
                .map_or_else(String::new, |m| m.username.to_string()),
            uploading_user_pic: AtomBool::new(false),
            is_saving_config: AtomBool::new(false),
        }
    }

    fn view_app(&mut self, state: &mut State, ui: &mut Ui) {
        let is_saving = self.is_saving_config.get();

        ui.add_enabled_ui(is_saving.not(), |ui| {
            let save_resp = ui
                .horizontal(|ui| {
                    let resp = ui.button("save");
                    if is_saving {
                        ui.add(egui::Spinner::new());
                    }
                    resp
                })
                .inner;
            if save_resp.clicked() {
                let conf = state.config.clone();
                let is_saving_config = self.is_saving_config.clone();
                spawn_client_fut!(state, |client| {
                    is_saving_config.set(true);
                    let res = conf.store(&client).await;
                    is_saving_config.set(false);
                    res
                });
            }

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
                    seperated_collapsing(ui, "app", false, |ui| {
                        self.view_app(state, ui);
                    });
                    seperated_collapsing(ui, "profile", false, |ui| {
                        self.view_profile(state, ui);
                    });
                    seperated_collapsing(ui, "egui settings (advanced)", false, |ui| {
                        view_egui_settings(ctx, ui);
                    });
                });
            });
        });
    }
}
