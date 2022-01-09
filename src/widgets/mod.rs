use std::ops::Not;

use client::harmony_rust_sdk::api::rest::{About, FileId};
use eframe::egui::{self, CollapsingHeader, CollapsingResponse, Response, RichText, Ui, WidgetText};

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

pub fn view_avatar(ui: &mut Ui, state: &State, maybe_id: Option<&FileId>, text: &str, size: f32) -> Response {
    if let Some((texid, _)) = maybe_id.and_then(|id| state.image_cache.get_avatar(id)) {
        ui.add(egui::ImageButton::new(texid, [size, size]).frame(false))
    } else {
        let icon = RichText::new(text.get(0..1).unwrap_or("u").to_ascii_uppercase()).strong();
        ui.add_sized([size, size], egui::Button::new(icon))
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
