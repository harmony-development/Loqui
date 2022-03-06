use std::{lazy::OnceCell, ops::Not, path::PathBuf, sync::Arc};

use client::{
    channel::Channel,
    content,
    guild::Guild,
    harmony_rust_sdk::api::{
        chat::{
            all_permissions, attachment::Info, embed, send_message_request, Attachment, Embed, Overrides,
            SendMessageRequest,
        },
        exports::prost::bytes::Bytes,
        mediaproxy::fetch_link_metadata_response::metadata,
        profile::user_status,
    },
    member::Member,
    message::{AttachmentExt, Message, MessageId, ReadMessagesView},
    smol_str::SmolStr,
    AHashMap, AHashSet, FetchEvent, Uri,
};
use eframe::egui::{vec2, Color32, Event, RichText, Rounding, Vec2};
use egui::Margin;

use crate::{
    config::BgImage,
    futures::UploadMessageResult,
    screen::guild_settings,
    style as loqui_style, utils,
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
    last_override_for_guild: AHashMap<u64, SmolStr>,
    last_override_for_channel: AHashMap<(u64, u64), SmolStr>,
    /// guild id -> channel id
    last_channel_id: AHashMap<u64, u64>,
    /// (guild id, channel id, message id)
    /// if exists on this map, then dont show the message with rich text
    dont_highlight_message: AHashSet<(u64, u64, MessageId)>,
    /// file id -> bool
    /// if `true`, then we are (down)loading this attachment
    loading_attachment: AHashMap<String, AtomBool>,
    /// current guild / channel
    current: CurrentIds,
    /// main message composer
    composer: EasyMarkEditor,
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
    main_composer_id: OnceCell<egui::Id>,
}

impl Screen {
    fn get_override(&self, state: &State) -> Option<Overrides> {
        let Some(guild_id) = self.current.guild() else { return None };
        state
            .config
            .default_profiles_for_guilds
            .get(&guild_id)
            .map(String::as_str)
            .or_else(|| {
                state
                    .config
                    .latch_to_channel_guilds
                    .contains(&guild_id)
                    .then(|| {
                        self.current
                            .channel()
                            .and_then(|ids| self.last_override_for_channel.get(&ids))
                    })
                    .unwrap_or_else(|| self.last_override_for_guild.get(&guild_id))
                    .map(SmolStr::as_str)
            })
            .and_then(|name| {
                state
                    .config
                    .overrides
                    .overrides
                    .iter()
                    .find(|p| p.username.as_deref() == Some(name))
            })
            .map(override_from_profile)
    }

    fn main_composer_id(&self) -> egui::Id {
        *self.main_composer_id.get_or_init(|| utils::id("main_composer"))
    }

    fn scroll_to_bottom(&mut self, ui: &mut Ui) {
        self.scroll_to_bottom = true;
        ui.ctx().request_repaint();
    }

    fn is_fetching_attachment(&self, attachment: &Attachment) -> bool {
        self.loading_attachment
            .get(&attachment.id)
            .map_or_else(|| attachment.is_thumbnail(), AtomBool::get)
    }

    fn download_file(&mut self, state: &State, attachment: Attachment) {
        let image_load_bool = AtomBool::new(false);
        self.loading_attachment
            .insert(attachment.id.clone(), image_load_bool.clone());

        spawn_evs!(state, |sender, client| {
            let file = image_load_bool
                .scope(client.fetch_attachment(attachment.id.clone()))
                .await?;
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
                            let mut avatar = Avatar::new(guild.picture.as_deref(), guild.name.as_str(), state);
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
                    self.scroll_to_bottom(ui);
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
            .margin(Margin::same(2.0))
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
                                c.fetch_messages(guild_id, channel_id, None, None, events).await?;
                                let _ = events.send(FetchEvent::FetchedMsgsPins(guild_id, channel_id));
                            });
                        }
                        if channel.fetched_msgs_pins.not() && channel.pinned_messages.is_empty() {
                            spawn_evs!(state, |events, c| {
                                c.fetch_pinned_messages(guild_id, channel_id, events).await?;
                                let _ = events.send(FetchEvent::FetchedMsgsPins(guild_id, channel_id));
                            });
                        }
                        self.scroll_to_bottom(ui);
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
        has_only_image: bool,
        urls: &[(&str, Uri)],
    ) {
        if has_only_image.not() {
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
                    let trimmed_edit_msg = self.edit_message_composer.text().trim().to_string();
                    let edit = self
                        .edit_message_composer
                        .highlight(highlight_message)
                        .editor_ui(ui, utils::id((guild_id, channel_id, id)));
                    let is_pressed = ui.input().key_pressed(egui::Key::Enter) && !ui.input().modifiers.shift;
                    if self.prev_editing_message.is_none() {
                        edit.request_focus();
                    }
                    if trimmed_edit_msg.is_empty().not() && edit.has_focus() && is_pressed {
                        let text = trimmed_edit_msg;
                        let message_id = id.id().unwrap();
                        self.editing_message = None;
                        spawn_client_fut!(state, |client| {
                            client.edit_message(guild_id, channel_id, message_id, text).await
                        });
                    }
                } else if highlight_message {
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
    }

    fn view_message_url_embeds<'a>(&mut self, state: &State, ui: &mut Ui, urls: impl Iterator<Item = (&'a str, Uri)>) {
        let urls = urls.filter_map(|(og, url)| Some((state.cache.get_link_data(&url)?, url, og)));
        for (data, _, raw_url) in urls {
            match data {
                metadata::Data::IsSite(data) => {
                    let id = data
                        .thumbnail
                        .first()
                        .map(|i| i.url.clone())
                        .unwrap_or_else(|| raw_url.to_string());
                    let has_site_title = data.site_title.is_empty().not();
                    let has_page_title = data.page_title.is_empty().not();
                    let has_desc = data.description.is_empty().not();
                    let maybe_thumbnail = state.image_cache.get_image(&id);

                    if has_site_title || has_page_title || has_desc {
                        ui.group(|ui| {
                            let factor = ui
                                .is_mobile()
                                .then(|| 0.95)
                                .unwrap_or_else(|| (500.0 / ui.input().screen_rect.width()));
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
                metadata::Data::IsMedia(data) => {
                    let attachment = Attachment {
                        name: data.name.clone(),
                        mimetype: data.mimetype.clone(),
                        // we dont want the attachment to count as thumbnail
                        size: u32::MAX,
                        id: raw_url.to_string(),
                        ..Default::default()
                    };

                    let mut download = false;
                    let mut open = false;
                    let is_fetching = self.is_fetching_attachment(&attachment);

                    if is_fetching.not() {
                        if attachment.is_raster_image() {
                            if let Some((tex, size)) = state.image_cache.get_image(&attachment.id) {
                                let size = ui.downscale(size);
                                let open_but = ui.frameless_image_button(tex.id(), size);
                                open = open_but.clicked();
                            } else {
                                download = ui.button(format!("download '{}'", data.name)).clicked();
                            }
                        } else {
                            open = ui.button(format!("open '{}'", data.name)).clicked();
                        }
                    } else {
                        ui.add(egui::Spinner::new())
                            .on_hover_text(format!("downloading '{}'", data.name));
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

        if let (Some(Info::Image(info)), true) = (&attachment.info, attachment.is_raster_image()) {
            let mut no_thumbnail = false;

            let (w, h) = (info.width, info.height);
            let maybe_size = (w == 0 || h == 0).not().then(|| ui.downscale([w as f32, h as f32]));

            let is_fetching = self.is_fetching_attachment(attachment);

            if let Some((tex, size)) = state.image_cache.get_image(&attachment.id) {
                let size = maybe_size.unwrap_or_else(|| ui.downscale(size));
                let open_but = ui.frameless_image_button(tex.id(), size);
                open = open_but.clicked();
                handled = true;
            } else if let Some((tex, size)) = state.image_cache.get_thumbnail(&attachment.id) {
                let size = maybe_size.unwrap_or_else(|| ui.downscale(size));
                let button = if is_fetching {
                    ImageBg::new(tex.id(), size)
                        .show(ui, |ui| {
                            ui.add_sized(size, egui::Spinner::new().size(32.0))
                                .on_hover_text_at_pointer("loading image")
                        })
                        .response
                } else {
                    ui.frameless_image_button(tex.id(), size)
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
            let maybe_minithumb = attachment.info.as_ref().and_then(|a| {
                if let Info::Image(info) = a {
                    info.minithumbnail.as_ref()
                } else {
                    None
                }
            });
            if let (true, Some(minithumbnail)) = (load_thumbnail, maybe_minithumb) {
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
        fn filter_empty(val: Option<&String>) -> Option<&str> {
            val.map(|s| s.trim()).filter(|s| s.is_empty().not())
        }

        ui.group(|ui| {
            let do_render_heading = |heading: &&embed::Heading| heading.text.is_empty().not();
            let render_header = |header: &embed::Heading, ui: &mut Ui| {
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
                });
            };

            if let Some(heading) = embed.header.as_ref().filter(do_render_heading) {
                render_header(heading, ui);
                ui.add_space(8.0);
            }

            if embed.title.is_empty().not() {
                ui.label(RichText::new(&embed.title).strong());
            }

            if let Some(body) = filter_empty(embed.body.as_ref().map(|f| &f.text)) {
                ui.label(body);
            }

            for field in &embed.fields {
                ui.group(|ui| {
                    if field.title.is_empty().not() {
                        ui.label(RichText::new(&field.title).strong());
                    }
                    ui.add_space(4.0);
                    if field.body.is_empty().not() {
                        ui.label(&field.body);
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
                    let override_name = overrides.and_then(|ov| ov.username.as_deref());
                    let sender_name = user.map_or_else(|| "unknown", |u| u.username.as_str());
                    let display_name = override_name.unwrap_or(sender_name);

                    let color = guild
                        .highest_role_for_member(message.sender)
                        .map_or(Color32::WHITE, |(_, role)| ui.role_color(role));

                    let user_resp = ui
                        .scope(|ui| {
                            ui.horizontal(|ui| {
                                let extreme_bg_color = ui.style().visuals.extreme_bg_color;
                                self.view_user_avatar(state, ui, user, overrides, extreme_bg_color);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(display_name).color(color).strong());
                                    if override_name.is_some() {
                                        ui.label(RichText::new(format!("({})", sender_name)).small());
                                    }
                                });
                            });
                        })
                        .response;

                    if let Some(user) = user {
                        user_resp.context_menu_styled(|ui| {
                            view_member_context_menu_items(ui, state, guild_id, message.sender, guild, user);
                        });
                    }

                    ui.add_space(ui.style().spacing.item_spacing.y);
                }

                let (urls, has_only_image) = parse_urls(&message.content.text, state);
                self.view_message_text_content(
                    state,
                    ui,
                    id,
                    guild_id,
                    channel_id,
                    message.content.text.trim(),
                    message.failed_to_send,
                    has_only_image,
                    &urls,
                );
                self.view_message_url_embeds(state, ui, urls.into_iter());
                for attachment in &message.content.attachments {
                    self.view_message_attachment(state, ui, attachment);
                }
                for embed in &message.content.embeds {
                    self.view_message_embed(ui, embed);
                }
            })
            .response;

        msg.context_menu_styled(|ui| {
            if let Some(message_id) = id.id() {
                if client::has_perm(guild, channel, all_permissions::MESSAGES_SEND)
                    && message.sender == user_id
                    && ui.button("edit").clicked()
                {
                    self.editing_message = id.id();
                    let edit_text = self.edit_message_composer.text_mut();
                    edit_text.clear();
                    edit_text.push_str(&message.content.text);
                    ui.close_menu();
                }
                if ui.button("quote reply").clicked() {
                    let composer_text = self.composer.text_mut();
                    composer_text.clear();
                    composer_text.push_str("> ");
                    composer_text.push_str(&message.content.text);
                    composer_text.push('\n');
                    ui.memory().request_focus(self.main_composer_id());
                    ui.close_menu();
                }
                if ui.button("copy").clicked() {
                    ui.output().copied_text = message.content.text.clone();
                    ui.close_menu();
                }
                if message.sender == state.client().user_id() && ui.button(dangerous_text("delete")).clicked() {
                    spawn_client_fut!(state, |client| {
                        client.delete_message(guild_id, channel_id, message_id).await
                    });
                    ui.close_menu();
                }
                if channel.pinned_messages.contains(&message_id) {
                    if client::has_perm(guild, channel, all_permissions::MESSAGES_PINS_REMOVE)
                        && ui.button("unpin").clicked()
                    {
                        spawn_client_fut!(state, |client| {
                            client.unpin_message(guild_id, channel_id, message_id).await
                        });
                        ui.close_menu();
                    }
                } else if client::has_perm(guild, channel, all_permissions::MESSAGES_PINS_ADD)
                    && ui.button("pin").clicked()
                {
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
                    egui::Frame::none().margin(Margin::same(5.0)).show(ui, |ui| {
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

        let text_string = self.composer.text().trim().to_string();
        let id = self.main_composer_id();
        let text_edit = self
            .composer
            .desired_rows(desired_lines)
            .desired_width(ui.available_width() - 36.0)
            .hint_text("Enter message...")
            .editor_ui(ui, id);

        let user_inputted_text = {
            let input = ctx.input();
            let has_text = input.events.iter().any(|ev| matches!(ev, Event::Text(_)));
            let any_modifier = input.modifiers.alt || input.modifiers.command;
            has_text && any_modifier.not()
        };

        if text_edit.has_focus().not() {
            let mut focus = false;
            for event in ctx.input().events.iter() {
                if let Event::Paste(text) = event {
                    self.composer.text_mut().push_str(text);
                    focus = true;
                }
            }
            if focus {
                text_edit.request_focus();
            }
        }

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

        let (text_string, (overrides, clear_override)) = text_string.strip_prefix("\\\\").map_or_else(
            || {
                state
                    .override_profile_for_text(&text_string)
                    .map(|(p, text)| (text.to_string(), (Some(override_from_profile(p)), false)))
                    .unwrap_or((text_string.clone(), (self.get_override(state), false)))
            },
            |text| (text.to_string(), (None, true)),
        );
        let is_latching_channel = state.config.latch_to_channel_guilds.contains(&guild_id);
        if clear_override {
            if is_latching_channel {
                self.last_override_for_channel.remove(&(guild_id, channel_id));
            } else {
                self.last_override_for_guild.remove(&guild_id);
            }
        } else if let Some(name) = overrides.as_ref().and_then(|ov| ov.username.as_deref()) {
            let name = SmolStr::new(name);
            if is_latching_channel {
                self.last_override_for_channel.insert((guild_id, channel_id), name);
            } else {
                self.last_override_for_guild.insert(guild_id, name);
            }
        }

        let did_submit_enter = {
            let input = ui.input();
            input.key_pressed(egui::Key::Enter) && !input.modifiers.shift
        };

        if text_string.is_empty().not() && text_edit.has_focus() && did_submit_enter {
            self.composer.text_mut().clear();
            let user_id = state.client().user_id();
            let request = SendMessageRequest {
                guild_id,
                channel_id,
                overrides,
                ..Default::default()
            }
            .with_text(text_string);
            let echo_id = state.cache.prepare_send_message(user_id, request.clone());
            spawn_evs!(state, |evs, client| {
                client.send_message(request.with_echo_id(echo_id), evs).await?;
            });
            self.scroll_to_bottom(ui);
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
    }

    fn view_uploading_attachments(&mut self, state: &State, ui: &mut Ui) {
        ui.label(RichText::new("Uploading:").strong());
        for name in state.uploading_files.read().expect("poisoned").iter() {
            egui::Frame::group(ui.style()).margin(Margin::same(0.0)).show(ui, |ui| {
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
                        // TODO: return errors for files that failed to upload
                        let id = client.upload_file(file.file_name(), mimetype.clone(), data).await?;
                        attachments.push(send_message_request::Attachment {
                            id,
                            name: file.file_name(),
                            ..Default::default()
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
        overrides: Option<&Overrides>,
        fill_bg: Color32,
    ) {
        const SIZE: Vec2 = egui::vec2(28.0, 28.0);

        let maybe_tex = overrides
            .and_then(|ov| ov.avatar.as_ref())
            .or_else(|| user.and_then(|u| u.avatar_url.as_ref()))
            .as_ref()
            .and_then(|id| state.image_cache.get_avatar(id));

        let status = user.map_or(user_status::Kind::OfflineUnspecified, |m| m.status.kind());
        let status_color = match status {
            user_status::Kind::OfflineUnspecified => Color32::GRAY,
            user_status::Kind::Online => Color32::GREEN,
            user_status::Kind::DoNotDisturb => Color32::RED,
            user_status::Kind::Idle => Color32::GOLD,
        };

        egui::Frame::group(ui.style())
            .margin(Vec2::ZERO)
            .rounding(0.0)
            .stroke(egui::Stroke::new(4.0, status_color))
            .show(ui, |ui| {
                if let Some((texid, _)) = maybe_tex {
                    ui.image(texid.id(), SIZE);
                } else {
                    ui.add_enabled_ui(false, |ui| {
                        let username = overrides
                            .and_then(|ov| ov.username.as_deref())
                            .or_else(|| user.map(|u| u.username.as_str()))
                            .unwrap_or("");
                        let letter = username.get(0..1).unwrap_or("u").to_ascii_uppercase();

                        ui.add_sized(SIZE, egui::Button::new(letter).fill(fill_bg));
                    });
                }
            });
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
                                        .map_or(Color32::WHITE, |(_, role)| ui.role_color(role));
                                    self.view_user_avatar(state, ui, Some(user), None, loqui_style::BG_LIGHT);
                                    ui.vertical(|ui| {
                                        ui.colored_label(role_color, user.username.as_str());
                                        if guild.owners.contains(id) {
                                            ui.label(RichText::new("(owner)").small());
                                        }
                                    });
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
                    .map(|(id, msg)| (id, &msg.content.text, msg.sender))
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
            margin: Margin::symmetric(8.0, 5.0),
            fill: ui.style().visuals.extreme_bg_color,
            stroke: ui.style().visuals.window_stroke(),
            rounding: Rounding::same(4.0),
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
            margin: Margin::symmetric(8.0, 5.0),
            fill: ui.style().visuals.window_fill(),
            stroke: ui.style().visuals.window_stroke(),
            rounding: Rounding::same(4.0),
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
            margin: Margin::symmetric(8.0, 5.0),
            fill: ui.style().visuals.extreme_bg_color,
            stroke: ui.style().visuals.window_stroke(),
            rounding: Rounding::same(4.0),
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
                margin: Margin::symmetric(4.0, 2.0),
                fill: ui.style().visuals.window_fill(),
                stroke: ui.style().visuals.window_stroke(),
                rounding: Rounding::same(2.0),
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
        let Some((guild_id, channel_id)) = self.current.channel() else { return };

        let (typing_members, can_send_message) = {
            let Some(guild) = state.cache.get_guild(guild_id) else { return };
            let Some(channel) = state.cache.get_channel(guild_id, channel_id) else { return };

            let current_user_id = state.client().user_id();
            let typing_members = self
                .get_typing_members(state, guild)
                .any(|(id, _)| current_user_id.ne(&id));

            let can_send_message = client::has_perm(guild, channel, all_permissions::MESSAGES_SEND);

            (typing_members, can_send_message)
        };

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

                    let (desired_lines, extra_offset) = ui
                        .memory()
                        .has_focus(self.main_composer_id())
                        .then(|| (4, 0.0))
                        .unwrap_or((1, 14.0));
                    let extra_offset = typing_members.then(|| extra_offset + 20.0).unwrap_or(extra_offset);
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
                            if can_send_message {
                                self.view_composer(state, ui, ctx, desired_lines);
                                self.view_upload_button(state, ui);
                            } else {
                                ui.label(RichText::new("can't send messages").underline());
                                ui.add_space(ui.available_width() - 8.0);
                            }
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
                        attachments: vec![send_message_request::Attachment {
                            id,
                            name,
                            ..Default::default()
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
            margin: Margin::same(8.0),
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
                central_panel.show(ctx, |ui| match state.local_config.bg_image {
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
                    _ => {
                        if let Some((tex, _)) = state.image_cache.get_bg_image() {
                            let size = ctx.available_rect().size() + egui::vec2(8.0, 8.0);
                            ImageBg::new(tex.id(), size)
                                .offset(egui::vec2(-8.0, -8.0))
                                .show(ui, |ui| show_main(state, ui));
                        } else {
                            show_main(state, ui);
                        }
                    }
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
