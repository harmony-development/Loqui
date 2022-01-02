use std::ops::Not;

use client::harmony_rust_sdk::api::rest::About;
use eframe::egui::{self, RichText, Ui, WidgetText};

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
