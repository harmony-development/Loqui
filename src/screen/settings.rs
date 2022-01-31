use std::ops::Not;

use client::content;
use eframe::egui::{CollapsingHeader, Color32, RichText};
use rfd::FileHandle;

use crate::{
    config::BgImage,
    widgets::{view_egui_settings, Avatar},
};

use super::prelude::*;

pub struct Screen {
    user_name_edit_text: String,
    uploading_user_pic: AtomBool,
    scale_factor: f32,
    mention_keyword_edit: String,
    inputting_bg_image_url: bool,
    bg_image_url_text: String,
}

impl Screen {
    pub fn new(ctx: &egui::Context, state: &State) -> Self {
        Self {
            user_name_edit_text: state
                .client
                .as_ref()
                .and_then(|c| c.this_user(&state.cache))
                .map_or_else(String::new, |m| m.username.to_string()),
            uploading_user_pic: AtomBool::new(false),
            scale_factor: ctx.pixels_per_point(),
            mention_keyword_edit: String::new(),
            inputting_bg_image_url: false,
            bg_image_url_text: String::new(),
        }
    }

    #[inline(always)]
    fn handle_futures(&mut self, state: &mut State) {
        #[cfg(not(target_arch = "wasm32"))]
        handle_future!(state, |maybe_file: Option<FileHandle>| {
            if let Some(file) = maybe_file {
                state.local_config.bg_image = BgImage::Local(file.path().to_path_buf());
                state.futures.spawn(state.local_config.bg_image.clone().load());
            }
        });
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
                    .on_hover_text(format!("Reset scale to native value ({native_pixels_per_point:.1})"))
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

            if self.inputting_bg_image_url {
                let edit = ui.text_edit_singleline(&mut self.bg_image_url_text);
                if ui.button("submit").clicked() || edit.did_submit(ui) {
                    state.local_config.bg_image = BgImage::External(self.bg_image_url_text.drain(..).collect());
                    state.futures.spawn(state.local_config.bg_image.clone().load());
                    self.inputting_bg_image_url = false;
                }
            } else {
                let chosen_image = match &state.local_config.bg_image {
                    BgImage::External(s) => format!("▼ external: {s}"),
                    BgImage::Local(s) => format!("▼ local: {s:?}"),
                    BgImage::Default => "▼ default".to_string(),
                    BgImage::None => "▼ none".to_string(),
                };

                ui.menu_button(chosen_image, |ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("choose file").clicked() {
                        state.futures.spawn(rfd::AsyncFileDialog::new().pick_file());
                        ui.close_menu();
                    }

                    if ui.button("enter url").clicked() {
                        self.inputting_bg_image_url = true;
                        ui.close_menu();
                    }

                    if ui.button("none").clicked() {
                        state.local_config.bg_image = BgImage::None;
                        ui.close_menu();
                    }

                    if ui.button("default").clicked() {
                        state.local_config.bg_image = BgImage::Default;
                        ui.close_menu();
                    }
                });
            }
        });
    }

    // this assumes that the parent ui is vertical layout
    fn view_profile(&mut self, state: &mut State, ui: &mut Ui) {
        let Some(member) = state.client.as_ref().and_then(|c| c.this_user(&state.cache)) else {
            ui.label("loading...");
            return;
        };

        ui.horizontal_top(|ui| {
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
            ui.vertical(|ui| {
                ui.label(RichText::new("username").small());
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.user_name_edit_text).desired_width(100.0));
                    if ui.add(egui::Button::new("edit").small()).clicked() {
                        let new_name = self.user_name_edit_text.clone();
                        spawn_client_fut!(state, |client| client.update_profile(Some(new_name), None, None).await);
                    }
                });
            });
        });

        ui.heading("mention keywords");
        ui.horizontal(|ui| {
            let text_edit = ui.add(egui::TextEdit::singleline(&mut self.mention_keyword_edit).desired_width(100.0));
            if ui.button("+ add").clicked() || text_edit.did_submit(ui) {
                let keyword = self.mention_keyword_edit.drain(..).collect();
                state.config.mention_keywords.push(keyword);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let mut delete_word = None;
            for (word_index, word) in state.config.mention_keywords.iter().enumerate() {
                let text = {
                    let mut job = egui::text::LayoutJob::default();
                    job.append(
                        "X ",
                        0.0,
                        egui::TextFormat {
                            color: Color32::RED,
                            ..egui::TextFormat::default()
                        },
                    );
                    job.append(word, 0.0, egui::TextFormat::default());
                    ui.painter().fonts().layout_job(job)
                };
                let but = ui.button(text).on_hover_text("click to delete");
                if but.clicked() {
                    delete_word = Some(word_index);
                }
            }
            if let Some(word_to_delete) = delete_word {
                state.config.mention_keywords.remove(word_to_delete);
            }
        });
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::Context, _frame: &epi::Frame, state: &mut State) {
        self.handle_futures(state);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.group(|ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    CollapsingHeader::new("profile").default_open(true).show(ui, |ui| {
                        self.view_profile(state, ui);
                    });
                    CollapsingHeader::new("app").default_open(true).show(ui, |ui| {
                        self.view_app(state, ui);
                    });
                    CollapsingHeader::new("egui settings (advanced)").show(ui, |ui| {
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
