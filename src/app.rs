use std::sync::Arc;

use client::{content::ContentStore, Client};
use eframe::{
    egui::{self, RichText},
    epi,
};

use super::utils::*;

use crate::screen::{auth, ScreenStack};

pub struct State {
    pub client: Option<Client>,
    pub futures: futures::Futures,
    pub content_store: Arc<ContentStore>,
    pub latest_error: Option<Error>,
}

impl State {
    pub fn client(&mut self) -> &mut Client {
        self.client.as_mut().expect("client not initialized yet")
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
    pub fn new(content_store: ContentStore) -> Self {
        Self {
            state: State {
                client: None,
                futures: futures::Futures::default(),
                content_store: Arc::new(content_store),
                latest_error: None,
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
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
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
    }
}
