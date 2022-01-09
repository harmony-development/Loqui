use std::ops::Not;

use client::{channel::Channel, content, guild::Guild, harmony_rust_sdk::api::chat::all_permissions, member::Member};
use eframe::egui::{Color32, RichText};

use crate::widgets::{seperated_collapsing, Avatar};

use super::prelude::*;

pub struct Screen {
    guild_id: u64,
    id_edit_text: String,
    possible_uses_text: String,
    channel_name_edit_text: String,
    guild_name_edit_text: String,
    fetching_invites: AtomBool,
    uploading_guild_pic: AtomBool,
}

impl Screen {
    pub fn new(guild_id: u64, state: &State) -> Self {
        Self {
            guild_id,

            id_edit_text: String::new(),
            possible_uses_text: String::new(),
            channel_name_edit_text: String::new(),
            guild_name_edit_text: state
                .cache
                .get_guild(guild_id)
                .map_or_else(String::new, |g| g.name.to_string()),
            fetching_invites: AtomBool::default(),
            uploading_guild_pic: AtomBool::default(),
        }
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        if guild.has_perm(all_permissions::CHANNELS_MANAGE_CREATE) {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.channel_name_edit_text).hint_text("channel name"));

                if ui.button("create channel").clicked() {
                    let name = self.channel_name_edit_text.drain(..).collect::<String>();
                    spawn_client_fut!(state, |client| {
                        client.create_channel(guild_id, name).await?;
                    });
                }
            });

            ui.separator();
        }

        for channel_id in guild.channels.iter().copied() {
            guard!(let Some(channel) = state.cache.get_channel(guild_id, channel_id) else { continue });
            self.view_channel(state, ui, channel_id, channel, guild);
        }
    }

    fn view_channel(&mut self, state: &State, ui: &mut Ui, channel_id: u64, channel: &Channel, guild: &Guild) {
        let id_string = channel_id.to_string();
        let resp = ui
            .group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&id_string).small());
                    ui.label(channel.name.as_str());
                });
            })
            .response;
        resp.on_hover_text("right click to manage").context_menu_styled(|ui| {
            if ui.button("copy id").clicked() {
                ui.output().copied_text = id_string;
                ui.close_menu();
            }
            if ui.button("copy name").clicked() {
                ui.output().copied_text = channel.name.to_string();
                ui.close_menu();
            }
            if guild.has_perm(all_permissions::CHANNELS_MANAGE_DELETE) && ui.button(dangerous_text("delete")).clicked()
            {
                let guild_id = self.guild_id;
                spawn_client_fut!(state, |client| {
                    client.delete_channel(guild_id, channel_id).await?;
                });
                ui.close_menu();
            }
            if guild.has_perm(all_permissions::CHANNELS_MANAGE_CHANGE_INFORMATION) && ui.button("edit").clicked() {
                // TODO
                ui.close_menu();
            }
        });
    }

    fn view_general(&mut self, state: &mut State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        if guild.has_perm(all_permissions::GUILD_MANAGE_CHANGE_INFORMATION) {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("guild name").small());
                ui.text_edit_singleline(&mut self.guild_name_edit_text);
                if ui.add(egui::Button::new("edit").small()).clicked() {
                    let new_name = self.guild_name_edit_text.clone();
                    spawn_client_fut!(state, |client| {
                        client.edit_guild(guild_id, Some(new_name), None).await?;
                    });
                }
            });
        } else {
            ui.label(guild.name.as_str());
        }

        if self.uploading_guild_pic.get().not() {
            let avatar_but = ui
                .add(Avatar::new(guild.picture.as_ref(), guild.name.as_str(), state).size(64.0))
                .on_hover_text("set picture");
            if avatar_but.clicked() {
                let uploading_guild_pic = self.uploading_guild_pic.clone();
                spawn_client_fut!(state, |client| {
                    let maybe_file = rfd::AsyncFileDialog::new().pick_file().await;
                    if let Some(file) = maybe_file {
                        uploading_guild_pic.set(true);
                        let name = file.file_name();
                        let data = file.read().await;
                        let mimetype = content::infer_type_from_bytes(&data);
                        let id = client.upload_file(name, mimetype, data).await?;
                        client.edit_guild(guild_id, None, Some(id)).await?;
                        uploading_guild_pic.set(false);
                    }
                });
            }
        } else {
            ui.add(egui::Spinner::new().size(64.0))
                .on_hover_text("uploading avatar");
        }
    }

    fn view_invites(&mut self, state: &mut State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        if guild.has_perm(all_permissions::INVITES_VIEW).not() {
            ui.colored_label(Color32::YELLOW, "no permission to manage invites");
            return;
        }

        if guild.has_perm(all_permissions::INVITES_MANAGE_CREATE) {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.id_edit_text).hint_text("invite name"));
                ui.add_sized(
                    [40.0, 12.0],
                    egui::TextEdit::singleline(&mut self.possible_uses_text).hint_text("uses"),
                );

                if ui.button("create invite").clicked() {
                    if let Ok(uses) = self.possible_uses_text.parse::<u64>() {
                        let name = self.id_edit_text.drain(..).collect::<String>();
                        spawn_client_fut!(state, |client| {
                            client.create_invite(guild_id, name, uses as u32).await?;
                        });
                    }
                }
            });

            ui.separator();
        }

        if guild.fetched_invites {
            if guild.invites.is_empty().not() {
                for (id, invite) in guild.invites.iter() {
                    let resp = ui
                        .group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(id);
                                ui.label(format!("uses {} / {}", invite.use_count, invite.possible_uses));
                            });
                        })
                        .response;
                    resp.on_hover_text("right click to manage").context_menu_styled(|ui| {
                        if ui.button("copy invite").clicked() {
                            let homeserver = guild
                                .homeserver
                                .is_empty()
                                .then(|| state.client().inner().homeserver_url().authority().unwrap().as_str())
                                .unwrap_or_else(|| guild.homeserver.as_str());
                            ui.output().copied_text = format!("hmc://{}/{}", homeserver, id);
                            ui.close_menu();
                        }
                        if guild.has_perm(all_permissions::INVITES_MANAGE_DELETE)
                            && ui.button(dangerous_text("delete")).clicked()
                        {
                            let name = id.clone();
                            spawn_client_fut!(state, |client| {
                                client.delete_invite(guild_id, name).await?;
                            });
                            ui.close_menu();
                        }
                    });
                }
            } else {
                ui.colored_label(Color32::YELLOW, "no invites");
            }
        } else if self.fetching_invites.get().not() {
            self.fetching_invites.set(true);
            let fetching_invites = self.fetching_invites.clone();
            spawn_evs!(state, |events, client| {
                let res = client.fetch_invites(guild_id, events).await;
                fetching_invites.set(false);
                res?;
            });
        } else {
            ui.add(egui::Spinner::new().size(32.0))
                .on_hover_text("fetching invites");
        }
    }

    fn view_members(&mut self, state: &State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        let sorted_members = sort_members(state, guild);
        let chunk_size = (ui.available_width() / 300.0).ceil() as usize;

        for chunk in sorted_members.chunks(chunk_size) {
            ui.columns(chunk_size, |ui| {
                for ((id, member), ui) in chunk.iter().zip(ui) {
                    self.view_member(state, ui, guild_id, **id, member, guild);
                }
            });
        }
    }

    fn view_member(&mut self, state: &State, ui: &mut Ui, guild_id: u64, user_id: u64, member: &Member, guild: &Guild) {
        let id_string = user_id.to_string();
        let resp = ui
            .group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&id_string).small());
                    ui.label(member.username.as_str());
                });
            })
            .response;
        resp.on_hover_text("right click to manage").context_menu_styled(|ui| {
            if ui.button("copy id").clicked() {
                ui.output().copied_text = id_string;
                ui.close_menu();
            }
            if ui.button("copy username").clicked() {
                ui.output().copied_text = member.username.to_string();
                ui.close_menu();
            }
            if guild.has_perm(all_permissions::USER_MANAGE_BAN) && ui.button(dangerous_text("ban")).clicked() {
                spawn_client_fut!(state, |client| {
                    client.ban_member(guild_id, user_id).await?;
                });
                ui.close_menu();
            }
            if guild.has_perm(all_permissions::USER_MANAGE_KICK) && ui.button(dangerous_text("kick")).clicked() {
                spawn_client_fut!(state, |client| {
                    client.kick_member(guild_id, user_id).await?;
                });
                ui.close_menu();
            }
            if guild.has_perm(all_permissions::ROLES_USER_MANAGE) && ui.button("manage roles").clicked() {
                // TODO
                ui.close_menu();
            }
        });
    }

    #[allow(unused_variables)]
    fn view_roles(&mut self, state: &mut State, ui: &mut Ui) {}
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, _: &epi::Frame, state: &mut State) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.group(|ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    seperated_collapsing(ui, "general", true, |ui| self.view_general(state, ui));
                    seperated_collapsing(ui, "invites", false, |ui| self.view_invites(state, ui));
                    seperated_collapsing(ui, "roles", false, |ui| self.view_roles(state, ui));
                    seperated_collapsing(ui, "members", false, |ui| self.view_members(state, ui));
                    seperated_collapsing(ui, "channels", false, |ui| self.view_channels(state, ui));
                });
            });
        });
    }
}
