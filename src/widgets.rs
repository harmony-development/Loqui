use eframe::egui::{self, Ui, WidgetText};

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
