use client::harmony_rust_sdk::{
    api::{
        chat::{
            stream_event::{ChannelCreated, Event as ChatEvent, MessageSent},
            Event, GetGuildChannelsRequest,
        },
        exports::hrpc::exports::futures_util::TryFutureExt,
    },
    client::api::chat::channel::GetChannelMessages,
};
use eframe::egui::RichText;

use super::prelude::*;

#[derive(Default)]
pub struct Screen {
    current_guild: Option<u64>,
    current_channel: Option<u64>,
    composer_text: String,
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
                if guild.channels.is_empty() {
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
            }
            ui.separator();
        }
    }

    fn view_channels(&mut self, state: &mut State, ui: &mut Ui) {
        let (guild_id, guild) = if let Some(val) = self
            .current_guild
            .and_then(|id| Some((id, state.client().guilds.get(&id)?)))
        {
            val
        } else {
            return;
        };

        for (id, channel) in guild.channels.iter() {
            let channel_id = *id;
            let mut text = RichText::new(format!("#{}", channel.name));
            if channel.has_unread {
                text = text.strong();
            }
            let is_enabled = !channel.is_category && (self.current_channel != Some(channel_id));
            let button = ui.add_enabled(is_enabled, egui::Button::new(text));
            if button.clicked() {
                self.current_channel = Some(channel_id);
                if !channel.reached_top && channel.messages.is_empty() {
                    state.spawn_cmd(move |client| {
                        let fut = client.inner().call(GetChannelMessages::new(guild_id, channel_id));
                        fut.map_ok(move |resp| {
                            resp.messages
                                .into_iter()
                                .rev()
                                .map(move |message| {
                                    let message_id = message.message_id;
                                    Event::Chat(ChatEvent::new_sent_message(Box::new(MessageSent {
                                        guild_id,
                                        channel_id,
                                        message_id,
                                        echo_id: None,
                                        message: message.message,
                                    })))
                                })
                                .collect()
                        })
                        .map_err(ClientError::from)
                    });
                }
            }
            ui.add_space(8.0);
        }
    }

    fn view_messages(&mut self, state: &State, ui: &mut Ui) {
        let maybe_chan = self
            .current_guild
            .zip(self.current_channel)
            .and_then(|(guild_id, channel_id)| {
                let guild = state.client().guilds.get(&guild_id)?;
                let channel = guild.channels.get(&channel_id)?;
                Some((guild_id, channel_id, guild, channel))
            });
        let (_, channel_id, _, channel) = if let Some(val) = maybe_chan {
            val
        } else {
            return;
        };

        egui::ScrollArea::vertical()
            .stick_to_right()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (id, message) in channel.messages.iter() {
                    ui.group(|ui| {
                        ui.label(RichText::new(format!("{}", message.sender)).italics());
                        match &message.content {
                            client::message::Content::Text(text) => {
                                ui.label(text);
                            }
                            client::message::Content::Files(_) => {}
                            client::message::Content::Embeds(_) => {}
                        }
                    });
                }
            });
    }

    fn view_composer(&mut self, state: &State, ui: &mut Ui) {
        ui.text_edit_multiline(&mut self.composer_text);
    }

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
        egui::panel::SidePanel::right("member_panel")
            .min_width(175.0)
            .max_width(500.0)
            .resizable(true)
            .show(ctx, |ui| self.view_members(state, ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(
                Layout::from_main_dir_and_cross_align(egui::Direction::LeftToRight, egui::Align::Center),
                |ui| {
                    ui.vertical(|ui| {
                        self.view_messages(state, ui);
                        ui.separator();
                        self.view_composer(state, ui);
                    });
                },
            );
        });
    }
}
