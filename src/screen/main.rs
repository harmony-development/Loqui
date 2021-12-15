use client::harmony_rust_sdk::api::{
    chat::{
        stream_event::{ChannelCreated, Event as ChatEvent},
        Event, GetGuildChannelsRequest,
    },
    exports::hrpc::exports::futures_util::TryFutureExt,
};
use eframe::egui::RichText;

use super::prelude::*;

#[derive(Default)]
pub struct Screen {
    current_guild: Option<u64>,
    current_channel: Option<u64>,
}

impl Screen {
    fn view_guilds(&mut self, state: &mut State, ui: &mut Ui) {
        for (id, guild) in state.client().guilds.iter() {
            let guild_id = *id;

            let icon = RichText::new(guild.name.get(0..1).unwrap_or("u").to_ascii_uppercase()).strong();

            let is_enabled = self.current_guild != Some(guild_id);

            let button = ui
                .add_enabled_ui(is_enabled, |ui| ui.add_sized([32.0, 32.0], egui::Button::new(icon)))
                .inner
                .on_hover_text(&guild.name);

            if button.clicked() {
                self.current_guild = Some(guild_id);
                state.spawn_cmd(move |client| {
                    let fut = client.inner().call(GetGuildChannelsRequest::new(guild_id));
                    fut.map_ok(move |resp| {
                        resp.channels
                            .into_iter()
                            .filter_map(|channel| {
                                let channel_id = channel.channel_id;
                                let channel = channel.channel?;
                                Some(Event::Chat(ChatEvent::new_created_channel(ChannelCreated {
                                    guild_id,
                                    channel_id,
                                    name: channel.channel_name,
                                    kind: channel.kind,
                                    position: None,
                                    metadata: channel.metadata,
                                })))
                            })
                            .collect()
                    })
                    .map_err(Into::into)
                });
            }
            ui.separator();
        }
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        let maybe_guild = self.current_guild.and_then(|id| state.client().guilds.get(&id));
        if let Some(channels) = maybe_guild.map(|g| g.channels.iter()) {
            for (id, channel) in channels {
                let mut text = RichText::new(format!("#{}", channel.name));
                if channel.has_unread {
                    text = text.strong();
                }
                let is_enabled = !channel.is_category && (self.current_channel != Some(*id));
                let but = ui.add_enabled(is_enabled, egui::Button::new(text));
                if but.clicked() {
                    self.current_channel = Some(*id);
                }
                ui.add_space(8.0);
            }
        }
    }

    fn view_messages(&mut self, state: &State, ui: &mut Ui) {}

    fn view_composer(&mut self, state: &State, ui: &mut Ui) {}

    fn view_members(&mut self, state: &State, ui: &mut Ui) {}
}

impl AppScreen for Screen {
    fn update(&mut self, ctx: &egui::CtxRef, _: &mut epi::Frame, state: &mut State) {
        egui::panel::SidePanel::left("guild_panel")
            .min_width(32.0)
            .max_width(32.0)
            .resizable(false)
            .show(ctx, |ui| self.view_guilds(state, ui));
        egui::panel::SidePanel::left("channel_panel")
            .min_width(175.0)
            .max_width(500.0)
            .resizable(true)
            .show(ctx, |ui| self.view_channels(state, ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(
                Layout::from_main_dir_and_cross_align(egui::Direction::LeftToRight, egui::Align::Center),
                |ui| {
                    ui.vertical(|ui| {
                        self.view_messages(state, ui);
                        self.view_composer(state, ui);
                    });
                },
            );
        });

        egui::panel::SidePanel::right("member_panel")
            .min_width(175.0)
            .max_width(500.0)
            .resizable(true)
            .show(ctx, |ui| self.view_members(state, ui));
    }
}
