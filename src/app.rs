use std::{cell::RefCell, ops::Not, sync::mpsc};

use client::{
    harmony_rust_sdk::{
        api::{
            chat::{Event, EventSource},
            rest::{About, FileId},
        },
        client::{EventsReadSocket, EventsSocket, EventsWriteSocket},
    },
    Cache, Client, FetchEvent,
};
use eframe::{
    egui::{self, FontData, FontDefinitions, TextureId, Ui, Vec2},
    epi,
};
use tokio::sync::mpsc as tokio_mpsc;

use super::utils::*;

use crate::{
    futures::Futures,
    image_cache::{ImageCache, LoadedImage},
    screen::{auth, BoxedScreen, Screen, ScreenStack},
    style as loqui_style,
    widgets::{menu_text_button, view_about, view_egui_settings},
};

pub struct State {
    pub socket_tx_tx: tokio_mpsc::Sender<EventsWriteSocket>,
    pub socket_rx_tx: tokio_mpsc::Sender<EventsReadSocket>,
    pub socket_event_rx: mpsc::Receiver<Event>,
    pub client: Option<Client>,
    pub cache: Cache,
    pub image_cache: ImageCache,
    pub loading_images: RefCell<Vec<FileId>>,
    pub futures: Futures,
    pub latest_errors: Vec<String>,
    pub about: Option<About>,
    pub harmony_lotus: (TextureId, Vec2),
    next_screen: Option<BoxedScreen>,
    prev_screen: bool,
}

impl State {
    pub fn client(&self) -> &Client {
        self.client.as_ref().expect("client not initialized yet")
    }

    pub fn push_screen<S: Screen>(&mut self, screen: S) {
        self.next_screen = Some(Box::new(screen));
    }

    pub fn pop_screen(&mut self) {
        self.prev_screen = true;
    }

    pub fn run<F, E, O>(&mut self, res: Result<O, E>, f: F) -> bool
    where
        F: FnOnce(&mut Self, O),
        E: std::error::Error + Send + Sync + 'static,
    {
        match res {
            Ok(val) => {
                f(self, val);
                false
            }
            Err(err) => {
                let msg = err.to_string();
                let exit = msg.contains("h.bad-session");
                self.latest_errors.push(msg);
                exit
            }
        }
    }

    #[inline(always)]
    fn handle_sockets(&mut self) {
        handle_future!(self, |res: ClientResult<EventsSocket>| {
            self.run(res, |state, sock| {
                let (tx, rx) = sock.split();
                let _ = state.socket_tx_tx.try_send(tx);
                let _ = state.socket_rx_tx.try_send(rx);
            });
        });
    }

    #[inline(always)]
    fn handle_errors(&mut self) {
        handle_future!(self, |res: ClientResult<()>| {
            self.run(res, |_, _| {});
        });
    }

    #[inline(always)]
    fn handle_events(&mut self, frame: &epi::Frame) {
        handle_future!(self, |res: ClientResult<Vec<FetchEvent>>| {
            self.run(res, |state, events| {
                let mut posts = Vec::new();
                for event in events {
                    match event {
                        FetchEvent::Attachment { attachment, file } => {
                            if attachment.kind.starts_with("image") && attachment.kind.ends_with("svg+xml").not() {
                                spawn_future!(
                                    state,
                                    LoadedImage::load(
                                        frame.clone(),
                                        file.data().clone(),
                                        attachment.id,
                                        attachment.name.into()
                                    )
                                );
                            }
                        }
                        event => state.cache.process_event(&mut posts, event),
                    }
                }
                if let Some(client) = state.client.as_ref().cloned() {
                    spawn_future!(state, {
                        async move {
                            let mut events = Vec::with_capacity(posts.len());
                            for post in posts {
                                client.process_post(&mut events, post).await?;
                            }
                            ClientResult::Ok(events)
                        }
                    });
                }
            });
        });
    }

    #[inline(always)]
    fn handle_images(&mut self, frame: &epi::Frame) {
        handle_future!(self, |image: LoadedImage| {
            let maybe_pos = self.loading_images.borrow().iter().position(|id| image.id() == id);
            if let Some(pos) = maybe_pos {
                self.loading_images.borrow_mut().remove(pos);
            }
            self.image_cache.add(frame, image);
        });
    }

    #[inline(always)]
    fn handle_socket_events(&mut self) {
        let mut evs = Vec::new();
        while let Ok(ev) = self.socket_event_rx.try_recv() {
            evs.push(FetchEvent::Harmony(ev));
        }
        if !evs.is_empty() {
            spawn_future!(self, std::future::ready(ClientResult::Ok(evs)));
        }
    }

    #[inline(always)]
    fn handle_about(&mut self) {
        handle_future!(self, |res: ClientResult<About>| {
            self.run(res, |state, about| {
                state.about = Some(about);
            });
        });
    }
}

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
        let mut cache = Cache::default();
        let futures = Futures::new();
        let (socket_sub_tx, mut socket_sub_rx) = tokio_mpsc::unbounded_channel::<EventSource>();
        cache.set_sub_tx(socket_sub_tx);
        let (socket_tx_tx, mut socket_tx_rx) = tokio_mpsc::channel::<EventsWriteSocket>(2);
        futures.spawn(async move {
            let mut tx = socket_tx_rx.recv().await.expect("closed");

            loop {
                tokio::select! {
                    Some(sock) = socket_tx_rx.recv() => {
                        tx = sock;
                    }
                    Some(sub) = socket_sub_rx.recv() => {
                        if tx.add_source(sub).await.is_err() {
                            // reset socket
                        }
                    }
                    else => std::hint::spin_loop(),
                }
            }
        });

        let (socket_rx_tx, mut socket_rx_rx) = tokio_mpsc::channel::<EventsReadSocket>(2);
        let (socket_event_tx, socket_event_rx) = mpsc::channel::<Event>();
        futures.spawn(async move {
            let mut rx = socket_rx_rx.recv().await.expect("closed");

            loop {
                tokio::select! {
                    Some(sock) = socket_rx_rx.recv() => {
                        rx = sock;
                    }
                    Ok(Some(ev)) = rx.get_event() => {
                        if socket_event_tx.send(ev).is_err() {
                            // reset socket
                        }
                    }
                    else => std::hint::spin_loop(),
                }
            }
        });

        Self {
            state: State {
                socket_rx_tx,
                socket_tx_tx,
                socket_event_rx,
                client: None,
                cache,
                image_cache: Default::default(),
                loading_images: RefCell::new(Vec::new()),
                futures,
                latest_errors: Vec::new(),
                about: None,
                harmony_lotus: (TextureId::Egui, Vec2::ZERO),
                next_screen: None,
                prev_screen: false,
            },
            screens: ScreenStack::new(auth::Screen::new()),
            show_errors_window: false,
            show_about_window: false,
            show_egui_debug: false,
        }
    }

    #[inline(always)]
    fn view_bottom_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            egui::warn_if_debug_build(ui);

            let is_main_or_auth = matches!(self.screens.current().id(), "main" | "auth");
            if is_main_or_auth.not() && ui.button("<- back").on_hover_text("go back").clicked() {
                self.state.pop_screen();
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

            ui.add_space(ui.available_width() - 100.0);

            egui::Frame::group(ui.style()).margin([0.0, 0.0]).show(ui, |ui| {
                menu_text_button("top_panel_menu", "menu", ui, |ui| {
                    if ui.text_button("about server").clicked() {
                        self.show_about_window = true;
                    }

                    if ui.text_button("settings").clicked() {
                        self.state.push_screen(super::screen::settings::Screen::default());
                    }

                    if ui.text_button("logout").clicked() {
                        self.screens.clear(super::screen::auth::Screen::new());
                        let client = self.state.client().clone();
                        self.state.client = None;
                        let state = &self.state;
                        spawn_future!(state, async move { client.logout().await });
                    }

                    if ui.text_button("exit loqui").clicked() {
                        std::process::exit(0);
                    }

                    if ui.text_button("egui debug").clicked() {
                        self.show_egui_debug = true;
                    }
                });
            });
        });
    }

    #[inline(always)]
    fn view_errors_window(&mut self, ctx: &egui::CtxRef) {
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
    fn view_about_window(&mut self, ctx: &egui::CtxRef) {
        guard!(let Some(about) = self.state.about.as_ref() else { return });

        egui::Window::new("about server")
            .open(&mut self.show_about_window)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    view_about(ui, about);
                });
            });
    }

    #[inline(always)]
    fn view_egui_debug_window(&mut self, ctx: &egui::CtxRef) {
        egui::Window::new("egui debug")
            .open(&mut self.show_egui_debug)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    view_egui_settings(ctx, ui);
                });
            });
    }

    fn load_harmony_lotus(&self, frame: &epi::Frame) -> (TextureId, Vec2) {
        const HARMONY_LOTUS: &[u8] = include_bytes!("../resources/lotus.png");
        let image = image::load_from_memory(HARMONY_LOTUS).expect("harmony lotus must be fine");
        let image = image.into_rgba8();
        let (w, h) = image.dimensions();
        let size = [w as usize, h as usize];
        let rgba = image.into_raw();
        let texid = frame.alloc_texture(epi::Image::from_rgba_unmultiplied(size, &rgba));
        (texid, [w as f32, h as f32].into())
    }
}

impl epi::App for App {
    fn name(&self) -> &str {
        "loqui"
    }

    fn setup(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame, _storage: Option<&dyn epi::Storage>) {
        self.state.futures.init(frame);
        self.state.futures.spawn(async move {
            guard!(let Some(session) = Client::read_latest_session().await else { return Ok(None); });

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
            "iosevka".to_string(),
            FontData::from_static(include_bytes!("fonts/Iosevka.ttf")),
        );
        font_defs
            .fonts_for_family
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "inter".to_string());
        font_defs
            .fonts_for_family
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "iosevka".to_string());

        ctx.set_fonts(font_defs);

        // load harmony lotus
        self.state.harmony_lotus = self.load_harmony_lotus(frame);

        let mut style = ctx.style().as_ref().clone();
        style.visuals.widgets.hovered.bg_stroke.color = loqui_style::HARMONY_LOTUS_ORANGE;
        style.visuals.widgets.hovered.bg_fill = loqui_style::HARMONY_LOTUS_ORANGE;
        style.visuals.selection.bg_fill = loqui_style::HARMONY_LOTUS_GREEN;
        style.visuals.widgets.noninteractive.bg_fill = loqui_style::BG_NORMAL;
        style.visuals.extreme_bg_color = loqui_style::BG_EXTREME;
        ctx.set_style(style);
    }

    fn max_size_points(&self) -> egui::Vec2 {
        [f32::INFINITY, f32::INFINITY].into()
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame) {
        let state = &mut self.state;

        state.futures.run();
        state.cache.maintain();

        state.handle_about();
        state.handle_errors();
        state.handle_events(frame);
        state.handle_sockets();
        state.handle_images(frame);

        state.handle_socket_events();

        // ui drawing starts here

        ctx.set_pixels_per_point(1.45);

        egui::TopBottomPanel::top("top_status_panel")
            .frame(egui::Frame {
                margin: [4.0, 2.0].into(),
                fill: ctx.style().visuals.extreme_bg_color,
                stroke: ctx.style().visuals.window_stroke(),
                ..Default::default()
            })
            .max_height(12.0)
            .min_height(12.0)
            .show(ctx, |ui| {
                self.view_bottom_panel(ui);
            });

        if self.state.latest_errors.is_empty().not() {
            self.view_errors_window(ctx);
        }
        self.view_about_window(ctx);
        self.view_egui_debug_window(ctx);

        self.screens.current_mut().update(ctx, frame, &mut self.state);

        // post ui update handling

        if let Some(screen) = self.state.next_screen.take() {
            self.screens.push_boxed(screen);
        } else if self.state.prev_screen {
            self.screens.pop();
            self.state.prev_screen = false;
        }
    }
}
