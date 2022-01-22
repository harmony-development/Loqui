use std::ops::Not;

use client::{
    harmony_rust_sdk::{
        api::{
            auth::{auth_step::Step, AuthStep},
            exports::hrpc::exports::futures_util::TryFutureExt,
        },
        client::api::auth::{next_step_request::form_fields::Field, AuthStepResponse},
    },
    smol_str::SmolStr,
    Client, IndexMap, Uri,
};
use eframe::egui::RichText;

use crate::{screen::main, widgets::view_about};

use super::prelude::*;

#[derive(Default)]
pub struct Screen {
    title: SmolStr,
    fields: IndexMap<(SmolStr, SmolStr), String>,
    choices: Vec<SmolStr>,
    can_go_back: bool,
    waiting: bool,
}

impl Screen {
    pub fn new() -> Self {
        let mut this = Self::default();
        this.reset();
        this
    }

    fn reset(&mut self) {
        const HOMESERVER: SmolStr = SmolStr::new_inline("homeserver");

        self.title = HOMESERVER;
        self.fields.clear();
        self.fields
            .insert((HOMESERVER, HOMESERVER), "https://chat.harmonyapp.io:2289".to_string());
        self.choices.clear();
        self.can_go_back = false;
        self.waiting = false;
    }

    fn handle_step(&mut self, state: &mut State) {
        handle_future!(state, |res: ClientResult<Option<AuthStep>>| {
            match res {
                Ok(step) => {
                    if let Some(step) = step {
                        self.fields.clear();
                        self.choices.clear();
                        self.can_go_back = step.can_go_back;
                        self.waiting = false;

                        if let Some(step) = step.step {
                            match step {
                                Step::Choice(choice) => {
                                    self.title = choice.title.into();
                                    self.choices.extend(choice.options.into_iter().map(Into::into));
                                }
                                Step::Form(form) => {
                                    self.title = form.title.into();
                                    self.fields.extend(
                                        form.fields
                                            .into_iter()
                                            .map(|field| ((field.name.into(), field.r#type.into()), String::new())),
                                    );
                                }
                                _ => todo!("Implement waiting"),
                            }
                        }
                    } else {
                        self.reset();
                        state.push_screen(main::Screen::default());
                        state.reset_socket.set(true);
                        spawn_evs!(state, |events, client| {
                            client.initial_sync(events).await?;
                        });
                        spawn_client_fut!(state, |client| client.save_session_to().await);
                    }
                }
                Err(err) => {
                    state.latest_errors.push(err.to_string());
                    state.client = None;
                    self.reset();
                }
            }
        });
    }

    fn handle_connect(&mut self, state: &mut State) {
        handle_future!(state, |res: ClientResult<Option<Client>>| {
            match res {
                Ok(maybe_client) => {
                    if let Some(client) = maybe_client {
                        state.client = Some(client);

                        spawn_client_fut!(state, |client| client.fetch_about().await);
                        if state.client().auth_status().is_authenticated() {
                            state
                                .futures
                                .spawn(std::future::ready(ClientResult::Ok(Option::<AuthStep>::None)));
                        } else {
                            self.next_step(state, AuthStepResponse::Initial);
                        }
                    }
                }
                Err(err) => {
                    state.latest_errors.push(err.to_string());
                    self.reset();
                }
            }
        });
    }

    fn homeserver(&mut self, state: &mut State) {
        let maybe_homeserver_url = self
            .fields
            .first()
            .expect("on homeserver step but actually not?")
            .1
            .parse::<Uri>();

        state.run(maybe_homeserver_url, |state, homeserver_url| {
            state.futures.spawn(async move {
                let client = Client::new(homeserver_url, None).await?;
                client.inner().begin_auth().await?;
                ClientResult::Ok(Some(client))
            });
            self.waiting = true;
        });
    }

    fn prev_step(&mut self, state: &mut State) {
        let fut = state.client().inner().prev_auth_step();
        state
            .futures
            .spawn(fut.map_ok(|resp| resp.step).map_err(ClientError::from));
        self.waiting = true;
    }

    fn next_step(&mut self, state: &mut State, response: AuthStepResponse) {
        let fut = state.client().inner().next_auth_step(response);
        state.futures.spawn(
            fut.map_ok(|resp| resp.and_then(|resp| resp.step))
                .map_err(ClientError::from),
        );
        self.waiting = true;
    }

    fn view_fields(&mut self, ui: &mut Ui) -> bool {
        let mut focus_after = false;
        for ((name, r#type), value) in self.fields.iter_mut() {
            ui.group(|ui| {
                ui.label(name.as_str());
                let edit = egui::TextEdit::singleline(value).password(r#type == "password" || r#type == "new-password");
                let edit = ui.add(edit);
                if focus_after {
                    edit.request_focus();
                }
                focus_after = edit.did_submit(ui);
            });
            ui.end_row();
        }
        focus_after
    }

    fn view_choices(&mut self, ui: &mut Ui) -> Option<SmolStr> {
        let mut selected_choice = None;
        for choice in &self.choices {
            if ui.button(choice.as_str()).clicked() {
                selected_choice = Some(choice.clone());
            }
            ui.end_row();
        }
        selected_choice
    }

    fn view_grid(&mut self, state: &mut State, ui: &mut Ui) {
        let did_submit = self.view_fields(ui);
        let selected_choice = self.view_choices(ui);

        ui.horizontal(|ui| {
            if let Some(choice) = selected_choice {
                self.next_step(state, AuthStepResponse::Choice(choice.into()));
            } else if self.fields.is_empty().not() {
                let are_fields_filled = self.fields.iter().all(|(_, text)| text.is_empty().not());
                let continue_resp = ui.add_enabled(are_fields_filled, egui::Button::new("continue"));
                if are_fields_filled && (did_submit || continue_resp.clicked()) {
                    if self.title == "homeserver" {
                        self.homeserver(state);
                    } else {
                        let response = AuthStepResponse::form(
                            self.fields
                                .iter()
                                .map(|((_, r#type), value)| match r#type.as_str() {
                                    "number" => Field::Number(value.parse().unwrap()),
                                    "new-password" | "password" => Field::Bytes(value.as_bytes().to_vec()),
                                    _ => Field::String(value.clone()),
                                })
                                .collect(),
                        );
                        self.next_step(state, response);
                    }
                }
            }

            if self.can_go_back && ui.button("back").clicked() {
                self.prev_step(state);
            }
        });
    }

    fn view_main(&mut self, state: &mut State, ui: &mut Ui) {
        if self.waiting {
            ui.horizontal(|ui| {
                ui.label(RichText::new("please wait...").heading());
                ui.add(egui::Spinner::new());
            });
            return;
        }

        egui::Grid::new("auth_grid_grid")
            .min_col_width(ui.available_width())
            .spacing([10.0; 2])
            .show(ui, |ui| self.view_grid(state, ui));
    }
}

impl AppScreen for Screen {
    fn id(&self) -> &'static str {
        "auth"
    }

    fn update(&mut self, ctx: &egui::Context, _: &epi::Frame, state: &mut State) {
        self.handle_connect(state);
        self.handle_step(state);

        egui::TopBottomPanel::top("auth_title").show(ctx, |ui| {
            ui.label(RichText::new(self.title.as_str()).strong().heading());
        });

        let is_mobile = ctx.is_mobile();
        egui::CentralPanel::default().show(ctx, |ui| {
            let num_columns = (state.about.is_some() && is_mobile.not()).then(|| 2).unwrap_or(1);
            let col_width = is_mobile
                .then(|| ui.available_width())
                .unwrap_or_else(|| ui.available_width() / num_columns as f32);
            let available_height_before = ui.available_height();

            egui::Grid::new("auth_grid")
                .min_col_width(col_width)
                .min_row_height(available_height_before / 3.0)
                .max_col_width(col_width)
                .num_columns(num_columns)
                .show(ui, |ui| {
                    ui.allocate_ui(
                        [
                            ui.available_width(),
                            is_mobile
                                .then(|| available_height_before / 3.0)
                                .unwrap_or(available_height_before),
                        ]
                        .into(),
                        |ui| self.view_main(state, ui),
                    );
                    if is_mobile {
                        ui.end_row();
                    }
                    if let Some(about) = state.about.as_ref() {
                        if is_mobile {
                            ui.end_row();
                        }
                        ui.group(|ui| {
                            view_about(ui, about);
                        });
                    }
                });
        });
    }
}
