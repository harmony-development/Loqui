use std::{cell::RefCell, cmp::Ordering, ops::Not};

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

use super::prelude::*;

#[derive(Debug, Default)]
struct CurrentIds {
    guild: Option<u64>,
    channel: Option<u64>,
}

impl CurrentIds {
    #[inline(always)]
    fn guild(&self) -> Option<u64> {
        self.guild
    }

    #[inline(always)]
    fn set_guild(&mut self, id: u64) {
        self.guild = Some(id);
        self.channel = None;
    }

    #[inline(always)]
    fn is_guild(&self, id: u64) -> bool {
        self.guild().map_or(false, |oid| oid == id)
    }

    #[inline(always)]
    fn has_guild(&self) -> bool {
        self.guild().is_some()
    }

    #[inline(always)]
    fn channel(&self) -> Option<(u64, u64)> {
        self.guild().zip(self.channel)
    }

    #[inline(always)]
    fn set_channel(&mut self, id: u64) {
        self.channel = Some(id);
    }

    #[inline(always)]
    fn is_channel(&self, gid: u64, cid: u64) -> bool {
        self.channel().map_or(false, |oid| oid == (gid, cid))
    }

    #[inline(always)]
    fn has_channel(&self) -> bool {
        self.channel().is_some()
    }
}

#[derive(Default)]
pub struct Screen {
    // guild id -> channel id
    last_channel_id: AHashMap<u64, u64>,
    current: CurrentIds,
    composer_text: String,
    edit_message_text: String,
    scroll_to_bottom: bool,
    editing_message: Option<u64>,
    prev_editing_message: Option<u64>,
    disable_users_bar: bool,
    typing_animating: bool,
    invite_text: RefCell<String>,
    guild_name_text: RefCell<String>,
    show_join_guild: RefCell<bool>,
    show_create_guild: RefCell<bool>,
}

impl Screen {
    fn view_join_guild(&self, state: &mut State, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.label(RichText::new("join guild").heading().strong());
            ui.add_space(12.0);
            ui.text_edit_singleline(&mut *self.invite_text.borrow_mut());
            ui.add_space(6.0);

            if ui.button("join").clicked() {
                let invite_id = self.invite_text.borrow().clone();
                spawn_client_fut!(state, |client| {
                    client.join_guild(invite_id).await?;
                });
            }
        });
    }

    fn view_create_guild(&self, state: &mut State, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.label(RichText::new("create guild").heading().strong());
            ui.add_space(12.0);
            ui.text_edit_singleline(&mut *self.guild_name_text.borrow_mut());
            ui.add_space(6.0);
            if ui.button("create").clicked() {
                let guild_name = self.guild_name_text.borrow().clone();
                spawn_client_fut!(state, |client| {
                    client.create_guild(guild_name).await?;
                });
            }
        });
    }

    fn view_guilds(&mut self, state: &mut State, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if state.cache.is_initial_sync_complete().not() {
                ui.add(egui::Spinner::new().size(32.0)).on_hover_text("loading guilds");
            }

            for (guild_id, guild) in state.cache.get_guilds() {
                let icon = RichText::new(guild.name.get(0..1).unwrap_or("u").to_ascii_uppercase()).strong();

                let is_enabled = guild.fetched && self.current.guild() != Some(guild_id);

                let button = ui
                    .add_enabled_ui(is_enabled, |ui| {
                        if guild.fetched {
                            if let Some((texid, _)) =
                                guild.picture.as_ref().and_then(|id| state.image_cache.get_avatar(id))
                            {
                                ui.add(egui::ImageButton::new(texid, [32.0, 32.0]).frame(false))
                            } else {
                                ui.add_sized([32.0, 32.0], egui::Button::new(icon))
                            }
                        } else {
                            ui.add(egui::Spinner::new().size(32.0))
                        }
                    })
                    .inner
                    .on_hover_text(guild.name.as_str());

                if button.clicked() {
                    self.current.set_guild(guild_id);
                    if let Some(channel_id) = self.last_channel_id.get(&guild_id) {
                        self.current.set_channel(*channel_id);
                    }
                    if guild.channels.is_empty() && guild.members.is_empty() {
                        spawn_evs!(state, |events, c| {
                            c.fetch_channels(guild_id, events).await?;
                            c.fetch_members(guild_id, events).await?;
                        });
                    }
                    self.scroll_to_bottom = true;
                }

                ui.separator();
            }

            let join_but = ui
                .add_sized([32.0, 32.0], egui::Button::new(RichText::new("j+").strong()))
                .on_hover_text("join guild");
            if join_but.clicked() {
                *self.show_join_guild.borrow_mut() = true;
            }

            let create_but = ui
                .add_sized([32.0, 32.0], egui::Button::new(RichText::new("c+").strong()))
                .on_hover_text("create guild");
            if create_but.clicked() {
                *self.show_create_guild.borrow_mut() = true;
            }
        });
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        guard!(let Some(guild_id) = self.current.guild() else { return });

        let guild_name = state
            .cache
            .get_guild(guild_id)
            .map_or_else(|| "unknown", |g| g.name.as_str());

        let menu_but_clicked = egui::Frame::group(ui.style())
            .margin([0.0, 1.5])
            .show(ui, |ui| {
                let but = ui
                    .add(egui::Button::new(format!("⚙ {}", guild_name)).small().frame(false))
                    .on_hover_text("open guild settings");

                but.clicked()
            })
            .inner;

        if menu_but_clicked {
            state.push_screen(guild_settings::Screen::new(guild_id));
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let channels = state.cache.get_channels(guild_id);

            if channels.is_empty().not() {
                for (channel_id, channel) in channels {
                    if channel.fetched {
                        let text = RichText::new(format!("#{}", channel.name));

                        let is_enabled = (channel.is_category || self.current.is_channel(guild_id, channel_id)).not();
                        let button = ui.add_enabled(is_enabled, egui::Button::new(text).frame(false));
                        if button.clicked() {
                            self.current.set_channel(channel_id);
                            self.last_channel_id.insert(guild_id, channel_id);
                            if !channel.reached_top && channel.messages.is_empty() {
                                spawn_evs!(state, |events, c| {
                                    c.fetch_messages(guild_id, channel_id, events).await?;
                                });
                            }
                            self.scroll_to_bottom = true;
                        }
                    } else {
                        ui.add(egui::Spinner::new());
                    }
                }
            } else {
                ui.add_sized(ui.available_size(), egui::Spinner::new().size(32.0))
                    .on_hover_text_at_pointer("loading channels");
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
                let text = self.edit_message_text.trim().to_string();
                let message_id = id.id().unwrap();
                self.editing_message = None;
                spawn_client_fut!(state, |client| {
                    client.edit_message(guild_id, channel_id, message_id, text).await?;
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
                        open_url(url.to_string());
                    }
                }
            }
        }
    }

    fn view_message_attachment(&mut self, state: &State, ui: &mut Ui, frame: &epi::Frame, attachment: &Attachment) {
        let mut handled = false;
        let mut fetch = false;
        let mut open = false;

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
                let open_but = ui.add(egui::ImageButton::new(
                    texid,
                    maybe_size.unwrap_or_else(|| downscale(size)),
                ));
                open = open_but.clicked();
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
                spawn_future!(state, LoadedImage::load(frame.clone(), data, id, kind));
            }
        }

        if !handled {
            open = ui.button(format!("open '{}'", attachment.name)).clicked();
        }

        if fetch {
            let client = state.client().clone();
            let attachment = attachment.clone();
            spawn_future!(state, async move {
                let (_, file) = client.fetch_attachment(attachment.id.clone()).await?;
                ClientResult::Ok(vec![FetchEvent::Attachment { attachment, file }])
            });
        }

        if open {
            open_url(make_url_from_file_id(state.client(), &attachment.id));
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
                        if let Some(url) = header.url.clone() {
                            open_url(url);
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

    fn view_messages(&mut self, state: &mut State, ui: &mut Ui, frame: &epi::Frame) {
        guard!(let Some((guild_id, channel_id)) = self.current.channel() else { return });
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
                                        self.view_message_attachment(state, ui, frame, attachment);
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
                                spawn_client_fut!(state, |client| {
                                    client.delete_message(guild_id, channel_id, message_id).await?;
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

    fn view_typing_members(&mut self, state: &State, ui: &mut Ui) {
        guard!(let Some(guild_id) = self.current.guild() else { return });
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        let current_user_id = state.client().user_id();
        let typing_members = self
            .get_typing_members(state, guild)
            .filter(|(id, _)| current_user_id.ne(id))
            .collect::<Vec<_>>();
        let typing_members_len = typing_members.len();

        if typing_members_len > 0 {
            let mut typing_animating = self.typing_animating;

            ui.horizontal(|ui| {
                let mut names = String::new();

                for (index, (_, member)) in typing_members.iter().enumerate() {
                    names.push_str(member.username.as_str());
                    if index != typing_members_len - 1 {
                        names.push_str(", ");
                    } else {
                        names.push(' ');
                    }
                }

                let typing_animate_value =
                    ui.animate_bool_with_time_alternate("typing animation", &mut typing_animating, 1.0);

                names.push_str((typing_members_len > 1).then(|| "are").unwrap_or("is"));
                names.push_str(" typing");
                for index in 1..=3 {
                    let dot_index = (typing_animate_value * 5.0) as usize;
                    let ch = (dot_index == index).then(|| '·').unwrap_or('.');
                    names.push(ch);
                }

                ui.label(RichText::new(names).small().strong());
            });

            self.typing_animating = typing_animating;
        }
    }

    fn view_composer(&mut self, state: &mut State, ui: &mut Ui, ctx: &egui::CtxRef) {
        guard!(let Some((guild_id, channel_id)) = self.current.channel() else { return });

        let text_edit = ui.add(
            egui::TextEdit::multiline(&mut self.composer_text)
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .hint_text("Enter message..."),
        );

        let user_inputted_text = ctx.input().events.iter().any(|ev| matches!(ev, Event::Text(_)));

        if text_edit.has_focus().not() && self.editing_message.is_none() && user_inputted_text {
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
        } else if user_inputted_text {
            let current_user_id = state.client().user_id();
            let should_send_typing = state.cache.get_guild(guild_id).map_or(false, |guild| {
                self.get_typing_members(state, guild)
                    .any(|(id, _)| id == current_user_id)
                    .not()
            });
            if should_send_typing {
                spawn_client_fut!(state, |client| {
                    client.send_typing(guild_id, channel_id).await?;
                });
            }
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
        guard!(let Some(guild_id) = self.current.guild() else { return });
        guard!(let Some(guild) = state.cache.get_guild(guild_id) else { return });

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let sorted_members = Self::sort_members(state, guild);
            if sorted_members.is_empty().not() {
                for (id, _) in sorted_members {
                    guard!(let Some(user) = state.cache.get_user(*id) else { continue });
                    ui.horizontal(|ui| {
                        if user.fetched {
                            self.view_user_avatar(state, ui, Some(user), None);
                            ui.label(user.username.as_str());
                        } else {
                            ui.add(egui::Spinner::new().size(32.0));
                        }
                    });
                    ui.separator();
                }
            } else {
                ui.add_sized(ui.available_size(), egui::Spinner::new().size(32.0))
                    .on_hover_text_at_pointer("loading members");
            }
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

    fn get_typing_members<'a>(
        &'a self,
        state: &'a State,
        guild: &'a Guild,
    ) -> impl Iterator<Item = (u64, &'a Member)> + 'a {
        guild
            .members
            .keys()
            .filter_map(move |uid| Some((*uid, state.cache.get_user(*uid)?)))
            .filter_map(|member| Some((member, member.1.typing_in_channel?)))
            .filter_map(move |(member, (gid, cid, _))| self.current.is_channel(gid, cid).then(|| member))
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame, state: &mut State) {
        if ctx.input().key_pressed(egui::Key::Escape) {
            self.editing_message = None;
        }

        if ctx.input().key_pressed(egui::Key::ArrowUp) {
            let maybe_channel = self
                .current
                .channel()
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

        egui::Window::new("join guild")
            .open(&mut self.show_join_guild.borrow_mut())
            .show(ctx, |ui| {
                self.view_join_guild(state, ui);
            });

        egui::Window::new("create guild")
            .open(&mut self.show_create_guild.borrow_mut())
            .show(ctx, |ui| {
                self.view_create_guild(state, ui);
            });

        egui::panel::SidePanel::left("guild_panel")
            .min_width(32.0)
            .max_width(32.0)
            .resizable(false)
            .show(ctx, |ui| self.view_guilds(state, ui));

        if self.current.has_guild() {
            egui::panel::SidePanel::left("channel_panel")
                .min_width(100.0)
                .max_width(300.0)
                .default_width(150.0)
                .resizable(true)
                .show(ctx, |ui| {
                    self.view_channels(state, ui);
                });

            if !self.disable_users_bar {
                egui::panel::SidePanel::right("member_panel")
                    .min_width(100.0)
                    .max_width(300.0)
                    .default_width(150.0)
                    .resizable(true)
                    .show(ctx, |ui| {
                        self.view_members(state, ui);
                    });
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let chan_name = self
                .current
                .channel()
                .and_then(|(gid, cid)| state.cache.get_channel(gid, cid))
                .map_or_else(|| "select a channel".to_string(), |c| format!("#{}", c.name));

            if self.current.has_guild() {
                egui::TopBottomPanel::top("central_top_panel")
                    .resizable(false)
                    .min_height(12.0)
                    .max_height(12.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(chan_name);
                            ui.separator();
                            ui.add_space(ui.available_width() - 12.0);
                            let show_members_but = ui
                                .add_sized([12.0, 12.0], egui::Button::new("👤").frame(false).small())
                                .on_hover_text("toggle member list");
                            if show_members_but.clicked() {
                                self.disable_users_bar = !self.disable_users_bar;
                            }
                        });
                    });

                if self.current.has_channel() {
                    ui.with_layout(
                        Layout::from_main_dir_and_cross_align(egui::Direction::LeftToRight, egui::Align::Center),
                        |ui| {
                            ui.vertical(|ui| {
                                ui.allocate_ui([ui.available_width(), ui.available_height() - 38.0].into(), |ui| {
                                    self.view_messages(state, ui, frame);
                                });
                                ui.group(|ui| {
                                    self.view_typing_members(state, ui);
                                    self.view_composer(state, ui, ctx);
                                });
                            });
                        },
                    );
                }
            }
        });

        self.prev_editing_message = self.editing_message;
    }
}
