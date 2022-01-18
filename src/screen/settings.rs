use std::ops::Not;

use client::content;
use eframe::egui::RichText;

use crate::widgets::{seperated_collapsing, view_egui_settings, Avatar};

use super::prelude::*;

pub struct Screen {
    user_name_edit_text: String,
    uploading_user_pic: AtomBool,
}

impl Screen {
    pub fn new(state: &State) -> Self {
        Self {
            user_name_edit_text: state
                .client()
                .this_user(&state.cache)
                .map_or_else(String::new, |m| m.username.to_string()),
            uploading_user_pic: AtomBool::new(false),
        }
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
                    seperated_collapsing(ui, "app", false, |_ui| {});
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
