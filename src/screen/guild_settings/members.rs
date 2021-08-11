use client::{error::ClientError, Client};
use iced::Tooltip;
use iced_aw::TabLabel;

use crate::{
    component::*,
    label_button, length,
    screen::{guild_settings::Message as ParentMessage, Message as TopLevelMessage, Screen as TopLevelScreen},
    style::{Theme, ERROR_COLOR, PADDING, SPACING},
};

use super::{GuildMetadata, Tab};

#[derive(Debug, Clone)]
pub enum MembersMessage {
    GoBack,
}

#[derive(Debug, Default, Clone)]
pub struct MembersTab {
    button_states: Vec<(button::State, button::State, button::State)>,
    member_list_state: scrollable::State,
    back_but_state: button::State,
    pub error_message: String,
}

impl MembersTab {
    pub fn update(
        &mut self,
        message: MembersMessage,
        _: &Client,
        _: &mut GuildMetadata,
        _: u64,
    ) -> Command<TopLevelMessage> {
        match message {
            MembersMessage::GoBack => TopLevelScreen::pop_screen_cmd(),
        }
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_message = error.to_string();
        Command::none()
    }
}

impl Tab for MembersTab {
    type Message = ParentMessage;

    fn title(&self) -> String {
        String::from("Members")
    }

    fn tab_label(&self) -> TabLabel {
        TabLabel::IconText(Icon::Person.into(), self.title())
    }

    fn content(
        &mut self,
        client: &Client,
        guild_id: u64,
        _: &mut GuildMetadata,
        theme: Theme,
        _: &ThumbnailCache,
    ) -> Element<'_, ParentMessage> {
        let mut members = Scrollable::new(&mut self.member_list_state)
            .align_items(Align::Start)
            .height(length!(+))
            .width(length!(+))
            .padding(PADDING)
            .spacing(SPACING)
            .style(theme);

        if let Some(guild) = client.guilds.get(&guild_id) {
            self.button_states.resize_with(guild.members.len(), Default::default);
            for ((member_id, _), (copy_state, copy_name_state, edit_state)) in
                guild.members.iter().zip(&mut self.button_states)
            {
                let member = match client.members.get(member_id) {
                    Some(member) => member,
                    _ => continue,
                };
                let member_id = *member_id;

                let mut content_widgets = Vec::with_capacity(6);
                content_widgets.push(
                    Tooltip::new(
                        label_button!(copy_name_state, member.username.as_str())
                            .style(theme)
                            .on_press(ParentMessage::CopyToClipboard(member.username.to_string())),
                        "Click to copy",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
                content_widgets.push(
                    Tooltip::new(
                        label_button!(copy_state, format!("ID {}", member_id))
                            .style(theme)
                            .on_press(ParentMessage::CopyIdToClipboard(member_id)),
                        "Click to copy",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );
                content_widgets.push(space!(w+).into());
                content_widgets.push(
                    Tooltip::new(
                        Button::new(edit_state, icon(Icon::Pencil))
                            .style(theme)
                            .on_press(ParentMessage::ShowManageUserRoles(member_id)),
                        "Edit member roles",
                        iced::tooltip::Position::Top,
                    )
                    .style(theme)
                    .into(),
                );

                members = members.push(Container::new(row(content_widgets)).style(theme));
            }
        }

        let mut content = Vec::with_capacity(3);

        if !self.error_message.is_empty() {
            content.push(label!(self.error_message.as_str()).color(ERROR_COLOR).into())
        }
        content.push(fill_container(members).style(theme).into());
        content.push(
            label_button!(&mut self.back_but_state, "Back")
                .on_press(ParentMessage::Members(MembersMessage::GoBack))
                .style(theme)
                .into(),
        );

        Container::new(column(content)).padding(PADDING * 10).into()
    }
}
