use std::{cmp::Ordering, ops::Not};

use client::{
    guild::Guild,
    harmony_rust_sdk::api::{chat::all_permissions, exports::prost::bytes::Bytes, profile::UserStatus},
    member::Member,
    message::{Attachment, Content, Embed, EmbedHeading, Message, MessageId, Override},
    smol_str::SmolStr,
    AHashMap, FetchEvent, Uri,
};
use eframe::egui::{Color32, Event, RichText};

use crate::{image_cache::LoadedImage, screen::guild_settings};

use super::{guild_discovery, prelude::*, settings};

#[derive(Default)]
pub struct Screen {
    last_channel_id: AHashMap<u64, u64>,
    current_guild: Option<u64>,
    current_channel: Option<u64>,
    composer_text: String,
    edit_message_text: String,
    scroll_to_bottom: bool,
    editing_message: Option<u64>,
    prev_editing_message: Option<u64>,
    disable_users_bar: bool,
}

impl Screen {
    fn view_guilds(&mut self, state: &mut State, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (guild_id, guild) in state.cache.get_guilds() {
                let icon = RichText::new(guild.name.get(0..1).unwrap_or("u").to_ascii_uppercase()).strong();

                let is_enabled = self.current_guild != Some(guild_id);

                let button = ui
                    .add_enabled_ui(is_enabled, |ui| {
                        if let Some((texid, _)) = guild.picture.as_ref().and_then(|id| state.image_cache.get_avatar(id))
                        {
                            ui.add(egui::ImageButton::new(texid, [32.0, 32.0]))
                        } else {
                            ui.add_sized([32.0, 32.0], egui::Button::new(icon))
                        }
                    })
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

            let discovery_but = ui
                .add_sized([32.0, 32.0], egui::Button::new(RichText::new("+").strong()))
                .on_hover_text("join / create guild");
            if discovery_but.clicked() {
                state.push_screen(guild_discovery::Screen::default());
            }
        });
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        guard!(let Some(guild_id) = self.current_guild else { return });

        if ui.text_button("⚙ - settings").clicked() {
            state.push_screen(guild_settings::Screen::new(guild_id));
        }

        ui.separator();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (channel_id, channel) in state.cache.get_channels(guild_id) {
                let text = RichText::new(format!("#{}", channel.name));

                let is_enabled = !channel.is_category && (self.current_channel != Some(channel_id));
                let button = ui.add_enabled(is_enabled, egui::Button::new(text).frame(false));
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

    fn view_message_text_content(
        &mut self,
        state: &State,
        ui: &mut Ui,
        id: &MessageId,
        guild_id: u64,
        channel_id: u64,
        text: &str,
    ) {
        if id.is_ack() && id.id() == self.editing_message {
            let edit = ui.add(egui::TextEdit::multiline(&mut self.edit_message_text).desired_rows(2));
            let is_pressed = ui.input().key_pressed(egui::Key::Enter) && !ui.input().modifiers.shift;
            if self.prev_editing_message.is_none() {
                edit.request_focus();
            }
            if self.edit_message_text.trim().is_empty().not() && edit.has_focus() && is_pressed {
                let client = state.client().clone();
                let text = self.edit_message_text.trim().to_string();
                let message_id = id.id().unwrap();
                self.editing_message = None;
                spawn_future!(state, async move {
                    client.edit_message(guild_id, channel_id, message_id, text).await
                });
            }
        } else {
            ui.label(text);
        }
    }

    fn view_message_url_embeds(&mut self, state: &State, ui: &mut Ui, text: &str) {
        let urls = text.split_whitespace().filter_map(|maybe_url| {
            maybe_url
                .parse::<Uri>()
                .ok()
                .and_then(|url| Some((state.cache.get_link_data(&url)?, maybe_url)))
        });
        for (data, url) in urls {
            match data {
                client::harmony_rust_sdk::api::mediaproxy::fetch_link_metadata_response::Data::IsSite(data) => {
                    let site_title_empty = data.site_title.is_empty().not();
                    let page_title_empty = data.page_title.is_empty().not();
                    let desc_empty = data.description.is_empty().not();
                    if site_title_empty && page_title_empty && desc_empty {
                        ui.group(|ui| {
                            if site_title_empty {
                                ui.add(egui::Label::new(RichText::new(&data.site_title).small()));
                            }
                            if page_title_empty {
                                ui.add(egui::Label::new(RichText::new(&data.page_title).strong()));
                            }
                            if site_title_empty && page_title_empty {
                                ui.separator();
                            }
                            if desc_empty {
                                ui.label(&data.description);
                            }
                        });
                    }
                }
                client::harmony_rust_sdk::api::mediaproxy::fetch_link_metadata_response::Data::IsMedia(data) => {
                    if ui.button(format!("open '{}' in browser", data.filename)).clicked() {
                        let _ = webbrowser::open(url);
                    }
                }
            }
        }
    }

    fn view_message_attachment(&mut self, state: &State, ui: &mut Ui, attachment: &Attachment) {
        let mut handled = false;
        let mut fetch = false;

        if attachment.kind.starts_with("image") {
            let mut no_thumbnail = false;

            let available_width = ui.available_width() / 3_f32;
            let downscale = |size: [f32; 2]| {
                let [w, h] = size;
                let max_size = (w < available_width).then(|| w).unwrap_or(available_width);
                let (w, h) = scale_down(w, h, max_size);
                [w as f32, h as f32]
            };

            let maybe_size = attachment.resolution.and_then(|(w, h)| {
                if w == 0 || h == 0 {
                    return None;
                }
                Some(downscale([w as f32, h as f32]))
            });

            if let Some((texid, size)) = state.image_cache.get_image(&attachment.id) {
                ui.add(egui::ImageButton::new(
                    texid,
                    maybe_size.unwrap_or_else(|| downscale(size)),
                ));
                handled = true;
            } else if let Some((texid, size)) = state.image_cache.get_thumbnail(&attachment.id) {
                let button = ui.add(egui::ImageButton::new(
                    texid,
                    maybe_size.unwrap_or_else(|| downscale(size)),
                ));
                fetch = button.clicked();
                handled = true;
            } else if let Some(size) = maybe_size {
                let button = ui.add_sized(size, egui::Button::new(format!("download '{}'", attachment.name)));
                fetch = button.clicked();
                handled = true;
                no_thumbnail = true;
            } else {
                no_thumbnail = true;
            }

            let load_thumbnail = no_thumbnail && state.loading_images.borrow().iter().all(|id| id != &attachment.id);
            if let (true, Some(minithumbnail)) = (load_thumbnail, &attachment.minithumbnail) {
                state.loading_images.borrow_mut().push(attachment.id.clone());
                let data = Bytes::copy_from_slice(minithumbnail.data.as_slice());
                let id = attachment.id.clone();
                let kind = SmolStr::new_inline("minithumbnail");
                spawn_future!(state, LoadedImage::load(data, id, kind));
            }
        }

        if !handled {
            fetch = ui.button(format!("download '{}'", attachment.name)).clicked();
        }

        if fetch {
            let client = state.client().clone();
            let attachment = attachment.clone();
            spawn_future!(state, async move {
                let (_, file) = client.fetch_attachment(attachment.id.clone()).await?;
                ClientResult::Ok(vec![FetchEvent::Attachment { attachment, file }])
            });
        }
    }

    fn view_message_embed(&mut self, ui: &mut Ui, embed: &Embed) {
        fn filter_empty(val: &Option<String>) -> Option<&str> {
            val.as_deref().map(str::trim).filter(|s| s.is_empty().not())
        }

        ui.group(|ui| {
            let do_render_heading =
                |heading: &&EmbedHeading| heading.text.is_empty().not() && filter_empty(&heading.subtext).is_some();
            let render_header = |header: &EmbedHeading, ui: &mut Ui| {
                // TODO: render icon
                ui.horizontal(|ui| {
                    let button = ui.add_enabled(
                        header.url.as_ref().map_or(false, |s| s.is_empty().not()),
                        egui::Button::new(RichText::new(&header.text).strong()),
                    );
                    if button.clicked() {
                        if let Some(url) = header.url.as_deref() {
                            let _ = webbrowser::open(url);
                        }
                    }
                    if let Some(subtext) = filter_empty(&header.subtext) {
                        ui.label(RichText::new(subtext).small());
                    }
                });
            };

            if let Some(heading) = embed.header.as_ref().filter(do_render_heading) {
                render_header(heading, ui);
                ui.add_space(8.0);
            }

            if embed.title.is_empty().not() {
                ui.label(RichText::new(&embed.title).strong());
            }

            if let Some(body) = filter_empty(&embed.body) {
                ui.label(body);
            }

            for field in &embed.fields {
                ui.group(|ui| {
                    if field.title.is_empty().not() {
                        ui.label(RichText::new(&field.title).strong());
                    }
                    if let Some(subtitle) = filter_empty(&field.subtitle) {
                        ui.label(RichText::new(subtitle).small());
                    }
                    ui.add_space(4.0);
                    if let Some(body) = filter_empty(&field.body) {
                        ui.label(body);
                    }
                });
            }

            if let Some(heading) = embed.footer.as_ref().filter(do_render_heading) {
                ui.add_space(8.0);
                render_header(heading, ui);
            }
        });
    }

    fn view_messages(&mut self, state: &mut State, ui: &mut Ui) {
        guard!(let Some((guild_id, channel_id)) = self.current_guild.zip(self.current_channel) else { return });
        guard!(let Some(channel) = state.cache.get_channel(guild_id, channel_id) else { return });
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });
        let user_id = state.client().user_id();

        egui::ScrollArea::vertical()
            .stick_to_bottom()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (id, message) in channel.messages.iter() {
                    let msg = ui
                        .group(|ui| {
                            let overrides = message.overrides.as_ref();
                            let override_name = overrides.and_then(|ov| ov.name.as_ref().map(SmolStr::as_str));
                            let user = state.cache.get_user(message.sender);
                            let sender_name = user.map_or_else(|| "unknown", |u| u.username.as_str());
                            let display_name = override_name.unwrap_or(sender_name);

                            let color = guild
                                .highest_role_for_member(message.sender)
                                .map_or(Color32::WHITE, |(_, role)| rgb_color(role.color));

                            ui.horizontal(|ui| {
                                self.view_user_avatar(state, ui, user, overrides);
                                ui.label(RichText::new(display_name).color(color).strong());
                                if override_name.is_some() {
                                    ui.label(RichText::new(format!("({})", sender_name)).italics().small());
                                }
                            });

                            match &message.content {
                                client::message::Content::Text(text) => {
                                    self.view_message_text_content(state, ui, id, guild_id, channel_id, text);
                                    self.view_message_url_embeds(state, ui, text);
                                }
                                client::message::Content::Files(attachments) => {
                                    for attachment in attachments {
                                        self.view_message_attachment(state, ui, attachment);
                                    }
                                }
                                client::message::Content::Embeds(embeds) => {
                                    for embed in embeds {
                                        self.view_message_embed(ui, embed);
                                    }
                                }
                            }
                        })
                        .response;

                    msg.context_menu(|ui| {
                        if let Some(message_id) = id.id() {
                            if let client::message::Content::Text(text) = &message.content {
                                if channel.has_perm(all_permissions::MESSAGES_SEND)
                                    && message.sender == user_id
                                    && ui.button("edit").clicked()
                                {
                                    self.editing_message = id.id();
                                    self.edit_message_text = text.clone();
                                    ui.close_menu();
                                }
                                if ui.button("copy").clicked() {
                                    ui.close_menu();
                                }
                            }
                            if message.sender == state.client().user_id() && ui.button("delete").clicked() {
                                let client = state.client().clone();
                                spawn_future!(state, async move {
                                    client.delete_message(guild_id, channel_id, message_id).await
                                });
                                ui.close_menu();
                            }
                            if channel.has_perm(all_permissions::MESSAGES_PINS_ADD) && ui.button("pin").clicked() {
                                ui.close_menu();
                            }
                        }
                    });
                }
                if self.scroll_to_bottom {
                    ui.scroll_to_cursor(egui::Align::Max);
                    self.scroll_to_bottom = false;
                }
            });
    }

    fn view_composer(&mut self, state: &mut State, ui: &mut Ui, ctx: &egui::CtxRef) {
        guard!(let Some((guild_id, channel_id)) = self.current_guild.zip(self.current_channel) else { return });

        let text_edit = ui.add(
            egui::TextEdit::multiline(&mut self.composer_text)
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .hint_text("Enter message..."),
        );

        if self.editing_message.is_none() && ctx.input().events.iter().any(|ev| matches!(ev, Event::Text(_))) {
            text_edit.request_focus();
        }

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

    fn view_user_avatar(&mut self, state: &State, ui: &mut Ui, user: Option<&Member>, overrides: Option<&Override>) {
        let maybe_tex = overrides
            .and_then(|ov| ov.avatar_url.as_ref())
            .or_else(|| user.and_then(|u| u.avatar_url.as_ref()))
            .as_ref()
            .and_then(|id| state.image_cache.get_avatar(id));

        if let Some((texid, _)) = maybe_tex {
            ui.image(texid, [32.0, 32.0]);
        } else {
            ui.add_enabled_ui(false, |ui| {
                let username = overrides
                    .and_then(|ov| ov.name.as_deref())
                    .or_else(|| user.map(|u| u.username.as_str()))
                    .unwrap_or("");

                ui.add_sized(
                    [32.0, 32.0],
                    egui::Button::new(username.get(0..1).unwrap_or("u").to_ascii_uppercase()),
                )
            });
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
                    ui.horizontal(|ui| {
                        self.view_user_avatar(state, ui, Some(user), None);
                        ui.label(user.username.as_str());
                    });
                    ui.separator();
                }
            });
    }

    fn view_profile_menu(&mut self, state: &mut State, ui: &mut Ui) {
        let username = state
            .cache
            .get_user(state.client().user_id())
            .map_or_else(|| "loading...", |u| u.username.as_str());
        let title = format!("☰ - {}", username);

        ui.vertical_centered_justified(|ui| {
            let response = ui.text_button(&title);
            let popup_id = ui.make_persistent_id("profile_menu");
            if response.clicked() {
                ui.memory().toggle_popup(popup_id);
            }
            egui::popup_below_widget(ui, popup_id, &response, |ui| {
                if ui.text_button("settings").clicked() {
                    state.push_screen(settings::Screen::default());
                }

                ui.add(egui::Separator::default().spacing(0.0));

                if ui.text_button("logout").clicked() {
                    let client = state.client().clone();
                    spawn_future!(state, async move { client.logout().await });
                    state.client = None;
                    state.pop_screen();
                }

                ui.add(egui::Separator::default().spacing(0.0));

                if ui.text_button("exit loqui").clicked() {
                    std::process::exit(0);
                }
            });
        });
    }

    fn view_members_hidden(&mut self, ui: &mut Ui) {
        let but = ui
            .add_sized(
                [20.0, ui.available_height()],
                egui::Button::new("<<<<<<<<<<<<<<<<<<<<<").frame(false).small(),
            )
            .on_hover_ui_at_pointer(|ui| {
                ui.label("click to enlarge\nmembers list");
            });
        if but.clicked() {
            self.disable_users_bar = false;
        }
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
    fn update(&mut self, ctx: &egui::CtxRef, _: &epi::Frame, state: &mut State) {
        if ctx.input().key_pressed(egui::Key::Escape) {
            self.editing_message = None;
        }

        if ctx.input().key_pressed(egui::Key::ArrowUp) {
            let maybe_channel = self
                .current_guild
                .zip(self.current_channel)
                .and_then(|(gid, cid)| state.cache.get_channel(gid, cid));

            if let Some(chan) = maybe_channel {
                let user_id = state.client().user_id();
                let maybe_msg = chan
                    .messages
                    .iter()
                    .rev()
                    .filter_map(|(id, msg)| id.is_ack().then(|| (id.id().unwrap(), msg)))
                    .filter_map(|(id, msg)| {
                        if let Content::Text(text) = &msg.content {
                            Some((id, text, msg.sender))
                        } else {
                            None
                        }
                    })
                    .find(|(_, _, sender)| *sender == user_id);

                if let Some((id, text, _)) = maybe_msg {
                    self.editing_message = Some(id);
                    self.edit_message_text = text.to_string();
                }
            }
        }

        egui::panel::SidePanel::left("guild_panel")
            .min_width(32.0)
            .max_width(32.0)
            .resizable(false)
            .show(ctx, |ui| self.view_guilds(state, ui));

        if self.current_guild.is_some() {
            egui::panel::SidePanel::left("channel_panel")
                .min_width(100.0)
                .max_width(300.0)
                .resizable(true)
                .show(ctx, |ui| self.view_channels(state, ui));

            if !self.disable_users_bar {
                egui::panel::SidePanel::right("member_panel")
                    .min_width(100.0)
                    .max_width(300.0)
                    .resizable(true)
                    .show(ctx, |ui| {
                        self.view_profile_menu(state, ui);
                        ui.separator();
                        ui.add_space(4.0);
                        self.view_members(state, ui);
                    });
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let maybe_guild_name = self
                .current_guild
                .map(|id| state.cache.get_guild(id).map_or("unknown", |g| g.name.as_str()));

            if let Some(guild_name) = maybe_guild_name {
                egui::TopBottomPanel::top("central_top_panel")
                    .resizable(false)
                    .min_height(12.0)
                    .max_height(12.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(guild_name);
                            ui.add_space(ui.available_width() - 12.0);
                            let show_members_but = ui
                                .add_sized([12.0, 12.0], egui::Button::new("👤").frame(false).small())
                                .on_hover_text("toggle member list");
                            if show_members_but.clicked() {
                                self.disable_users_bar = !self.disable_users_bar;
                            }
                        });
                    });

                if self.current_channel.is_some() {
                    ui.with_layout(
                        Layout::from_main_dir_and_cross_align(egui::Direction::LeftToRight, egui::Align::Center),
                        |ui| {
                            ui.vertical(|ui| {
                                self.view_messages(state, ui);
                                ui.separator();
                                self.view_composer(state, ui, ctx);
                            });
                        },
                    );
                }
            }
        });

        self.prev_editing_message = self.editing_message;
    }
}
