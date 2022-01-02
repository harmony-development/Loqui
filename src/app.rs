use std::{cell::RefCell, ops::Not, sync::mpsc};

use client::{
    harmony_rust_sdk::{
        api::{
            chat::{Event, EventSource},
            rest::FileId,
        },
        client::{EventsReadSocket, EventsSocket, EventsWriteSocket},
    },
    Cache, Client, FetchEvent,
};
use eframe::{
    egui::{self, FontData, FontDefinitions, RichText},
    epi,
};
use tokio::sync::mpsc as tokio_mpsc;

use super::utils::*;

use crate::{
    futures::Futures,
    image_cache::{ImageCache, LoadedImage},
    screen::{auth, BoxedScreen, Screen, ScreenStack},
    widgets::menu_text_button,
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
}

pub struct App {
    state: State,
    screens: ScreenStack,
    last_error: bool,
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
                next_screen: None,
                prev_screen: false,
            },
            screens: ScreenStack::new(auth::Screen::new()),
            last_error: false,
        }
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
    }

    fn max_size_points(&self) -> egui::Vec2 {
        [f32::INFINITY, f32::INFINITY].into()
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame) {
        self.state.futures.run();

        let state = &mut self.state;
        handle_future!(state, |res: ClientResult<()>| {
            state.run(res, |_, _| {});
        });

        handle_future!(state, |res: ClientResult<Vec<FetchEvent>>| {
            state.run(res, |state, events| {
                let mut posts = Vec::new();
                for event in events {
                    match event {
                        FetchEvent::Attachment { attachment, file } => {
                            if attachment.kind.starts_with("image") && attachment.kind.ends_with("svg+xml").not() {
                                spawn_future!(
                                    state,
                                    LoadedImage::load(file.data().clone(), attachment.id, attachment.name.into())
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

        handle_future!(state, |res: ClientResult<EventsSocket>| {
            state.run(res, |state, sock| {
                let (tx, rx) = sock.split();
                let _ = state.socket_tx_tx.try_send(tx);
                let _ = state.socket_rx_tx.try_send(rx);
            });
        });

        handle_future!(state, |image: LoadedImage| {
            let maybe_pos = state.loading_images.borrow().iter().position(|id| image.id() == id);
            if let Some(pos) = maybe_pos {
                state.loading_images.borrow_mut().remove(pos);
            }
            state.image_cache.add(frame, image);
        });

        // handle socket events
        let mut evs = Vec::new();
        while let Ok(ev) = state.socket_event_rx.try_recv() {
            evs.push(FetchEvent::Harmony(ev));
        }
        if !evs.is_empty() {
            spawn_future!(state, std::future::ready(ClientResult::Ok(evs)));
        }

        ctx.set_pixels_per_point(1.45);
        {
            let last_error = &mut self.last_error;
            let screens = &mut self.screens;
            egui::TopBottomPanel::top("bottom_panel")
                .max_height(12.0)
                .min_height(12.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if state.latest_errors.is_empty().not() {
                            if ui.button("clear").clicked() {
                                state.latest_errors.clear();
                            }
                            if ui
                                .button(RichText::new("new errors").color(egui::Color32::RED))
                                .clicked()
                            {
                                *last_error = true;
                            }
                        } else {
                            ui.label("no errors");
                        }

                        ui.add_space(ui.available_width() - 100.0);

                        egui::Frame::group(ui.style()).margin([0.0, 0.0]).show(ui, |ui| {
                            menu_text_button("top_panel_menu", "menu", ui, |ui| {
                                if ui.text_button("settings").clicked() {
                                    state.push_screen(super::screen::settings::Screen::default());
                                }

                                ui.add(egui::Separator::default().spacing(0.0));

                                if ui.text_button("logout").clicked() {
                                    screens.clear(super::screen::auth::Screen::new());
                                    let client = state.client().clone();
                                    state.client = None;
                                    spawn_future!(state, async move { client.logout().await });
                                }

                                ui.add(egui::Separator::default().spacing(0.0));

                                if ui.text_button("exit loqui").clicked() {
                                    std::process::exit(0);
                                }
                            });
                        });
                    });
                });
        }

        if self.state.latest_errors.is_empty().not() {
            let latest_errors = &self.state.latest_errors;
            egui::Window::new("last error")
                .open(&mut self.last_error)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for error in latest_errors.iter() {
                            ui.label(error);
                            ui.separator();
                        }
                    });
                });
        }

        self.screens.current_mut().update(ctx, frame, &mut self.state);
        if let Some(screen) = self.state.next_screen.take() {
            self.screens.push_boxed(screen);
        } else if self.state.prev_screen {
            self.screens.pop();
            self.state.prev_screen = false;
        }
    }
}
