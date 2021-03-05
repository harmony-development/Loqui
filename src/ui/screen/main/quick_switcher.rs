use crate::{
    label_button, length, space,
    ui::{
        component::*,
        style::{Theme, DEF_SIZE, PADDING},
    },
};

#[derive(Debug, Clone)]
pub enum Message {
    SwitchToChannel { guild_id: u64, channel_id: u64 },
    SwitchToGuild(u64),
    SearchTermChanged(String),
}

#[derive(Debug, Clone)]
pub enum SearchResult {
    Guild {
        id: u64,
        name: String,
    },
    Channel {
        guild_id: u64,
        id: u64,
        name: String,
    },
}

#[derive(Debug, Default)]
pub struct QuickSwitcherModal {
    search_state: text_input::State,
    results_buts_state: [button::State; 8],
    pub search_value: String,
    pub results: Vec<SearchResult>,
}

impl QuickSwitcherModal {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        self.search_state.focus();

        let mut result_widgets = Vec::with_capacity(self.results.len());

        let mut search_bar = TextInput::new(
            &mut self.search_state,
            "Search guilds, channels",
            &self.search_value,
            Message::SearchTermChanged,
        )
        .padding(PADDING / 2)
        .size(DEF_SIZE + 4)
        .style(theme);

        if let Some(result) = self.results.first() {
            let msg = match result {
                SearchResult::Guild { id, name: _ } => Message::SwitchToGuild(*id),
                SearchResult::Channel {
                    guild_id,
                    id,
                    name: _,
                } => Message::SwitchToChannel {
                    guild_id: *guild_id,
                    channel_id: *id,
                },
            };
            search_bar = search_bar.on_submit(msg);
        }

        result_widgets.push(search_bar.into());

        for (result, but_stt) in self.results.iter().zip(self.results_buts_state.iter_mut()) {
            let widget = match result {
                SearchResult::Guild { id, name } => label_button!(but_stt, &format!("* {}", name))
                    .style(theme)
                    .on_press(Message::SwitchToGuild(*id)),
                SearchResult::Channel { guild_id, id, name } => {
                    label_button!(but_stt, &format!("# {}", name))
                        .style(theme)
                        .on_press(Message::SwitchToChannel {
                            guild_id: *guild_id,
                            channel_id: *id,
                        })
                }
            };

            result_widgets.push(widget.into());
        }

        Row::with_children(vec![
            space!(w % 2).into(),
            Column::with_children(vec![
                space!(h % 2).into(),
                Container::new(column(result_widgets))
                    .style(theme.round())
                    .height(length!(%6))
                    .into(),
                space!(h % 2).into(),
            ])
            .width(length!(%6))
            .into(),
            space!(w % 2).into(),
        ])
        .into()
    }
}
