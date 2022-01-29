use std::{ops::Not, path::PathBuf, sync::Arc};

use client::{
    channel::Channel,
    content,
    guild::Guild,
    harmony_rust_sdk::api::{chat::all_permissions, exports::prost::bytes::Bytes, rest::FileId},
    member::Member,
    message::{Attachment, Content, Embed, EmbedHeading, Message, MessageId, Override, ReadMessagesView},
    smol_str::SmolStr,
    AHashMap, AHashSet, FetchEvent, Uri,
};
use eframe::egui::{vec2, Color32, Event, RichText, Vec2};

use crate::{
    config::BgImage,
    futures::UploadMessageResult,
    screen::guild_settings,
    style as loqui_style,
    widgets::{
        bg_image::ImageBg,
        easy_mark::{self, EasyMarkEditor},
        view_channel_context_menu_items, view_member_context_menu_items, Avatar, TextButton,
    },
};

use super::{prelude::*, settings};

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
    #[allow(dead_code)]
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Messages,
    GuildChannels,
    Members,
}

impl Default for Panel {
    fn default() -> Self {
        Self::GuildChannels
    }
}

struct PanelStack {
    inner: Vec<Panel>,
}

impl PanelStack {
    fn push(&mut self, panel: Panel) {
        self.inner.push(panel);
    }

    fn pop(&mut self) -> Option<Panel> {
        self.inner.pop()
    }

    fn current(&self) -> Panel {
        self.inner.last().copied().expect("must have panel -- this is a bug")
    }
}

impl Default for PanelStack {
    fn default() -> Self {
        Self {
            inner: vec![Panel::default()],
        }
    }
}

#[derive(Default)]
pub struct Screen {
    /// guild id -> channel id
    last_channel_id: AHashMap<u64, u64>,
    /// (guild id, channel id, message id)
    /// if exists on this map, then dont show the message with rich text
    dont_highlight_message: AHashSet<(u64, u64, MessageId)>,
    /// file id -> bool
    /// if `true`, then we are (down)loading this attachment
    loading_attachment: AHashMap<FileId, AtomBool>,
    /// current guild / channel
    current: CurrentIds,
    /// main message composer
    composer: EasyMarkEditor,
    /// was the main composer focused in last frame
    is_composer_focused: bool,
    /// whether to focus the main composer in the next frame
    focus_composer: bool,
    /// composer used for editing messages
    edit_message_composer: EasyMarkEditor,
    /// whether to scroll to bottom on next frame
    scroll_to_bottom: bool,
    /// acked message id
    /// the message the user is currently editing (or not)
    editing_message: Option<u64>,
    /// acked message id
    /// the message the user was editing before (or not)
    prev_editing_message: Option<u64>,
    /// whether to show guild members list
    disable_users_bar: bool,
    /// animate bool for typing anim
    typing_animating: bool,

    /// guild join window ///
    /// whether to show join guild window
    show_join_guild: bool,
    /// invite id for the guild to join
    invite_text: String,

    /// guild create window ///
    show_create_guild: bool,
    /// guild name we want to create with
    guild_name_text: String,

    /// pinned messages window ///
    /// whether to show pinned messages window
    show_pinned_messages: bool,

    /// panel stack keeping track of where we are / were
    panel_stack: PanelStack,
}

impl Screen {
    fn is_fetching_attachment(&self, attachment: &Attachment) -> bool {
        self.loading_attachment
            .get(&attachment.id)
            .map_or_else(|| attachment.is_thumbnail(), AtomBool::get)
    }

    fn download_file(&mut self, state: &State, attachment: Attachment) {
        let image_load_bool = AtomBool::new(true);
        self.loading_attachment
            .insert(attachment.id.clone(), image_load_bool.clone());

        spawn_evs!(state, |sender, client| {
            let res = client.fetch_attachment(attachment.id.clone()).await;
            image_load_bool.set(false);
            let (_, file) = res?;
            let _ = sender.send(FetchEvent::Attachment { attachment, file });
            ClientResult::Ok(())
        });
    }

    /// toggle panel, if we are currently on the given panel then pop
    /// otherwise push the panel to the stack
    fn toggle_panel(&mut self, panel: Panel) {
        if self.panel_stack.current() == panel {
            self.panel_stack.pop();
        } else {
            self.panel_stack.push(panel);
        }
    }

    fn view_pinned_messages(&mut self, state: &State, ctx: &egui::Context) {
        let Some((guild_id, channel_id)) = self.current.channel() else { return };
        let Some(channel) = state.cache.get_channel(guild_id, channel_id) else { return };
        let Some(guild) = state.cache.get_guild(guild_id) else { return };
        let user_id = state.client().user_id();

        let mut show_pinned_messages = self.show_pinned_messages;

        egui::Window::new("pinned messages")
            .open(&mut show_pinned_messages)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom().show(ui, |ui| {
                    for id in &channel.pinned_messages {
                        let id = MessageId::Ack(*id);
                        let Some(message) = channel.messages.view().get_message(&id) else { continue };
                        self.view_message(
                            state, ui, guild, channel, message, guild_id, channel_id, &id, user_id, true,
                        );
                    }
                });
            });

        self.show_pinned_messages = show_pinned_messages;
    }

    fn view_join_guild(&mut self, state: &mut State, ctx: &egui::Context) {
        let invite_text = &mut self.invite_text;
        egui::Window::new("join guild")
            .auto_sized()
            .collapsible(false)
            .open(&mut self.show_join_guild)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.text_edit_singleline(invite_text);
                    ui.add_space(6.0);

                    let enabled = invite_text.is_empty().not();
                    if ui.add_enabled(enabled, egui::Button::new("join")).clicked() {
                        let invite_id = invite_text.clone();
                        spawn_client_fut!(state, |client| client.join_guild(invite_id).await);
                    }
                });
            });
    }

    fn view_create_guild(&mut self, state: &mut State, ctx: &egui::Context) {
        let guild_name_text = &mut self.guild_name_text;
        egui::Window::new("create guild")
            .auto_sized()
            .collapsible(false)
            .open(&mut self.show_create_guild)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.text_edit_singleline(guild_name_text);
                    ui.add_space(6.0);

                    let enabled = guild_name_text.is_empty().not();
                    if ui.add_enabled(enabled, egui::Button::new("create")).clicked() {
                        let guild_name = guild_name_text.clone();
                        spawn_client_fut!(state, |client| client.create_guild(guild_name).await);
                    }
                });
            });
    }

    fn view_guilds(&mut self, state: &mut State, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if state.cache.is_initial_sync_complete().not() {
                ui.add(egui::Spinner::new().size(32.0)).on_hover_text("loading guilds");
                ui.separator();
            }

            for (guild_id, guild) in state.cache.get_guilds() {
                let is_enabled = guild.fetched && self.current.guild() != Some(guild_id);

                let button = ui
                    .add_enabled_ui(is_enabled, |ui| {
                        if guild.fetched {
                            let mut avatar = Avatar::new(guild.picture.as_ref(), guild.name.as_str(), state);
                            if !is_enabled {
                                avatar = avatar.fill_bg(loqui_style::BG_LIGHT);
                            }
                            ui.add(avatar)
                        } else {
                            ui.add(egui::Spinner::new().size(32.0))
                        }
                    })
                    .inner
                    .on_disabled_hover_text(guild.fetched.then(|| guild.name.as_str()).unwrap_or("loading guild"))
                    .on_hover_text(guild.name.as_str())
                    .context_menu_styled(|ui| {
                        if ui.button(dangerous_text("leave guild")).clicked() {
                            spawn_client_fut!(state, |client| client.leave_guild(guild_id).await);
                            ui.close_menu();
                        }
                    });

                if button.clicked() {
                    self.current.set_guild(guild_id);
                    if let Some(channel_id) = self.last_channel_id.get(&guild_id) {
                        self.current.set_channel(*channel_id);
                    }
                    if guild.channels.is_empty() && guild.members.is_empty() {
                        spawn_evs!(state, |events, c| {
                            c.fetch_channels(guild_id, events).await?;
                        });
                        spawn_evs!(state, |events, c| {
                            c.fetch_guild_perms(guild_id, events).await?;
                        });
                        spawn_evs!(state, |events, c| {
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
                self.show_join_guild = true;
            }

            ui.separator();

            let create_but = ui
                .add_sized([32.0, 32.0], egui::Button::new(RichText::new("c+").strong()))
                .on_hover_text("create guild");
            if create_but.clicked() {
                self.show_create_guild = true;
            }
        });
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        let Some(guild_id) = self.current.guild() else { return };

        let guild_name = state
            .cache
            .get_guild(guild_id)
            .map_or_else(|| "unknown", |g| g.name.as_str());

        let menu_but_clicked = egui::Frame::group(ui.style())
            .margin([2.0, 2.0])
            .show(ui, |ui| {
                let but = ui
                    .add(TextButton::text(guild_name).small())
                    .on_hover_text("open guild settings");

                but.clicked()
            })
            .inner;

        if menu_but_clicked {
            state.push_screen(guild_settings::Screen::new(guild_id, state));
        }

        let maybe_guild = state.cache.get_guild(guild_id);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let channels = state.cache.get_channels(guild_id);

            if channels.is_empty().not() {
                for (channel_id, channel) in channels {
                    let symbol = channel.is_category.then(|| "☰ ").unwrap_or("#");
                    let text = format!("{}{}", symbol, channel.name);

                    let is_enabled = (channel.is_category || self.current.is_channel(guild_id, channel_id)).not();
                    let mut button = ui.add_enabled(is_enabled, TextButton::text(text));
                    if let Some(guild) = maybe_guild {
                        button = button.context_menu_styled(|ui| {
                            view_channel_context_menu_items(ui, state, guild_id, channel_id, guild, channel);
                        });
                    }
                    if button.clicked() {
                        self.toggle_panel(Panel::Messages);
                        self.current.set_channel(channel_id);
                        self.last_channel_id.insert(guild_id, channel_id);
                        if channel.fetched_msgs_pins.not()
                            && channel.reached_top.not()
                            && channel.messages.continuous_view().is_empty()
                        {
                            spawn_evs!(state, |events, c| {
                                c.fetch_messages(guild_id, channel_id, events).await?;
                                let _ = events.send(FetchEvent::FetchedMsgsPins(guild_id, channel_id));
                            });
                        }
                        if channel.fetched_msgs_pins.not() && channel.pinned_messages.is_empty() {
                            spawn_evs!(state, |events, c| {
                                c.fetch_pinned_messages(guild_id, channel_id, events).await?;
                                let _ = events.send(FetchEvent::FetchedMsgsPins(guild_id, channel_id));
                            });
                        }
                        self.scroll_to_bottom = true;
                    }
                }
            } else {
                ui.add_sized(ui.available_size(), egui::Spinner::new().size(32.0))
                    .on_hover_text_at_pointer("loading channels");
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn view_message_text_content(
        &mut self,
        state: &State,
        ui: &mut Ui,
        id: &MessageId,
        guild_id: u64,
        channel_id: u64,
        text: &str,
        is_failed: bool,
    ) {
        ui.scope(|ui| {
            let weak_text = ui.visuals().weak_text_color();
            let strong_text = ui.visuals().strong_text_color().linear_multiply(0.35);
            let color = is_failed
                .then(|| Color32::RED)
                .or_else(|| id.is_ack().then(|| strong_text))
                .unwrap_or(weak_text);
            ui.style_mut().visuals.override_text_color = Some(color);

            let highlight_message = self.dont_highlight_message.contains(&(guild_id, channel_id, *id)).not();

            if id.is_ack() && id.id() == self.editing_message {
                let edit = self.edit_message_composer.highlight(highlight_message).editor_ui(ui);
                let is_pressed = ui.input().key_pressed(egui::Key::Enter) && !ui.input().modifiers.shift;
                if self.prev_editing_message.is_none() {
                    edit.request_focus();
                }
                let trimmed_edit_msg = self.edit_message_composer.text().trim();
                if trimmed_edit_msg.is_empty().not() && edit.has_focus() && is_pressed {
                    let text = trimmed_edit_msg.to_string();
                    let message_id = id.id().unwrap();
                    self.editing_message = None;
                    spawn_client_fut!(state, |client| {
                        client.edit_message(guild_id, channel_id, message_id, text).await
                    });
                }
            } else if highlight_message {
                let urls = parse_urls(text);
                let mut text = text.to_string();
                for (source, _) in urls {
                    text = text.replace(source, &format!("<{}>", source));
                }
                easy_mark::easy_mark(ui, &text);
            } else {
                ui.label(text);
            }
        });
    }

    fn view_message_url_embeds(&mut self, state: &State, ui: &mut Ui, text: &str) {
        let urls = parse_urls(text).filter_map(|(og, url)| Some((state.cache.get_link_data(&url)?, url, og)));
        for (data, url, raw_url) in urls {
            match data {
                client::harmony_rust_sdk::api::mediaproxy::fetch_link_metadata_response::Data::IsSite(data) => {
                    let id = FileId::External(data.image.parse::<Uri>().unwrap_or(url));
                    let has_site_title = data.site_title.is_empty().not();
                    let has_page_title = data.page_title.is_empty().not();
                    let has_desc = data.description.is_empty().not();
                    let maybe_thumbnail = state.image_cache.get_image(&id);

                    if has_site_title || has_page_title || has_desc {
                        ui.group(|ui| {
                            let factor = ui
                                .is_mobile()
                                .then(|| 0.95)
                                .unwrap_or_else(|| (ui.available_width() > 1440.0).then(|| 0.45).unwrap_or(0.7));
                            ui.set_max_width(ui.available_width() * factor);

                            if has_site_title {
                                let but_resp = ui
                                    .add(TextButton::text(RichText::new(&data.site_title).small()).small())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if but_resp.clicked() {
                                    open_url(raw_url.to_string());
                                }
                            }
                            if has_page_title {
                                let but_resp = ui
                                    .add(TextButton::text(RichText::new(&data.page_title).strong()).small())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if but_resp.clicked() {
                                    open_url(raw_url.to_string());
                                }
                            }
                            if has_site_title || has_page_title {
                                ui.separator();
                            }
                            if has_desc {
                                ui.label(&data.description);
                                ui.separator();
                            }
                            if let Some((tex, size)) = maybe_thumbnail {
                                let size = ui.downscale_to(size, 1.0);
                                ui.image(tex.id(), size);
                            }
                        });
                    }
                }
                client::harmony_rust_sdk::api::mediaproxy::fetch_link_metadata_response::Data::IsMedia(data) => {
                    let id = FileId::External(url);
                    let attachment = Attachment {
                        name: data.filename.clone(),
                        kind: data.mimetype.clone(),
                        // we dont want the attachment to count as thumbnail
                        size: u32::MAX,
                        ..Attachment::new_unknown(id)
                    };

                    let mut download = false;
                    let mut open = false;
                    let is_fetching = self.is_fetching_attachment(&attachment);

                    if is_fetching.not() {
                        if attachment.is_raster_image() {
                            if let Some((tex, size)) = state.image_cache.get_image(&attachment.id) {
                                let size = ui.downscale(size);
                                let open_but = ui.add(egui::ImageButton::new(tex.id(), size));
                                open = open_but.clicked();
                            } else {
                                download = ui.button(format!("download '{}'", data.filename)).clicked();
                            }
                        } else {
                            open = ui.button(format!("open '{}'", data.filename)).clicked();
                        }
                    } else {
                        ui.add(egui::Spinner::new())
                            .on_hover_text(format!("downloading '{}'", data.filename));
                    }

                    if open {
                        open_url(raw_url.to_string());
                    }
                    if download {
                        self.download_file(state, attachment);
                    }
                }
            }
        }
    }

    fn view_message_attachment(&mut self, state: &State, ui: &mut Ui, attachment: &Attachment) {
        let mut handled = false;
        let mut fetch = false;
        let mut open = false;

        if attachment.is_raster_image() {
            let mut no_thumbnail = false;

            let maybe_size = attachment
                .resolution
                .and_then(|(w, h)| (w == 0 || h == 0).not().then(|| ui.downscale([w as f32, h as f32])));

            let is_fetching = self.is_fetching_attachment(attachment);

            if let Some((texid, size)) = state.image_cache.get_image(&attachment.id) {
                let size = maybe_size.unwrap_or_else(|| ui.downscale(size));
                let open_but = ui.add(egui::ImageButton::new(texid.id(), size));
                open = open_but.clicked();
                handled = true;
            } else if let Some((texid, size)) = state.image_cache.get_thumbnail(&attachment.id) {
                let size = maybe_size.unwrap_or_else(|| ui.downscale(size));
                let button = if is_fetching {
                    ImageBg::new(texid.id(), size)
                        .show(ui, |ui| {
                            ui.add_sized(size, egui::Spinner::new().size(32.0))
                                .on_hover_text_at_pointer("loading image")
                        })
                        .response
                } else {
                    ui.add(egui::ImageButton::new(texid.id(), size))
                };
                fetch = button.clicked();
                handled = true;
            } else if let Some(size) = maybe_size {
                let button = if is_fetching {
                    ui.add_sized(size, egui::Spinner::new().size(32.0))
                        .on_hover_text_at_pointer("loading image")
                } else {
                    ui.add_sized(size, egui::Button::new(format!("download '{}'", attachment.name)))
                };
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
                crate::image_cache::op::decode_image(data, id, "minithumbnail".to_string());
            }
        }

        if !handled {
            open = ui.button(format!("open '{}'", attachment.name)).clicked();
        }

        if fetch {
            self.download_file(state, attachment.clone());
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

    #[allow(clippy::too_many_arguments)]
    fn view_message(
        &mut self,
        state: &State,
        ui: &mut Ui,
        guild: &Guild,
        channel: &Channel,
        message: &Message,
        guild_id: u64,
        channel_id: u64,
        id: &MessageId,
        user_id: u64,
        display_user: bool,
    ) {
        let msg = ui
            .scope(|ui| {
                let user = state.cache.get_user(message.sender);

                if display_user {
                    let overrides = message.overrides.as_ref();
                    let override_name = overrides.and_then(|ov| ov.name.as_ref().map(SmolStr::as_str));
                    let sender_name = user.map_or_else(|| "unknown", |u| u.username.as_str());
                    let display_name = override_name.unwrap_or(sender_name);

                    let color = guild
                        .highest_role_for_member(message.sender)
                        .map_or(Color32::WHITE, |(_, role)| rgb_color(role.color));

                    let user_resp = ui
                        .scope(|ui| {
                            ui.horizontal(|ui| {
                                let extreme_bg_color = ui.style().visuals.extreme_bg_color;
                                self.view_user_avatar(state, ui, user, overrides, extreme_bg_color);
                                ui.label(RichText::new(display_name).color(color).strong());
                                if override_name.is_some() {
                                    ui.label(RichText::new(format!("({})", sender_name)).italics().small());
                                }
                            });
                        })
                        .response;

                    if let Some(user) = user {
                        user_resp.context_menu_styled(|ui| {
                            view_member_context_menu_items(ui, state, guild_id, message.sender, guild, user);
                        });
                    }
                }

                match &message.content {
                    client::message::Content::Text(text) => {
                        self.view_message_text_content(
                            state,
                            ui,
                            id,
                            guild_id,
                            channel_id,
                            text,
                            message.failed_to_send,
                        );
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

        msg.context_menu_styled(|ui| {
            if let Some(message_id) = id.id() {
                if let client::message::Content::Text(text) = &message.content {
                    if channel.has_perm(all_permissions::MESSAGES_SEND)
                        && message.sender == user_id
                        && ui.button("edit").clicked()
                    {
                        self.editing_message = id.id();
                        let edit_text = self.edit_message_composer.text_mut();
                        edit_text.clear();
                        edit_text.push_str(text);
                        ui.close_menu();
                    }
                    if ui.button("quote reply").clicked() {
                        let composer_text = self.composer.text_mut();
                        composer_text.clear();
                        composer_text.push_str("> ");
                        composer_text.push_str(text);
                        composer_text.push('\n');
                        self.focus_composer = true;
                        ui.close_menu();
                    }
                    if ui.button("copy").clicked() {
                        ui.output().copied_text = text.clone();
                        ui.close_menu();
                    }
                }
                if message.sender == state.client().user_id() && ui.button(dangerous_text("delete")).clicked() {
                    spawn_client_fut!(state, |client| {
                        client.delete_message(guild_id, channel_id, message_id).await
                    });
                    ui.close_menu();
                }
                if channel.pinned_messages.contains(&message_id) {
                    if channel.has_perm(all_permissions::MESSAGES_PINS_REMOVE) && ui.button("unpin").clicked() {
                        spawn_client_fut!(state, |client| {
                            client.unpin_message(guild_id, channel_id, message_id).await
                        });
                        ui.close_menu();
                    }
                } else if channel.has_perm(all_permissions::MESSAGES_PINS_ADD) && ui.button("pin").clicked() {
                    spawn_client_fut!(state, |client| {
                        client.pin_message(guild_id, channel_id, message_id).await
                    });
                    ui.close_menu();
                }
                if ui.button("toggle highlighting").clicked() {
                    let key = (guild_id, channel_id, *id);
                    let is_highlighted = self.dont_highlight_message.contains(&key).not();
                    if is_highlighted {
                        self.dont_highlight_message.insert(key);
                    } else {
                        self.dont_highlight_message.remove(&key);
                    }
                    ui.close_menu();
                }
            }
        });
    }

    fn view_messages(&mut self, state: &State, ui: &mut Ui) {
        let Some((guild_id, channel_id)) = self.current.channel() else { return };
        let Some(channel) = state.cache.get_channel(guild_id, channel_id) else { return };
        let Some(guild) = state.cache.get_guild(guild_id) else { return };
        let user_id = state.client().user_id();

        egui::ScrollArea::vertical()
            .stick_to_bottom()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let all_messages = channel.messages.continuous_view().all_messages();
                let chunked_messages =
                    all_messages
                        .into_iter()
                        .fold(vec![Vec::<(&MessageId, &Message)>::new()], |mut tot, (id, msg)| {
                            let last_chunk = tot.last().unwrap();

                            let is_same_author = last_chunk.last().map_or(false, |(_, omsg)| omsg.sender == msg.sender);
                            let is_same_display_name = last_chunk.last().map_or(false, |(_, omsg)| {
                                let odisp = state.get_member_display_name(omsg);
                                let disp = state.get_member_display_name(msg);
                                odisp == disp
                            });
                            let is_chunk_big = last_chunk.len() > 5;

                            if is_same_author && is_same_display_name && is_chunk_big.not() {
                                tot.last_mut().unwrap().push((id, msg));
                            } else {
                                tot.push(vec![(id, msg)]);
                            }
                            tot
                        });
                for chunk in chunked_messages {
                    ui.group_filled_with(loqui_style::BG_LIGHT)
                        .stroke((0.0, Color32::WHITE).into())
                        .margin([5.0, 5.0])
                        .show(ui, |ui| {
                            for (index, (id, message)) in chunk.into_iter().enumerate() {
                                self.view_message(
                                    state,
                                    ui,
                                    guild,
                                    channel,
                                    message,
                                    guild_id,
                                    channel_id,
                                    id,
                                    user_id,
                                    index == 0,
                                );
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
        let Some(guild_id) = self.current.guild() else { return };
        let Some(guild) = state.cache.get_guild(guild_id) else { return };

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

    fn view_composer(&mut self, state: &mut State, ui: &mut Ui, ctx: &egui::Context, desired_lines: usize) {
        let Some((guild_id, channel_id)) = self.current.channel() else { return };

        let text_edit = self
            .composer
            .desired_rows(desired_lines)
            .desired_width(ui.available_width() - 36.0)
            .hint_text("Enter message...")
            .editor_ui(ui);

        let user_inputted_text = {
            let input = ctx.input();
            let has_text = input.events.iter().any(|ev| matches!(ev, Event::Text(_)));
            let any_modifier = input.modifiers.alt || input.modifiers.command;
            has_text && any_modifier.not()
        };
        let should_focus_composer = (self.show_create_guild || self.show_join_guild).not()
            && text_edit.has_focus().not()
            && ctx.wants_keyboard_input().not()
            && user_inputted_text;

        if should_focus_composer {
            for event in ctx.input().events.iter() {
                if let Event::Text(text) = event {
                    self.composer.text_mut().push_str(text);
                }
            }
            text_edit.request_focus();
        }

        if self.focus_composer {
            text_edit.request_focus();
            self.focus_composer = false;
        }

        let is_pressed = {
            let input = ui.input();
            input.key_pressed(egui::Key::Enter) && !input.modifiers.shift
        };
        if self.composer.text().trim().is_empty().not() && text_edit.has_focus() && is_pressed {
            let text_string = self.composer.text().trim().to_string();
            self.composer.text_mut().clear();
            let message = Message {
                content: Content::Text(text_string),
                sender: state.client().user_id(),
                ..Default::default()
            };
            let echo_id = state.cache.prepare_send_message(guild_id, channel_id, message.clone());
            spawn_evs!(state, |evs, client| {
                client.send_message(echo_id, guild_id, channel_id, message, evs).await?;
            });
            self.scroll_to_bottom = true;
            text_edit.surrender_focus();
        } else if user_inputted_text {
            let current_user_id = state.client().user_id();
            let should_send_typing = state.cache.get_guild(guild_id).map_or(false, |guild| {
                self.get_typing_members(state, guild)
                    .any(|(id, _)| id == current_user_id)
                    .not()
            });
            if should_send_typing {
                spawn_client_fut!(state, |client| client.send_typing(guild_id, channel_id).await);
            }
        }

        self.is_composer_focused = text_edit.has_focus();
    }

    fn view_uploading_attachments(&mut self, state: &State, ui: &mut Ui) {
        ui.label(RichText::new("Uploading:").strong());
        for name in state.uploading_files.read().expect("poisoned").iter() {
            egui::Frame::group(ui.style()).margin([0.0; 2]).show(ui, |ui| {
                ui.label(name);
            });
        }
    }

    fn view_upload_button(&mut self, state: &State, ui: &mut Ui) {
        let Some((guild_id, channel_id)) = self.current.channel() else { return };

        let resp = ui.button("^").on_hover_text("upload file(s)");
        if resp.clicked() {
            let uploading_files = state.uploading_files.clone();
            spawn_client_fut!(state, |client| {
                let files = rfd::AsyncFileDialog::new().pick_files().await;
                if let Some(files) = files {
                    {
                        let mut guard = uploading_files.write().expect("poisoned");
                        for file in files.iter() {
                            guard.push(file.file_name());
                        }
                    }
                    let mut attachments = Vec::with_capacity(files.len());
                    for file in files {
                        let data = file.read().await;
                        let mimetype = content::infer_type_from_bytes(&data);
                        let size = data.len() as u32;
                        // TODO: return errors for files that failed to upload
                        let id = client.upload_file(file.file_name(), mimetype.clone(), data).await?;
                        attachments.push(Attachment {
                            id,
                            kind: mimetype,
                            size,
                            name: file.file_name(),
                            minithumbnail: None,
                            resolution: None,
                        });
                    }
                    ClientResult::Ok(Some(UploadMessageResult {
                        guild_id,
                        channel_id,
                        attachments,
                    }))
                } else {
                    Ok(None)
                }
            });
        }
    }

    fn view_user_avatar(
        &mut self,
        state: &State,
        ui: &mut Ui,
        user: Option<&Member>,
        overrides: Option<&Override>,
        fill_bg: Color32,
    ) {
        let maybe_tex = overrides
            .and_then(|ov| ov.avatar_url.as_ref())
            .or_else(|| user.and_then(|u| u.avatar_url.as_ref()))
            .as_ref()
            .and_then(|id| state.image_cache.get_avatar(id));

        if let Some((texid, _)) = maybe_tex {
            ui.image(texid.id(), [32.0, 32.0]);
        } else {
            ui.add_enabled_ui(false, |ui| {
                let username = overrides
                    .and_then(|ov| ov.name.as_deref())
                    .or_else(|| user.map(|u| u.username.as_str()))
                    .unwrap_or("");
                let letter = username.get(0..1).unwrap_or("u").to_ascii_uppercase();

                ui.add_sized([32.0, 32.0], egui::Button::new(letter).fill(fill_bg));
            });
        }
    }

    fn view_members(&mut self, state: &State, ui: &mut Ui) {
        let Some(guild_id) = self.current.guild() else { return };
        let Some(guild) = state.cache.get_guild(guild_id) else { return };

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let sorted_members = sort_members(state, guild);
            if sorted_members.is_empty().not() {
                for (id, _) in sorted_members {
                    let Some(user) = state.cache.get_user(*id) else { continue };
                    let frame_resp = ui
                        .scope(|ui| {
                            ui.horizontal(|ui| {
                                if user.fetched {
                                    let role_color = guild
                                        .highest_role_for_member(*id)
                                        .map_or(Color32::WHITE, |(_, role)| rgb_color(role.color));
                                    self.view_user_avatar(state, ui, Some(user), None, loqui_style::BG_LIGHT);
                                    ui.colored_label(role_color, user.username.as_str());
                                } else {
                                    ui.add(egui::Spinner::new().size(32.0));
                                }
                            });
                        })
                        .response;
                    frame_resp.context_menu_styled(|ui| {
                        view_member_context_menu_items(ui, state, guild_id, *id, guild, user);
                    });
                    ui.separator();
                }
            } else {
                ui.add_sized(ui.available_size(), egui::Spinner::new().size(32.0))
                    .on_hover_text_at_pointer("loading members");
            }
        });
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

    #[inline(always)]
    fn handle_arrow_up_edit(&mut self, ctx: &egui::Context, state: &State) {
        if self.composer.text().is_empty()
            && self.editing_message.is_none()
            && ctx.input().key_pressed(egui::Key::ArrowUp)
        {
            let maybe_channel = self
                .current
                .channel()
                .and_then(|(gid, cid)| state.cache.get_channel(gid, cid));

            if let Some(chan) = maybe_channel {
                let user_id = state.client().user_id();
                let maybe_msg = chan
                    .messages
                    .continuous_view()
                    .all_messages()
                    .into_iter()
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
                    let edit_text = self.edit_message_composer.text_mut();
                    edit_text.clear();
                    edit_text.push_str(text);
                }
            }
        }
    }

    #[inline(always)]
    fn show_guild_panel(&mut self, ui: &mut Ui, state: &mut State) {
        let panel_frame = egui::Frame {
            margin: Vec2::new(8.0, 5.0),
            fill: ui.style().visuals.extreme_bg_color,
            stroke: ui.style().visuals.window_stroke(),
            corner_radius: 4.0,
            ..Default::default()
        };

        egui::panel::SidePanel::left("guild_panel")
            .frame(panel_frame)
            .min_width(32.0)
            .max_width(32.0)
            .resizable(false)
            .show_inside(ui, |ui| self.view_guilds(state, ui));
    }

    #[inline(always)]
    fn show_channel_panel(&mut self, ui: &mut Ui, state: &mut State) {
        let panel_frame = egui::Frame {
            margin: Vec2::new(8.0, 5.0),
            fill: ui.style().visuals.window_fill(),
            stroke: ui.style().visuals.window_stroke(),
            corner_radius: 4.0,
            ..Default::default()
        };

        let panel = egui::panel::SidePanel::left("channel_panel").frame(panel_frame);

        let panel = if ui.ctx().is_mobile() {
            panel.resizable(false).min_width(ui.available_width() - 16.0)
        } else {
            panel
                .min_width(100.0)
                .max_width(300.0)
                .default_width(125.0)
                .resizable(true)
        };

        panel.show_inside(ui, |ui| {
            self.view_channels(state, ui);
        });
    }

    #[inline(always)]
    fn show_member_panel(&mut self, ui: &mut Ui, state: &mut State) {
        let panel_frame = egui::Frame {
            margin: Vec2::new(8.0, 5.0),
            fill: ui.style().visuals.extreme_bg_color,
            stroke: ui.style().visuals.window_stroke(),
            corner_radius: 4.0,
            ..Default::default()
        };

        let panel = egui::panel::SidePanel::right("member_panel").frame(panel_frame);

        let panel = if ui.ctx().is_mobile() {
            panel.resizable(false).min_width(ui.available_width() - 16.0)
        } else {
            panel
                .min_width(100.0)
                .max_width(300.0)
                .default_width(125.0)
                .resizable(true)
        };

        panel.show_inside(ui, |ui| {
            self.view_members(state, ui);
        });
    }

    #[inline(always)]
    fn show_channel_bar(&mut self, ui: &mut Ui, state: &mut State) {
        let interact_size = ui.style().spacing.interact_size;
        let top_channel_bar_width = ui.available_width()
            - 8.0
            - self.current.has_guild().then(|| 6.0).unwrap_or(0.0)
            - self.current.has_channel().then(|| 6.0).unwrap_or(0.0);
        let is_mobile = ui.ctx().is_mobile();

        ui.allocate_ui([top_channel_bar_width, interact_size.y].into(), |ui| {
            let frame = egui::Frame {
                margin: [4.0, 2.0].into(),
                fill: ui.style().visuals.window_fill(),
                stroke: ui.style().visuals.window_stroke(),
                corner_radius: 2.0,
                ..Default::default()
            };
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.style_mut().spacing.item_spacing = egui::Vec2::ZERO;

                    if self.current.has_channel() && is_mobile {
                        let show_guilds_but = ui
                            .add_sized([12.0, interact_size.y], TextButton::text("☰").small())
                            .on_hover_text("show guilds / channels");
                        if show_guilds_but.clicked() {
                            self.toggle_panel(Panel::GuildChannels);
                        }
                        ui.add_sized([8.0, interact_size.y], egui::Separator::default().spacing(4.0));
                    }

                    let chan_name = self
                        .current
                        .channel()
                        .and_then(|(gid, cid)| state.cache.get_channel(gid, cid))
                        .map_or_else(|| "select a channel".to_string(), |c| format!("#{}", c.name));

                    ui.label(RichText::new(chan_name).strong());
                    ui.add_sized([8.0, interact_size.y], egui::Separator::default().spacing(4.0));

                    let mut offset = 0.0;
                    if self.current.has_channel() {
                        offset += 12.0;
                    }
                    if self.current.has_guild() {
                        offset += 12.0;
                    }
                    if is_mobile {
                        offset += 12.0;
                    }
                    ui.offsetw(offset);

                    if self.current.has_guild() {
                        let show_members_but = ui
                            .add_sized([12.0, interact_size.y], TextButton::text("👤"))
                            .on_hover_text("toggle member list");
                        if show_members_but.clicked() {
                            self.toggle_panel(Panel::Members);
                            self.disable_users_bar = !self.disable_users_bar;
                        }
                    }

                    if self.current.has_channel() {
                        let pinned_msgs_but = ui
                            .add_sized([12.0, interact_size.y], TextButton::text("📌"))
                            .on_hover_text("show pinned messages");
                        if pinned_msgs_but.clicked() {
                            self.show_pinned_messages = true;
                        }
                    }

                    if is_mobile {
                        let settings_but = ui
                            .add_sized([12.0, interact_size.y], TextButton::text("⚙"))
                            .on_hover_text("settings");
                        if settings_but.clicked() {
                            state.push_screen(settings::Screen::new(ui.ctx(), state));
                        }
                    }
                });
            });
        });
    }

    #[inline(always)]
    fn show_main_area(&mut self, ui: &mut Ui, state: &mut State, ctx: &egui::Context) {
        ui.with_layout(
            Layout::from_main_dir_and_cross_align(egui::Direction::LeftToRight, egui::Align::Center),
            |ui| {
                ui.vertical(|ui| {
                    let uploading_any_files = state.uploading_files.read().expect("poisoned").is_empty().not();
                    if uploading_any_files {
                        egui::TopBottomPanel::top("uploading_panel").show_inside(ui, |ui| {
                            ui.horizontal(|ui| self.view_uploading_attachments(state, ui));
                        });
                    }

                    let (desired_lines, extra_offset) = self.is_composer_focused.then(|| (4, 0.0)).unwrap_or((1, 14.0));
                    let desired_height = (desired_lines as f32 * ui.style().spacing.interact_size.y) + extra_offset;
                    ui.allocate_ui(
                        vec2(ui.available_width(), ui.available_height() - desired_height),
                        |ui| {
                            self.view_messages(state, ui);
                        },
                    );

                    ui.group_filled().show(ui, |ui| {
                        self.view_typing_members(state, ui);
                        ui.horizontal(|ui| {
                            self.view_composer(state, ui, ctx, desired_lines);
                            self.view_upload_button(state, ui);
                        });
                    });
                });
            },
        );
    }

    fn handle_dropped_files(&self, ctx: &egui::Context, state: &State) {
        let Some((guild_id, channel_id)) = self.current.channel() else { return };

        enum DataType {
            Path(PathBuf),
            Bytes { filename: String, bytes: Arc<[u8]> },
        }

        for file in ctx.input_mut().raw.dropped_files.drain(..) {
            let data = file
                .bytes
                .map(|bytes| DataType::Bytes {
                    bytes,
                    filename: file.name,
                })
                .or_else(|| file.path.map(DataType::Path));

            if let Some(data) = data {
                let uploading_files = state.uploading_files.clone();
                spawn_client_fut!(state, |client| {
                    let (name, mimetype, data) = match data {
                        DataType::Path(path) => {
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let name = path
                                    .file_name()
                                    .map_or_else(|| "unknown".to_string(), |s| s.to_string_lossy().into_owned());
                                let data = tokio::task::spawn_blocking(move || std::fs::read(path).unwrap())
                                    .await
                                    .unwrap();
                                let mimetype = content::infer_type_from_bytes(&data);

                                (name, mimetype, data)
                            }
                            #[cfg(target_arch = "wasm32")]
                            {
                                unreachable!("wasm does not send path");
                            }
                        }
                        DataType::Bytes { filename, bytes } => {
                            let name = filename;
                            let data = bytes.to_vec();
                            let mimetype = content::infer_type_from_bytes(&data);

                            (name, mimetype, data)
                        }
                    };

                    {
                        let mut guard = uploading_files.write().expect("poisoned");
                        guard.push(name.clone());
                    }

                    let id = client.upload_file(name.clone(), mimetype.clone(), data).await?;

                    ClientResult::Ok(Some(UploadMessageResult {
                        guild_id,
                        channel_id,
                        attachments: vec![Attachment {
                            name,
                            kind: mimetype,
                            ..Attachment::new_unknown(id)
                        }],
                    }))
                });
            }
        }
    }
}

impl AppScreen for Screen {
    fn id(&self) -> &'static str {
        "main"
    }

    fn update(&mut self, ctx: &egui::Context, _: &epi::Frame, state: &mut State) {
        if ctx.input().key_pressed(egui::Key::Escape) {
            self.editing_message = None;
        }

        self.handle_dropped_files(ctx, state);
        self.handle_arrow_up_edit(ctx, state);

        self.view_join_guild(state, ctx);
        self.view_create_guild(state, ctx);
        self.view_pinned_messages(state, ctx);

        let panel_frame = egui::Frame {
            margin: Vec2::new(8.0, 8.0),
            fill: loqui_style::BG_LIGHT,
            ..Default::default()
        };
        let central_panel = egui::CentralPanel::default().frame(panel_frame);

        if ctx.is_mobile() {
            central_panel.show(ctx, |ui| {
                self.show_channel_bar(ui, state);

                if state.cache.is_initial_sync_complete() {
                    match self.panel_stack.current() {
                        Panel::Messages => self.show_main_area(ui, state, ctx),
                        Panel::Members => self.show_member_panel(ui, state),
                        Panel::GuildChannels => {
                            self.show_guild_panel(ui, state);
                            if self.current.has_guild() {
                                self.show_channel_panel(ui, state);
                            }
                        }
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Spinner::new().size(ui.style().spacing.interact_size.y * 4.0))
                    });
                }
            });
        } else {
            let mut show_main = |state: &mut State, ui: &mut Ui| {
                self.show_guild_panel(ui, state);

                if self.current.has_guild() {
                    self.show_channel_panel(ui, state);

                    if !self.disable_users_bar {
                        self.show_member_panel(ui, state);
                    }

                    self.show_channel_bar(ui, state);

                    if self.current.has_channel() {
                        self.show_main_area(ui, state, ctx);
                    }
                }
            };

            if state.cache.is_initial_sync_complete() {
                central_panel.show(ctx, |ui| match state.config.bg_image {
                    BgImage::None => show_main(state, ui),
                    BgImage::Default => {
                        let (texid, size) = state
                            .harmony_lotus
                            .as_ref()
                            .map(|(tex, size)| (tex.id(), *size))
                            .unwrap();
                        let size = size * 0.2;
                        ImageBg::new(texid, size)
                            .tint(Color32::WHITE.linear_multiply(0.05))
                            .offset(ui.available_size() - (size * 0.8) - vec2(75.0, 0.0))
                            .show(ui, |ui| show_main(state, ui));
                    }
                    _ => show_main(state, ui),
                });
            } else {
                central_panel.show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Spinner::new().size(ui.style().spacing.interact_size.y * 3.0));
                        ui.label(RichText::new("loading...").heading());
                    })
                });
            }
        }

        self.prev_editing_message = self.editing_message;
    }
}
