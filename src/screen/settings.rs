use std::ops::Not;

use client::{
    content,
    harmony_rust_sdk::api::profile::{profile_override::Reason, OverrideTag, ProfileOverride},
    member::Member,
};
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

    fn view_general_profile(&mut self, state: &State, ui: &mut Ui, member: &Member) {
        ui.horizontal_top(|ui| {
            if self.uploading_user_pic.get().not() {
                let avatar = Avatar::new(member.avatar_url.as_deref(), member.username.as_str(), state).size(64.0);
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
    }

    fn view_mention_keywords(&mut self, state: &mut State, ui: &mut Ui) {
        ui.collapsing("mention keywords", |ui| {
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
        });
    }

    fn view_override_profiles(&mut self, state: &mut State, ui: &mut Ui) {
        ui.collapsing("override profiles", |ui| {
            if state.config.overrides.overrides.is_empty() {
                ui.colored_label(Color32::YELLOW, "no profiles");
            } else {
                let mut delete_index = None;
                for (index, profile) in state.config.overrides.overrides.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            if let Some(username) = &mut profile.username {
                                ui.add(
                                    egui::TextEdit::singleline(username)
                                        .desired_width(100.0)
                                        .hint_text("enter username..."),
                                )
                                .context_menu_styled(|ui| {
                                    if ui.button("reset").clicked() {
                                        profile.username = None;
                                    }
                                });
                            } else if ui.button("set username").clicked() {
                                profile.username = Some(String::new());
                            }

                            if let Some(avatar) = &mut profile.avatar {
                                ui.add(
                                    egui::TextEdit::singleline(avatar)
                                        .desired_width(100.0)
                                        .hint_text("enter avatar..."),
                                )
                                .context_menu_styled(|ui| {
                                    if ui.button("reset").clicked() {
                                        profile.avatar = None;
                                    }
                                });
                            } else if ui.button("set avatar").clicked() {
                                profile.avatar = Some(String::new());
                            }

                            if let Some(reason) = &mut profile.reason {
                                let set_custom_reason = match reason {
                                    Reason::SystemPlurality(_) => ui.button("set custom reason").clicked(),
                                    Reason::UserDefined(custom) => {
                                        ui.add(
                                            egui::TextEdit::singleline(custom)
                                                .desired_width(100.0)
                                                .hint_text("enter reason..."),
                                        )
                                        .context_menu_styled(|ui| {
                                            if ui.button("reset").clicked() {
                                                *reason = Reason::SystemPlurality(Default::default());
                                            }
                                        });
                                        false
                                    }
                                };

                                if set_custom_reason {
                                    *reason = Reason::UserDefined(String::new());
                                }
                            }

                            if ui.button("delete").clicked() {
                                delete_index = Some(index);
                            }
                        });
                        ui.separator();
                        if profile.tags.is_empty() {
                            ui.colored_label(Color32::YELLOW, "no tags");
                        } else {
                            let mut delete_index = None;
                            for (index, tag) in profile.tags.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut tag.before)
                                            .desired_width(50.0)
                                            .hint_text("before"),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut tag.after)
                                            .desired_width(50.0)
                                            .hint_text("after"),
                                    );
                                    if ui.button("delete").clicked() {
                                        delete_index = Some(index);
                                    }
                                });
                            }
                            if let Some(index) = delete_index {
                                profile.tags.remove(index);
                            }
                        }
                        if ui.button("new tag").clicked() {
                            profile.tags.push(OverrideTag::default());
                        }
                    });
                }
                if let Some(index) = delete_index {
                    state.config.overrides.overrides.remove(index);
                }
            }
            if ui.button("create profile").clicked() {
                state.config.overrides.overrides.push(ProfileOverride {
                    reason: Some(Reason::SystemPlurality(Default::default())),
                    ..Default::default()
                });
            }

            ui.collapsing("guild profiles", |ui| {
                for (id, guild) in state.cache.get_guilds() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(truncate_string(guild.name.as_str(), 10).into_owned());

                        let latching_to_channel = state.config.latch_to_channel_guilds.contains(&id);
                        let last_used_text = latching_to_channel
                            .then(|| "last used channel")
                            .unwrap_or("last used guild");
                        let text = state
                            .config
                            .default_profiles_for_guilds
                            .get(&id)
                            .map_or(last_used_text, |s| s.as_str());
                        let text = format!("▼ {}", text);
                        ui.menu_button(text, |ui| {
                            if ui.button("clear").clicked() {
                                state.config.latch_to_channel_guilds.remove(&id);
                                state.config.default_profiles_for_guilds.remove(&id);
                                ui.close_menu();
                            }

                            if ui.button("last used guild").clicked() {
                                state.config.latch_to_channel_guilds.remove(&id);
                                state.config.default_profiles_for_guilds.remove(&id);
                                ui.close_menu();
                            }

                            if ui.button("last used channel").clicked() {
                                state.config.latch_to_channel_guilds.insert(id);
                                state.config.default_profiles_for_guilds.remove(&id);
                                ui.close_menu();
                            }

                            let profiles = state
                                .config
                                .overrides
                                .overrides
                                .iter()
                                .filter_map(|p| p.username.as_deref());
                            for name in profiles {
                                if ui.button(format!("use {}", name)).clicked() {
                                    state.config.default_profiles_for_guilds.insert(id, name.to_string());
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                }
            });
        });
    }

    // this assumes that the parent ui is vertical layout
    fn view_profile(&mut self, state: &mut State, ui: &mut Ui) {
        let Some(member) = state.client.as_ref().and_then(|c| c.this_user(&state.cache)) else {
            ui.label("loading...");
            return;
        };

        self.view_general_profile(state, ui, member);
        self.view_mention_keywords(state, ui);
        self.view_override_profiles(state, ui);
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::Context, _frame: &epi::Frame, state: &mut State) {
        self.handle_futures(state);

        egui::CentralPanel::default().show(ctx, |ui| {
            // TODO find a way to remove collapsing header lines and icons?
            if ui.is_mobile() {
                ui.spacing_mut().indent = 0.0;
            }

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
