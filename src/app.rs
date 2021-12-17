use std::sync::{mpsc, Arc};

use client::{
    content::ContentStore,
    harmony_rust_sdk::{
        api::chat::{Event, EventSource},
        client::{EventsReadSocket, EventsSocket, EventsWriteSocket},
    },
    Cache, Client,
};
use eframe::{
    egui::{self, RichText},
    epi,
};
use tokio::sync::mpsc as tokio_mpsc;

use super::utils::*;

use crate::screen::{auth, BoxedScreen, Screen, ScreenStack};

pub struct State {
    pub socket_tx_tx: tokio_mpsc::Sender<EventsWriteSocket>,
    pub socket_rx_tx: tokio_mpsc::Sender<EventsReadSocket>,
    pub socket_event_rx: mpsc::Receiver<Event>,
    pub client: Option<Client>,
    pub cache: Cache,
    pub futures: futures::Futures,
    pub content_store: Arc<ContentStore>,
    pub latest_error: Option<Error>,
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

    pub fn run<F, E, O>(&mut self, res: Result<O, E>, f: F)
    where
        F: FnOnce(&mut Self, O),
        E: std::error::Error + Send + Sync + 'static,
    {
        match res {
            Ok(val) => f(self, val),
            Err(err) => self.latest_error = Some(anyhow::Error::new(err)),
        }
    }
}

pub struct App {
    state: State,
    screens: ScreenStack,
}

impl App {
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn new(content_store: ContentStore) -> Self {
        let mut cache = Cache::default();
        let futures = futures::Futures::new();
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
                    else => tokio::task::yield_now().await,
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
                    else => tokio::task::yield_now().await,
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
                futures,
                content_store: Arc::new(content_store),
                latest_error: None,
                next_screen: None,
                prev_screen: false,
            },
            screens: ScreenStack::new(auth::Screen::new()),
        }
    }
}

impl epi::App for App {
    fn name(&self) -> &str {
        "loqui"
    }

    fn setup(&mut self, _ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>, _storage: Option<&dyn epi::Storage>) {
        self.state.futures.init(frame);
        if self.state.content_store.latest_session_file().exists() {
            let content_store = self.state.content_store.clone();
            self.state.futures.spawn(async move {
                let session = Client::read_latest_session(content_store.as_ref()).await?;
                let client = Client::new(session.homeserver.parse().unwrap(), Some(session.into())).await?;
                ClientResult::Ok(client)
            });
        }
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
        self.state.futures.run();

        let state = &mut self.state;
        handle_future!(state, |res: ClientResult<Vec<Event>>| {
            state.run(res, |state, events| {
                let mut posts = Vec::new();
                for event in events {
                    state.cache.process_event(&mut posts, event);
                }
                spawn_future!(state, {
                    let client = state.client().clone();
                    async move {
                        let mut events = Vec::with_capacity(posts.len());
                        for post in posts {
                            client.process_post(&mut events, post).await?;
                        }
                        ClientResult::Ok(events)
                    }
                });
            });
        });

        handle_future!(state, |res: ClientResult<EventsSocket>| {
            state.run(res, |state, sock| {
                let (tx, rx) = sock.split();
                state.socket_tx_tx.try_send(tx);
                state.socket_rx_tx.try_send(rx);
            });
        });

        // handle socket events
        let mut evs = Vec::new();
        while let Ok(ev) = state.socket_event_rx.try_recv() {
            evs.push(ev);
        }
        if !evs.is_empty() {
            spawn_future!(state, std::future::ready(ClientResult::Ok(evs)));
        }

        ctx.set_pixels_per_point(1.5);
        egui::TopBottomPanel::new(egui::panel::TopBottomSide::Bottom, "bottom_panel")
            .max_height(25.0)
            .min_height(25.0)
            .show(ctx, |ui| {
                let maybe_err_msg = self
                    .state
                    .latest_error
                    .as_ref()
                    .map(|err| format!("last error: {}", err));
                ui.horizontal(|ui| match maybe_err_msg {
                    Some(text) => {
                        if ui.button("clear").clicked() {
                            self.state.latest_error = None;
                        }
                        ui.label(RichText::new(text).color(egui::Color32::RED))
                    }
                    None => ui.label("no errors"),
                });
            });

        self.screens.current_mut().update(ctx, frame, &mut self.state);
        if let Some(screen) = self.state.next_screen.take() {
            self.screens.push_boxed(screen);
        } else if self.state.prev_screen {
            self.screens.pop();
        }
    }
}
