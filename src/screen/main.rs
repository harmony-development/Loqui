use std::{cmp::Ordering, ops::Not};

use client::{
    guild::Guild,
    harmony_rust_sdk::api::profile::UserStatus,
    member::Member,
    message::{Content, Message},
    smol_str::SmolStr,
    AHashMap,
};
use eframe::egui::{Color32, RichText};

use super::prelude::*;

#[derive(Default)]
pub struct Screen {
    last_channel_id: AHashMap<u64, u64>,
    current_guild: Option<u64>,
    current_channel: Option<u64>,
    composer_text: String,
    scroll_to_bottom: bool,
}

impl Screen {
    fn view_guilds(&mut self, state: &mut State, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (guild_id, guild) in state.cache.get_guilds() {
                let icon = RichText::new(guild.name.get(0..1).unwrap_or("u").to_ascii_uppercase()).strong();

                let is_enabled = self.current_guild != Some(guild_id);

                let button = ui
                    .add_enabled_ui(is_enabled, |ui| ui.add_sized([32.0, 32.0], egui::Button::new(icon)))
                    .inner
                    .on_hover_text(guild.name.as_str());

                if button.clicked() {
                    self.current_guild = Some(guild_id);
                    if let Some(channel_id) = self.last_channel_id.get(&guild_id) {
                        self.current_channel = Some(*channel_id);
                    }
                    if guild.channels.is_empty() && guild.members.is_empty() {
                        spawn_evs!(state, |events, c| async move {
                            c.fetch_channels(guild_id, events).await?;
                            c.fetch_members(guild_id, events).await?;
                            Ok(())
                        });
                    }
                    self.scroll_to_bottom = true;
                }

                ui.separator();
            }
        });
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        guard!(let Some(guild_id) = self.current_guild else { return });

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (channel_id, channel) in state.cache.get_channels(guild_id) {
                let text = RichText::new(format!("#{}", channel.name));

                let is_enabled = !channel.is_category && (self.current_channel != Some(channel_id));
                let button = ui.add_enabled(is_enabled, egui::Button::new(text));
                if button.clicked() {
                    self.current_channel = Some(channel_id);
                    self.last_channel_id.insert(guild_id, channel_id);
                    if !channel.reached_top && channel.messages.is_empty() {
                        spawn_evs!(state, |events, c| async move {
                            c.fetch_messages(guild_id, channel_id, events).await?;
                            Ok(())
                        });
                    }
                    self.scroll_to_bottom = true;
                }
            }
        });
    }

    fn view_messages(&mut self, state: &State, ui: &mut Ui) {
        guard!(let Some((guild_id, channel_id)) = self.current_guild.zip(self.current_channel) else { return });
        guard!(let Some(channel) = state.cache.get_channel(guild_id, channel_id) else { return });
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        egui::ScrollArea::vertical()
            .stick_to_bottom()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (id, message) in channel.messages.iter() {
                    ui.group(|ui| {
                        let override_name = message
                            .overrides
                            .as_ref()
                            .and_then(|ov| ov.name.as_ref().map(SmolStr::as_str));
                        let sender_name = state
                            .cache
                            .get_user(message.sender)
                            .map_or_else(|| "unknown", |u| u.username.as_str());
                        let display_name = override_name.unwrap_or(sender_name);

                        let color = guild
                            .highest_role_for_member(message.sender)
                            .map_or(Color32::WHITE, |(_, role)| rgb_color(role.color));

                        ui.horizontal(|ui| {
                            ui.label(RichText::new(display_name).color(color).strong());
                            if override_name.is_some() {
                                ui.label(RichText::new(format!("({})", sender_name)).italics().small());
                            }
                        });

                        match &message.content {
                            client::message::Content::Text(text) => {
                                ui.label(text);
                            }
                            client::message::Content::Files(_) => {}
                            client::message::Content::Embeds(_) => {}
                        }
                    });
                }
                if self.scroll_to_bottom {
                    ui.scroll_to_cursor(egui::Align::Max);
                    self.scroll_to_bottom = false;
                }
            });
    }

    fn view_composer(&mut self, state: &mut State, ui: &mut Ui) {
        guard!(let Some((guild_id, channel_id)) = self.current_guild.zip(self.current_channel) else { return });

        let text_edit = ui.add(
            egui::TextEdit::multiline(&mut self.composer_text)
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .hint_text("Enter message..."),
        );
        let is_pressed = ui.input().key_pressed(egui::Key::Enter) && !ui.input().modifiers.shift;
        if self.composer_text.trim().is_empty().not() && text_edit.has_focus() && is_pressed {
            let message = Message {
                content: Content::Text(
                    self.composer_text
                        .drain(..self.composer_text.len())
                        .collect::<String>()
                        .trim()
                        .to_string(),
                ),
                sender: state.client().user_id(),
                ..Default::default()
            };
            let echo_id = state.cache.prepare_send_message(guild_id, channel_id, message.clone());
            let client = state.client().clone();
            spawn_future!(state, async move {
                client.send_message(echo_id, guild_id, channel_id, message).await
            });
            self.scroll_to_bottom = true;
        }
    }

    fn view_members(&mut self, state: &State, ui: &mut Ui) {
        guard!(let Some(guild_id) = self.current_guild else { return });
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        let row_height = ui.spacing().interact_size.y;
        let row_number = guild.members.len();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_height, row_number, |ui, range| {
                let sorted_members = Self::sort_members(state, guild);
                guard!(let Some(users) = sorted_members.get(range) else { return });
                for (id, _) in users {
                    guard!(let Some(user) = state.cache.get_user(**id) else { continue });
                    ui.label(user.username.as_str());
                    ui.separator();
                }
            });
    }

    fn view_profile_menu(&mut self, state: &mut State, ui: &mut Ui) {
        let username = state
            .cache
            .get_user(state.client().user_id())
            .map_or_else(|| SmolStr::new_inline("loading..."), |u| u.username.clone());

        ui.vertical_centered_justified(|ui| {
            ui.menu_button(username.as_str(), |ui| {
                if ui.button("logout").clicked() {
                    let client = state.client().clone();
                    let content_store = state.content_store.clone();
                    spawn_future!(state, async move { client.logout(content_store.as_ref(), true).await });
                    state.client = None;
                    state.pop_screen();
                }

                if ui.button("exit loqui").clicked() {
                    std::process::exit(0);
                }
            });
        });
    }

    fn sort_members<'a, 'b>(state: &'a State, guild: &'b Guild) -> Vec<(&'b u64, &'a Member)> {
        let mut sorted_members = guild
            .members
            .keys()
            .flat_map(|id| state.cache.get_user(*id).map(|m| (id, m)))
            .collect::<Vec<_>>();
        sorted_members.sort_unstable_by(|(_, member), (_, other_member)| {
            let name = member.username.as_str().cmp(other_member.username.as_str());
            let offline = matches!(member.status, UserStatus::OfflineUnspecified);
            let other_offline = matches!(other_member.status, UserStatus::OfflineUnspecified);

            match (offline, other_offline) {
                (false, true) => Ordering::Less,
                (true, false) => Ordering::Greater,
                _ => name,
            }
        });
        sorted_members
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, _: &mut epi::Frame, state: &mut State) {
        egui::panel::SidePanel::left("guild_panel")
            .min_width(32.0)
            .max_width(32.0)
            .resizable(false)
            .show(ctx, |ui| self.view_guilds(state, ui));
        egui::panel::SidePanel::left("channel_panel")
            .min_width(100.0)
            .max_width(400.0)
            .resizable(true)
            .show(ctx, |ui| self.view_channels(state, ui));
        egui::panel::SidePanel::right("member_panel")
            .min_width(100.0)
            .max_width(400.0)
            .resizable(true)
            .show(ctx, |ui| {
                self.view_profile_menu(state, ui);
                ui.separator();
                ui.add_space(4.0);
                self.view_members(state, ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(
                Layout::from_main_dir_and_cross_align(egui::Direction::LeftToRight, egui::Align::Center),
                |ui| {
                    ui.vertical(|ui| {
                        self.view_messages(state, ui);
                        ui.separator();
                        self.view_composer(state, ui);
                    });
                },
            );
        });
    }
}
