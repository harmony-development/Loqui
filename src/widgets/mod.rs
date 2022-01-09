use std::ops::Not;

use client::harmony_rust_sdk::api::rest::{About, FileId};
use eframe::egui::{self, CollapsingHeader, CollapsingResponse, Color32, Response, RichText, Ui, Widget, WidgetText};

use crate::{app::State, utils::UiExt};

pub mod bg_image;
pub mod easy_mark;

pub fn menu_text_button<R>(
    id: impl AsRef<str>,
    title: impl Into<WidgetText>,
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let response = ui.add_sized(
        [ui.available_width(), 12.0],
        egui::Button::new(title).small().frame(false),
    );
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

pub fn view_panel_chooser<P>(ui: &mut Ui, panels: &[P], chosen_panel: &mut P)
where
    P: AsRef<str> + Clone,
{
    for panel in panels {
        let panel_name = panel.as_ref();

        let enabled = panel_name != chosen_panel.as_ref();
        let but = ui.add_enabled_ui(enabled, |ui| ui.text_button(panel_name)).inner;
        if but.clicked() {
            *chosen_panel = panel.clone();
        }

        ui.add(egui::Separator::default().spacing(0.0));
    }
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
