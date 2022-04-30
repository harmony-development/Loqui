use std::ops::Not;

use client::{content, Client};
use eframe::egui::{self, vec2, Color32, FontData, FontDefinitions, Style, Ui};

use super::utils::*;

use crate::{
    screen::{auth, ScreenStack},
    state::State,
    style as loqui_style,
    widgets::{view_egui_settings, About},
};

pub struct App {
    state: State,
    screens: ScreenStack,
    show_errors_window: bool,
    show_about_window: bool,
    show_egui_debug: bool,
}

impl App {
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            state: State::new(),
            screens: ScreenStack::new(auth::Screen::new()),
            show_errors_window: false,
            show_about_window: false,
            show_egui_debug: false,
        }
    }

    fn view_connection_status(&mut self, ui: &mut Ui) {
        let is_connected = self.state.is_connected;
        let is_reconnecting = self.state.connecting_socket;

        let (connection_status_color, text_color) = if is_connected {
            (Color32::GREEN, Color32::BLACK)
        } else if is_reconnecting {
            (Color32::YELLOW, Color32::BLACK)
        } else {
            (Color32::RED, Color32::WHITE)
        };

        egui::Frame::none().fill(connection_status_color).show(ui, |ui| {
            ui.style_mut().visuals.override_text_color = Some(text_color);
            ui.style_mut().visuals.widgets.active.fg_stroke.color = text_color;

            if is_connected {
                ui.label("✓ connected");
            } else if is_reconnecting {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label("reconnecting");
            } else {
                let resp = ui.label("X disconnected");
                let last_retry_passed = self
                    .state
                    .last_socket_retry
                    .map(|ins| format!("retrying in {}", ins.elapsed().as_secs()));
                if let Some(text) = last_retry_passed {
                    resp.on_hover_text(text);
                }
            }
        });
    }

    #[inline(always)]
    fn view_bottom_panel(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        ui.horizontal_top(|ui| {
            ui.style_mut().spacing.item_spacing = vec2(2.0, 0.0);

            self.view_connection_status(ui);

            let is_mobile = ui.ctx().is_mobile();

            if is_mobile.not() {
                if cfg!(debug_assertions) {
                    egui::Frame::none().fill(Color32::RED).show(ui, |ui| {
                        ui.colored_label(Color32::BLACK, "⚠ Debug build ⚠")
                            .on_hover_text("egui was compiled with debug assertions enabled.");
                    });
                }

                if self.state.latest_errors.is_empty().not() {
                    let new_errors_but = ui
                        .add(egui::Button::new(dangerous_text("new errors")).small())
                        .on_hover_text("show errors");
                    if new_errors_but.clicked() {
                        self.show_errors_window = true;
                    }
                } else {
                    ui.label("no errors");
                }
            }

            let show_back_button = matches!(self.screens.current().id(), "main" | "auth").not();
            if show_back_button {
                ui.offsetw(140.0);
                if ui.button("<- back").on_hover_text("go back").clicked() {
                    self.state.pop_screen();
                }
            }

            if show_back_button.not() {
                ui.offsetw(80.0);
            }

            ui.vertical_centered_justified(|ui| {
                ui.menu_button("▼ menu", |ui| {
                    if ui.button("about server").clicked() {
                        self.show_about_window = true;
                        ui.close_menu();
                    }

                    if ui.ctx().is_mobile().not() && ui.button("settings").clicked() {
                        self.state
                            .push_screen(super::screen::settings::Screen::new(ui.ctx(), &self.state));
                        ui.close_menu();
                    }

                    if ui.button("logout").clicked() {
                        self.screens.clear(super::screen::auth::Screen::new());
                        let client = self.state.client.take().expect("no logout");
                        self.state.reset_socket_state();
                        self.state.futures.spawn(async move { client.logout().await });
                        ui.close_menu();
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("exit loqui").clicked() {
                        _frame.quit();
                        ui.close_menu();
                    }

                    if ui.button("egui debug").clicked() {
                        self.show_egui_debug = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    #[inline(always)]
    fn view_errors_window(&mut self, ctx: &egui::Context) {
        let latest_errors = &mut self.state.latest_errors;
        egui::Window::new("last error")
            .open(&mut self.show_errors_window)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("clear").clicked() {
                        latest_errors.clear();
                    }
                    if ui.button("copy all").clicked() {
                        let errors_concatted = latest_errors.iter().fold(String::new(), |mut all, error| {
                            all.push('\n');
                            all.push_str(error);
                            all
                        });
                        ui.output().copied_text = errors_concatted;
                    }
                });
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let errors_len = latest_errors.len();
                    for (index, error) in latest_errors.iter().enumerate() {
                        ui.label(error);
                        if index != errors_len - 1 {
                            ui.separator();
                        }
                    }
                });
            });
    }

    #[inline(always)]
    fn view_about_window(&mut self, ctx: &egui::Context) {
        let Some(about) = self.state.about.as_ref() else { return };

        egui::Window::new("about server")
            .open(&mut self.show_about_window)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add(About::new(about.clone()));
                });
            });
    }

    #[inline(always)]
    fn view_egui_debug_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("egui debug")
            .open(&mut self.show_egui_debug)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    view_egui_settings(ctx, ui);
                });
            });
    }

    pub fn setup(&mut self, cc: &eframe::CreationContext) {
        let ctx = &cc.egui_ctx;

        self.state.init(ctx, cc.integration_info.clone());
        if self.state.local_config.scale_factor != 0.0 {
            ctx.set_pixels_per_point(self.state.local_config.scale_factor);
        }

        self.state.futures.spawn(async move {
            let Some(session) = Client::read_latest_session().await else { return Ok(None) };

            Client::new(session.homeserver.parse().unwrap(), Some(session.into()))
                .await
                .map(Some)
        });

        let mut font_defs = FontDefinitions::default();
        font_defs.font_data.insert(
            "inter".to_string(),
            FontData::from_static(include_bytes!("fonts/Inter.otf")),
        );
        font_defs.font_data.insert(
            "hack".to_string(),
            FontData::from_static(include_bytes!("fonts/Hack-Regular.ttf")),
        );
        font_defs.font_data.insert(
            "emoji-icon-font".to_string(),
            FontData::from_static(include_bytes!("fonts/emoji-icon-font.ttf")),
        );

        font_defs.families.insert(
            egui::FontFamily::Proportional,
            vec!["inter".to_string(), "emoji-icon-font".to_string()],
        );
        font_defs
            .families
            .insert(egui::FontFamily::Monospace, vec!["hack".to_string()]);

        ctx.set_fonts(font_defs);

        if let Some(style) = content::get_local_config::<Style>("style") {
            ctx.set_style(style);
        } else {
            let mut style = Style {
                visuals: egui::Visuals::dark(),
                ..Style::default()
            };
            style.visuals.widgets.hovered.bg_stroke.color = loqui_style::HARMONY_LOTUS_ORANGE;
            style.visuals.widgets.hovered.bg_fill = loqui_style::HARMONY_LOTUS_ORANGE;
            style.visuals.selection.bg_fill = loqui_style::HARMONY_LOTUS_GREEN;
            style.visuals.widgets.noninteractive.bg_fill = loqui_style::BG_NORMAL;
            style.visuals.extreme_bg_color = loqui_style::BG_EXTREME;
            content::set_local_config("style", &style);
            ctx.set_style(style);
        }
    }
}

impl eframe::App for App {
    fn max_size_points(&self) -> egui::Vec2 {
        [f32::INFINITY, f32::INFINITY].into()
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.state.maintain(ctx);

        // ui drawing starts here

        let is_main_screen = self.screens.current().id() == "main";
        if self.state.is_connected.not() || is_main_screen.not() || ctx.is_mobile().not() {
            let style = ctx.style();
            let frame_panel = egui::Frame {
                fill: style.visuals.extreme_bg_color,
                stroke: style.visuals.window_stroke(),
                ..Default::default()
            };
            egui::TopBottomPanel::top("top_status_panel")
                .frame(frame_panel)
                .max_height(style.spacing.interact_size.y)
                .min_height(style.spacing.interact_size.y)
                .show(ctx, |ui| {
                    self.view_bottom_panel(ui, frame);
                });
        }

        if self.state.latest_errors.is_empty().not() {
            self.view_errors_window(ctx);
        }
        self.view_about_window(ctx);
        self.view_egui_debug_window(ctx);

        self.screens.current_mut().update(ctx, frame, &mut self.state);

        // post ui update handling

        if let Some(screen) = self.state.next_screen.take() {
            self.screens.current_mut().on_push(ctx, frame, &mut self.state);
            self.screens.push_boxed(screen);
        } else if self.state.prev_screen {
            self.screens.current_mut().on_pop(ctx, frame, &mut self.state);
            self.screens.pop();
            self.state.prev_screen = false;
        }
    }

    fn on_exit(&mut self, _glow: &eframe::glow::Context) {
        self.state.save_config();
    }
}
