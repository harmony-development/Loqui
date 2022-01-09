use std::ops::Not;

use client::{
    channel::Channel,
    guild::Guild,
    harmony_rust_sdk::api::{
        chat::all_permissions,
        rest::{About, FileId},
    },
    member::Member,
};
use eframe::egui::{
    self, Button, CollapsingHeader, CollapsingResponse, Color32, Response, RichText, Ui, Widget, WidgetText,
};

use crate::{
    app::State,
    utils::{dangerous_text, spawn_client_fut},
};

pub mod bg_image;
pub mod easy_mark;

pub struct TextButton {
    inner: Button,
}

impl TextButton {
    #[inline(always)]
    pub fn text(text: impl Into<WidgetText>) -> Self {
        Self::new(Button::new(text))
    }

    #[inline(always)]
    pub fn new(button: Button) -> Self {
        Self {
            inner: button.frame(false),
        }
    }

    #[inline(always)]
    pub fn small(mut self) -> Self {
        self.inner = self.inner.small();
        self
    }
}

impl Widget for TextButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let text_color = ui.style().visuals.widgets.hovered.bg_fill;
        ui.style_mut().visuals.widgets.hovered.fg_stroke.color = text_color;

        ui.add(self.inner)
    }
}

pub fn menu_text_button<R>(
    id: impl AsRef<str>,
    title: impl Into<WidgetText>,
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let response = ui.add_sized([ui.available_width(), 12.0], TextButton::text(title).small());
    let popup_id = ui.make_persistent_id(id.as_ref());
    if response.clicked() {
        ui.memory().toggle_popup(popup_id);
    }
    egui::popup_below_widget(ui, popup_id, &response, add_contents)
}

pub fn view_about(ui: &mut Ui, about: &About) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&about.server_name).heading().strong());
            ui.label(&about.version);
        });
        ui.separator();
        if about.about_server.is_empty().not() {
            ui.label(&about.about_server);
        } else {
            ui.label("no about");
        }
        if about.message_of_the_day.is_empty().not() {
            ui.label(format!("motd: {}", about.message_of_the_day));
        } else {
            ui.label("no motd");
        }
    });
}

pub struct Avatar<'a> {
    state: &'a State,
    maybe_id: Option<&'a FileId>,
    text: &'a str,
    size: f32,
    fill_bg: Option<Color32>,
}

impl<'a> Avatar<'a> {
    pub fn new(maybe_id: Option<&'a FileId>, text: &'a str, state: &'a State) -> Self {
        Self {
            state,
            maybe_id,
            text,
            size: 32.0,
            fill_bg: None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn fill_bg(mut self, color: Color32) -> Self {
        self.fill_bg = Some(color);
        self
    }
}

impl<'a> Widget for Avatar<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        if let Some((texid, _)) = self.maybe_id.and_then(|id| self.state.image_cache.get_avatar(id)) {
            ui.add(egui::ImageButton::new(texid, [self.size; 2]).frame(false))
        } else {
            let icon = RichText::new(self.text.get(0..1).unwrap_or("u").to_ascii_uppercase()).strong();
            let mut button = egui::Button::new(icon);
            if let Some(color) = self.fill_bg {
                button = button.fill(color);
            }

            ui.add_sized([self.size; 2], button)
        }
    }
}

pub fn seperated_collapsing<R>(
    ui: &mut Ui,
    title: &str,
    show_default: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> CollapsingResponse<R> {
    ui.horizontal_top(|ui| {
        let resp = egui::CollapsingHeader::new(title)
            .default_open(show_default)
            .show(ui, add_contents);
        ui.add(egui::Separator::default().horizontal());
        resp
    })
    .inner
}

pub fn view_egui_settings(ctx: &egui::CtxRef, ui: &mut Ui) {
    ctx.settings_ui(ui);
    CollapsingHeader::new("Inspection")
        .default_open(false)
        .show(ui, |ui| ctx.inspection_ui(ui));
    CollapsingHeader::new("Memory")
        .default_open(false)
        .show(ui, |ui| ctx.memory_ui(ui));
}

pub fn view_member_context_menu_items(
    ui: &mut Ui,
    state: &State,
    guild_id: u64,
    member_id: u64,
    guild: &Guild,
    member: &Member,
) {
    if ui.button("copy id").clicked() {
        ui.output().copied_text = member_id.to_string();
        ui.close_menu();
    }
    if ui.button("copy username").clicked() {
        ui.output().copied_text = member.username.to_string();
        ui.close_menu();
    }
    if guild.has_perm(all_permissions::USER_MANAGE_BAN) && ui.button(dangerous_text("ban")).clicked() {
        spawn_client_fut!(state, |client| {
            client.ban_member(guild_id, member_id).await?;
        });
        ui.close_menu();
    }
    if guild.has_perm(all_permissions::USER_MANAGE_KICK) && ui.button(dangerous_text("kick")).clicked() {
        spawn_client_fut!(state, |client| {
            client.kick_member(guild_id, member_id).await?;
        });
        ui.close_menu();
    }
}

pub fn view_channel_context_menu_items(
    ui: &mut Ui,
    state: &State,
    guild_id: u64,
    channel_id: u64,
    guild: &Guild,
    channel: &Channel,
) {
    if ui.button("copy id").clicked() {
        ui.output().copied_text = channel_id.to_string();
        ui.close_menu();
    }
    if ui.button("copy name").clicked() {
        ui.output().copied_text = channel.name.to_string();
        ui.close_menu();
    }
    if guild.has_perm(all_permissions::CHANNELS_MANAGE_DELETE) && ui.button(dangerous_text("delete")).clicked() {
        spawn_client_fut!(state, |client| {
            client.delete_channel(guild_id, channel_id).await?;
        });
        ui.close_menu();
    }
}
