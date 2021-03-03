use crate::{
    label, label_button, length, space,
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR, PADDING, SUCCESS_COLOR},
    },
};

#[derive(Debug, Clone)]
pub enum ChannelState {
    Created {
        guild_id: u64,
        channel_id: u64,
        name: String,
    },
    Creating {
        name: String,
    },
    None,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState::None
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    ChannelNameChanged(String),
    CreateChannel,
    CreatedChannel { guild_id: u64, channel_id: u64 },
    GoBack,
}

#[derive(Default, Debug)]
pub struct ChannelCreationModal {
    create_channel_back_but_state: button::State,
    channel_name_textedit_state: text_input::State,
    channel_create_but_state: button::State,
}

impl ChannelCreationModal {
    pub fn view(
        &mut self,
        theme: Theme,
        channel_creation_state: ChannelState,
        channel_name_field: String,
        error_text: String,
    ) -> Element<Message> {
        let mut create_text_edit = TextInput::new(
            &mut self.channel_name_textedit_state,
            "Enter a channel name...",
            &channel_name_field,
            Message::ChannelNameChanged,
        )
        .padding(PADDING / 2)
        .style(theme);

        let mut create = label_button!(&mut self.channel_create_but_state, "Create").style(theme);
        let mut back = label_button!(&mut self.create_channel_back_but_state, "Back").style(theme);

        let mut create_widgets = Vec::with_capacity(3);

        if matches!(
            &channel_creation_state,
            ChannelState::None
                | ChannelState::Created {
                    guild_id: _,
                    channel_id: _,
                    name: _
                }
        ) {
            back = back.on_press(Message::GoBack);

            if !channel_name_field.is_empty() {
                create_text_edit = create_text_edit.on_submit(Message::CreateChannel);
                create = create.on_press(Message::CreateChannel);
            }
        }

        if let ChannelState::Created { name, .. } = &channel_creation_state {
            create_widgets.push(
                label!("Successfully created channel {}", name)
                    .color(SUCCESS_COLOR)
                    .into(),
            );
        }

        if let ChannelState::Creating { name } = channel_creation_state {
            create_widgets.push(label!("Creating channel {}", name).into());
        }

        if !error_text.is_empty() {
            create_widgets.push(label!(error_text).size(18).color(ERROR_COLOR).into());
        }

        create_widgets.push(space!(h+).into());
        create_widgets.push(create_text_edit.into());
        create_widgets.push(
            row(vec![
                create.width(length!(+)).into(),
                space!(w+).into(),
                back.width(length!(+)).into(),
            ])
            .width(length!(+))
            .into(),
        );

        row(vec![
            space!(w % 3).into(),
            column(vec![
                space!(h % 3).into(),
                fill_container(column(create_widgets).width(length!(+)).height(length!(+)))
                    .height(length!(% 4))
                    .style(theme.round())
                    .into(),
                space!(h % 3).into(),
            ])
            .width(length!(% 4))
            .height(length!(+))
            .into(),
            space!(w % 3).into(),
        ])
        .height(length!(+))
        .width(length!(+))
        .into()
    }
}
