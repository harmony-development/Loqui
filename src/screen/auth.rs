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

use super::prelude::*;

#[derive(Default)]
pub struct Screen {
    title: SmolStr,
    fields: IndexMap<(SmolStr, SmolStr), String>,
    choices: Vec<SmolStr>,
    can_go_back: bool,
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
    }
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, _: &mut epi::Frame<'_>, state: &mut State) {
        handle_future!(state, HomeserverFut, |client_res: ClientResult<Client>| {
            state.run(client_res, |state, client| {
                let fut = client.inner().next_auth_step(AuthStepResponse::Initial);
                state.client = Some(client);
                spawn_future!(state, StepFut, fut.map_ok(|resp| resp.and_then(|resp| resp.step)));
            });
        });
        handle_future!(state, StepFut, |res: ClientResult<Option<AuthStep>>| {
            state.run(res, |_, step| match step {
                Some(step) => {
                    println!("got step: {:?}", step);
                    self.fields.clear();
                    self.choices.clear();
                    self.can_go_back = step.can_go_back;

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
                }
                None => {
                    todo!()
                }
            });
        });

        let Self {
            title,
            fields,
            choices,
            can_go_back,
        } = self;

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Grid::new("auth_grid").min_col_width(300.0).show(ui, |ui| {
                let mut focus_after = false;
                for ((name, _type), value) in fields.iter_mut() {
                    ui.group(|ui| {
                        ui.label(name.as_str());
                        let edit = ui.text_edit_singleline(value);
                        if focus_after {
                            edit.request_focus();
                        }
                        focus_after = edit.did_submit(ui);
                    });
                    ui.end_row();
                }

                let mut chosen_choice = None;
                for choice in choices.iter() {
                    if ui.button(choice.as_str()).clicked() {
                        chosen_choice = Some(choice.clone());
                    }
                    ui.end_row();
                }

                if let Some(choice) = chosen_choice {
                    let fut = state
                        .client()
                        .inner()
                        .next_auth_step(AuthStepResponse::Choice(choice.into()));
                    spawn_future!(state, StepFut, fut.map_ok(|resp| resp.and_then(|resp| resp.step)));
                } else if fields.is_empty().not() && ui.button("continue").clicked() {
                    if title == "homeserver" {
                        handle_homeserver(state, fields);
                    } else {
                        let response = AuthStepResponse::form(
                            fields
                                .iter()
                                .map(|((_, r#type), value)| match r#type.as_str() {
                                    "number" => Field::Number(value.parse().unwrap()),
                                    "new-password" | "password" => Field::Bytes(value.as_bytes().to_vec()),
                                    _ => Field::String(value.clone()),
                                })
                                .collect(),
                        );
                        let fut = state.client().inner().next_auth_step(response);
                        spawn_future!(state, StepFut, fut.map_ok(|resp| resp.and_then(|resp| resp.step)));
                    }
                }

                if *can_go_back && ui.button("back").clicked() {
                    let fut = state.client().inner().prev_auth_step();
                    spawn_future!(state, StepFut, fut.map_ok(|resp| resp.step));
                }
            });
        });
    }
}

struct HomeserverFut;
struct StepFut;

fn handle_homeserver(state: &mut State, fields: &IndexMap<(SmolStr, SmolStr), String>) {
    let maybe_homeserver_url = fields
        .first()
        .expect("on homeserver step but actually not?")
        .1
        .parse::<Uri>();

    state.run(maybe_homeserver_url, |state, homeserver_url| {
        let content_store = state.content_store.clone();
        spawn_future!(state, HomeserverFut, async move {
            let client = Client::new(homeserver_url, None, content_store).await?;
            client.inner().begin_auth().await?;
            ClientResult::Ok(client)
        });
    });
}
