use std::{fmt::Write, ops::Not};

use client::{
    channel::Channel,
    content, get_random_u64,
    guild::Guild,
    harmony_rust_sdk::api::chat::{all_permissions, Permission},
    member::Member,
    role::Role,
};
use eframe::egui::{CollapsingHeader, Color32, RichText};

use crate::widgets::{view_channel_context_menu_items, view_member_context_menu_items, Avatar, Toggle};

use super::prelude::*;

pub struct Screen {
    guild_id: u64,
    id_edit_text: String,
    possible_uses_text: String,
    channel_name_edit_text: String,
    guild_name_edit_text: String,
    fetching_invites: AtomBool,
    uploading_guild_pic: AtomBool,
    managing_perms: bool,
    managing_perms_role: u64,
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
            managing_perms: false,
            managing_perms_role: 0,
        }
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        let Some(guild) = state.cache.get_guild(guild_id) else { return };

        if guild.has_perm(all_permissions::CHANNELS_MANAGE_CREATE) {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.channel_name_edit_text).hint_text("channel name"));

                if ui.button("create channel").clicked() {
                    let name = self.channel_name_edit_text.drain(..).collect::<String>();
                    spawn_client_fut!(state, |client| client.create_channel(guild_id, name).await);
                }
            });

            ui.separator();
        }

        for channel_id in guild.channels.iter().copied() {
            let Some(channel) = state.cache.get_channel(guild_id, channel_id) else { continue };
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
            view_channel_context_menu_items(ui, state, self.guild_id, channel_id, guild, channel);
            if guild.has_perm(all_permissions::CHANNELS_MANAGE_CHANGE_INFORMATION) && ui.button("edit").clicked() {
                // TODO
                ui.close_menu();
            }
        });
    }

    fn view_general(&mut self, state: &mut State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        let Some(guild) = state.cache.get_guild(guild_id) else { return };

        ui.horizontal_top(|ui| {
            if self.uploading_guild_pic.get().not() {
                let avatar_but = ui
                    .add_enabled(
                        guild.has_perm(all_permissions::GUILD_MANAGE_CHANGE_INFORMATION),
                        Avatar::new(guild.picture.as_deref(), guild.name.as_str(), state).size(64.0),
                    )
                    .on_hover_text("set picture");
                if avatar_but.clicked() {
                    let uploading_guild_pic = self.uploading_guild_pic.clone();
                    spawn_client_fut!(state, |client| {
                        let maybe_file = rfd::AsyncFileDialog::new().pick_file().await;
                        if let Some(file) = maybe_file {
                            uploading_guild_pic
                                .scope(async move {
                                    let name = file.file_name();
                                    let data = file.read().await;
                                    let mimetype = content::infer_type_from_bytes(&data);
                                    let id = client.upload_file(name, mimetype, data).await?;
                                    client.edit_guild(guild_id, None, Some(id)).await?;
                                    ClientResult::Ok(())
                                })
                                .await?;
                        }
                        ClientResult::Ok(())
                    });
                }
            } else {
                ui.add(egui::Spinner::new().size(64.0))
                    .on_hover_text("uploading avatar");
            }

            ui.vertical(|ui| {
                if guild.has_perm(all_permissions::GUILD_MANAGE_CHANGE_INFORMATION) {
                    ui.label(RichText::new("guild name").small());
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.guild_name_edit_text).desired_width(100.0));
                        if ui.add(egui::Button::new("edit").small()).clicked() {
                            let new_name = self.guild_name_edit_text.clone();
                            spawn_client_fut!(state, |client| client.edit_guild(guild_id, Some(new_name), None).await);
                        }
                    });
                } else {
                    ui.label(RichText::new("guild name").small());
                    ui.label(guild.name.as_str());
                }
            });
        });
    }

    fn view_invites(&mut self, state: &mut State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        let Some(guild) = state.cache.get_guild(guild_id) else { return };

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

                if ui.button("create").clicked() {
                    if let Ok(uses) = self.possible_uses_text.parse::<u64>() {
                        let name = self.id_edit_text.drain(..).collect::<String>();
                        spawn_client_fut!(state, |client| client.create_invite(guild_id, name, uses as u32).await);
                    }
                }

                if ui.button("generate").clicked() {
                    self.id_edit_text.clear();
                    write!(&mut self.id_edit_text, "{}", get_random_u64()).unwrap();
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
                            spawn_client_fut!(state, |client| client.delete_invite(guild_id, name).await);
                            ui.close_menu();
                        }
                    });
                }
            } else {
                ui.colored_label(Color32::YELLOW, "no invites");
            }
        } else if self.fetching_invites.get().not() {
            let fetching_invites = self.fetching_invites.clone();
            spawn_evs!(state, |events, client| {
                fetching_invites.scope(client.fetch_invites(guild_id, events)).await?;
            });
        } else {
            ui.add(egui::Spinner::new().size(32.0))
                .on_hover_text("fetching invites");
        }
    }

    fn view_members(&mut self, state: &State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        let Some(guild) = state.cache.get_guild(guild_id) else { return };

        let sorted_members = sort_members(state, guild);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            for (id, member) in sorted_members {
                self.view_member(state, ui, guild_id, *id, member, guild);
                if ui.available_size_before_wrap().x <= 200.0 {
                    ui.end_row();
                }
            }
        });
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
            view_member_context_menu_items(ui, state, guild_id, user_id, guild, member);
            if guild.has_perm(all_permissions::ROLES_USER_MANAGE) && ui.button("manage roles").clicked() {
                // TODO
                ui.close_menu();
            }
        });
    }

    fn view_roles(&mut self, state: &mut State, ui: &mut Ui) {
        let guild_id = self.guild_id;
        let Some(guild) = state.cache.get_guild(guild_id) else { return };

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            for (role_id, role) in &guild.roles {
                self.view_role(state, ui, guild, role, *role_id);
                if ui.available_size_before_wrap().x <= 200.0 {
                    ui.end_row();
                }
            }
        });
    }

    fn view_role(&mut self, state: &State, ui: &mut Ui, guild: &Guild, role: &Role, role_id: u64) {
        let id_string = role_id.to_string();
        let resp = ui
            .group(|ui| {
                ui.horizontal(|ui| {
                    let color = ui.role_color(role);
                    ui.label(RichText::new(&id_string).small());
                    ui.colored_label(color, role.name.as_str());
                });
            })
            .response;
        resp.on_hover_text("right click to manage").context_menu_styled(|ui| {
            if ui.button("copy id").clicked() {
                ui.output().copied_text = id_string;
                ui.close_menu();
            }
            if ui.button("copy name").clicked() {
                ui.output().copied_text.push_str(role.name.as_str());
                ui.close_menu();
            }
            if guild.has_perm(all_permissions::PERMISSIONS_MANAGE_GET) && ui.button("manage permissions").clicked() {
                self.managing_perms = true;
                self.managing_perms_role = role_id;
                if guild.role_perms.is_empty() {
                    let guild_id = self.guild_id;
                    let channels = guild.channels.clone();
                    spawn_evs!(state, |events, client| {
                        client.fetch_role_perms(guild_id, channels, role_id, events).await?;
                    });
                }
                ui.close_menu();
            }
        });
    }

    fn view_manage_role_perms(&mut self, state: &State, ctx: &egui::Context) {
        let Some(guild) = state.cache.get_guild(self.guild_id) else { return };
        let Some(role) = guild.roles.get(&self.managing_perms_role) else { return };
        let Some(role_perms) = guild.role_perms.get(&self.managing_perms_role) else {
            egui::Window::new(role.name.as_str())
            .open(&mut self.managing_perms)
            .show(ctx, |ui| {
                ui.add(egui::Spinner::new().size(32.0));
            });
            return;
        };

        let mut managing_perms = self.managing_perms;

        egui::Window::new(role.name.as_str())
            .open(&mut managing_perms)
            .show(ctx, |ui| {
                ui.label("guild");
                ui.separator();
                for perm in role_perms {
                    self.view_perm(state, ui, self.guild_id, None, self.managing_perms_role, perm, guild);
                }

                for (channel_id, channel) in guild
                    .channels
                    .iter()
                    .filter_map(|id| state.cache.get_channel(self.guild_id, *id).map(|c| (id, c)))
                {
                    let Some(role_perms) = channel.role_perms.get(&self.managing_perms_role) else { continue };

                    ui.label(format!("#{}", channel.name));
                    ui.separator();
                    for perm in role_perms {
                        self.view_perm(
                            state,
                            ui,
                            self.guild_id,
                            Some(*channel_id),
                            self.managing_perms_role,
                            perm,
                            guild,
                        );
                    }
                }
            });

        self.managing_perms = managing_perms;
    }

    #[allow(clippy::too_many_arguments)]
    fn view_perm(
        &self,
        state: &State,
        ui: &mut Ui,
        guild_id: u64,
        channel_id: Option<u64>,
        role_id: u64,
        perm: &Permission,
        guild: &Guild,
    ) {
        let mut new_ok = perm.ok;

        ui.horizontal(|ui| {
            ui.label(&perm.matches);
            ui.add_space(ui.available_width() - ui.spacing().interact_size.y * 2.0);
            ui.add_enabled(
                guild.has_perm(all_permissions::PERMISSIONS_MANAGE_SET),
                Toggle::new(&mut new_ok),
            );
            if ui.ui_contains_pointer() && ui.input().pointer.any_click() && new_ok != perm.ok {
                let perm = Permission {
                    matches: perm.matches.clone(),
                    ok: new_ok,
                };
                spawn_client_fut!(state, |client| {
                    client.set_role_perms(guild_id, channel_id, role_id, vec![perm]).await
                });
            }
        });
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::Context, _: &epi::Frame, state: &mut State) {
        self.view_manage_role_perms(state, ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.group(|ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    CollapsingHeader::new("general")
                        .default_open(true)
                        .show(ui, |ui| self.view_general(state, ui));

                    CollapsingHeader::new("invites").show(ui, |ui| self.view_invites(state, ui));
                    CollapsingHeader::new("roles").show(ui, |ui| self.view_roles(state, ui));
                    CollapsingHeader::new("members").show(ui, |ui| self.view_members(state, ui));
                    CollapsingHeader::new("channels").show(ui, |ui| self.view_channels(state, ui));
                });
            });
        });
    }
}
