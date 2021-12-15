use std::{cell::RefCell, future::Future, sync::Arc};

use client::{content::ContentStore, harmony_rust_sdk::api::chat::Event, Client};
use eframe::{
    egui::{self, RichText},
    epi,
};

use super::utils::*;

use crate::screen::{auth, future_markers, BoxedScreen, Screen, ScreenStack};

pub struct State {
    pub client: Option<Client>,
    pub futures: RefCell<futures::Futures>,
    pub content_store: Arc<ContentStore>,
    pub latest_error: Option<Error>,
    next_screen: Option<BoxedScreen>,
    prev_screen: bool,
}

impl State {
    pub fn client(&self) -> &Client {
        self.client.as_ref().expect("client not initialized yet")
    }

    pub fn client_mut(&mut self) -> &mut Client {
        self.client.as_mut().expect("client not initialized yet")
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

    pub fn spawn_cmd<F, Fut>(&self, f: F)
    where
        F: FnOnce(&Client) -> Fut,
        Fut: Future<Output = ClientResult<Vec<Event>>> + Send + 'static,
    {
        let fut = f(self.client());
        spawn_future!(self, future_markers::ProcessEvents, fut);
    }
}

pub struct App {
    state: State,
    screens: ScreenStack,
}

impl App {
    #[must_use]
    pub fn new(content_store: ContentStore) -> Self {
        Self {
            state: State {
                client: None,
                futures: RefCell::new(futures::Futures::default()),
                content_store: Arc::new(content_store),
                latest_error: None,
                next_screen: None,
                prev_screen: false,
            },
            screens: ScreenStack::new(auth::Screen::new()),
        }
    }

    fn handle_initial_sync(&mut self) {
        let state = &mut self.state;
        handle_future!(state, future_markers::InitialSync, |res: ClientResult<Vec<Event>>| {
            self.state.run(res, |state, events| {
                let client = state.client_mut();
                let posts = client.process_event(events);
                let fut = post_process_events(client, posts);
                spawn_future!(state, future_markers::ProcessEvents, fut);
            });
        });
    }

    fn handle_process_events(&mut self) {
        let state = &mut self.state;
        handle_future!(state, future_markers::ProcessEvents, |res: ClientResult<Vec<Event>>| {
            self.state.run(res, |state, events| {
                let client = state.client_mut();
                let posts = client.process_event(events);
                let fut = post_process_events(client, posts);
                spawn_future!(state, future_markers::ProcessEvents, fut);
            });
        });
    }
}

impl epi::App for App {
    fn name(&self) -> &str {
        "loqui"
    }

    fn setup(&mut self, _ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>, _storage: Option<&dyn epi::Storage>) {
        self.state.futures.borrow_mut().init(frame);
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
        self.handle_initial_sync();
        self.handle_process_events();

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
