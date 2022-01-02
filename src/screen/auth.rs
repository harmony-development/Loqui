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

use crate::screen::main;

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
                        spawn_evs!(state, |events, client| async move {
                            client.initial_sync(events).await?;
                            Ok(())
                        });
                        let client = state.client().clone();
                        spawn_future!(state, async move { client.connect_socket(Vec::new()).await });
                        let client = state.client().clone();
                        spawn_future!(state, async move { client.save_session_to().await });
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
                        if state.client().auth_status().is_authenticated() {
                            spawn_future!(state, std::future::ready(ClientResult::Ok(Option::<AuthStep>::None)));
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
            spawn_future!(state, async move {
                let client = Client::new(homeserver_url, None).await?;
                client.inner().begin_auth().await?;
                ClientResult::Ok(Some(client))
            });
            self.waiting = true;
        });
    }

    fn prev_step(&mut self, state: &mut State) {
        let fut = state.client().inner().prev_auth_step();
        spawn_future!(state, fut.map_ok(|resp| resp.step).map_err(ClientError::from));
        self.waiting = true;
    }

    fn next_step(&mut self, state: &mut State, response: AuthStepResponse) {
        let fut = state.client().inner().next_auth_step(response);
        spawn_future!(
            state,
            fut.map_ok(|resp| resp.and_then(|resp| resp.step))
                .map_err(ClientError::from)
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

        if let Some(choice) = selected_choice {
            self.next_step(state, AuthStepResponse::Choice(choice.into()));
        } else if self.fields.is_empty().not() && (did_submit || ui.button("continue").clicked()) {
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

        if self.can_go_back && ui.button("back").clicked() {
            self.prev_step(state);
        }
    }

    fn view_main(&mut self, state: &mut State, ui: &mut Ui) {
        if self.waiting {
            ui.label(RichText::new("please wait...").heading());
            return;
        }

        egui::Grid::new("auth_grid")
            .spacing((0.0, 15.0))
            .min_col_width(300.0)
            .show(ui, |ui| {
                self.view_grid(state, ui);
            });
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, _: &epi::Frame, state: &mut State) {
        self.handle_connect(state);
        self.handle_step(state);

        egui::TopBottomPanel::top("auth_title").show(ctx, |ui| {
            ui.label(RichText::new(self.title.as_str()).strong().heading());
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(
                Layout::from_main_dir_and_cross_align(egui::Direction::LeftToRight, egui::Align::Center),
                |ui| {
                    ui.add_space(50.0);
                    self.view_main(state, ui);
                },
            )
        });
    }
}
