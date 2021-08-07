use std::convert::identity;

use super::{super::Screen as TopLevelScreen, GuildMetaData};
use crate::{
    client::error::ClientError,
    component::*,
    label_button, length,
    screen::{
        guild_settings::{Message as ParentMessage, Tab},
        Client, ClientExt, Message as TopLevelMessage,
    },
    space,
    style::{Theme, DEF_SIZE, ERROR_COLOR, PADDING},
};
use client::{
    error::ClientResult,
    harmony_rust_sdk::{
        api::exports::hrpc::url::Url,
        client::api::chat::invite::{
            create_invite, delete_invite, get_guild_invites_response::Invite, CreateInviteRequest, DeleteInviteRequest,
        },
    },
};
use iced::Element;
use iced_aw::{Icon, TabLabel};

#[derive(Debug, Clone)]
pub enum InviteMessage {
    InviteNameChanged(String),
    InviteUsesChanged(String),
    CreateInvitePressed,
    InviteCreated((String, i32)),
    InvitesLoaded(Vec<Invite>),
    GoBack,
    DeleteInvitePressed(usize),
    InviteDeleted(usize),
}

#[derive(Debug, Default, Clone)]
pub struct InviteTab {
    invite_name_state: text_input::State,
    invite_name_value: String,
    invite_uses_state: text_input::State,
    invite_uses_value: String,
    create_invite_but_state: button::State,
    invite_list_state: scrollable::State,
    back_but_state: button::State,
    delete_invite_but_states: Vec<button::State>,
    pub error_message: String,
}

impl InviteTab {
    pub fn update(
        &mut self,
        message: InviteMessage,
        client: &Client,
        meta_data: &mut GuildMetaData,
        guild_id: u64,
    ) -> Command<TopLevelMessage> {
        match message {
            InviteMessage::InviteNameChanged(s) => {
                self.invite_name_value = s;
            }
            InviteMessage::InviteUsesChanged(s) => {
                self.invite_uses_value = s;
            }
            InviteMessage::CreateInvitePressed => {
                let uses = self.invite_uses_value.clone();
                let name = self.invite_name_value.clone();
                return client.mk_cmd(
                    |inner| async move {
                        let uses: i32 = match uses.parse() {
                            Ok(val) => val,
                            Err(err) => return Err(ClientError::Custom(err.to_string())),
                        };
                        let request = CreateInviteRequest {
                            name,
                            possible_uses: uses,
                            guild_id,
                        };
                        let name = create_invite(&inner, request).await?.name;
                        Ok(TopLevelMessage::guild_settings(ParentMessage::Invite(
                            InviteMessage::InviteCreated((name, uses)),
                        )))
                    },
                    identity,
                );
            }
            InviteMessage::InvitesLoaded(invites) => {
                // Triggered if Invites are loaded, loading is started in guild_settings, as soon as the invite-tab
                // is selected
                meta_data.invites = Some(invites);
            }
            InviteMessage::InviteCreated((name, uses)) => {
                let new_invite = Invite {
                    invite_id: name,
                    possible_uses: uses,
                    use_count: 0,
                };
                if let Some(invites) = &mut meta_data.invites {
                    invites.push(new_invite);
                    self.invite_name_value.clear();
                    self.invite_uses_value.clear();
                } else {
                    meta_data.invites = Some(vec![new_invite]);
                }
            }
            InviteMessage::GoBack => {
                // Return to main screen
                return TopLevelScreen::push_screen_cmd(TopLevelScreen::Main(Box::new(
                    super::super::MainScreen::default(),
                )));
            }
            InviteMessage::DeleteInvitePressed(n) => {
                let invite_id = meta_data.invites.as_ref().unwrap()[n].invite_id.clone();
                return client.mk_cmd(
                    |inner| async move {
                        delete_invite(&inner, DeleteInviteRequest { guild_id, invite_id }).await?;
                        ClientResult::Ok(TopLevelMessage::guild_settings(ParentMessage::Invite(
                            InviteMessage::InviteDeleted(n),
                        )))
                    },
                    identity,
                );
            }
            InviteMessage::InviteDeleted(n) => {
                meta_data.invites.as_mut().unwrap().remove(n);
            }
        }

        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_message = error.to_string();
        Command::none()
    }
}

impl Tab for InviteTab {
    fn title(&self) -> String {
        String::from("Invites")
    }

    fn tab_label(&self) -> TabLabel {
        TabLabel::IconText(Icon::Heart.into(), self.title())
    }

    fn content(
        &mut self,
        client: &Client,
        meta_data: &mut GuildMetaData,
        theme: Theme,
        _: &ThumbnailCache,
    ) -> Element<'_, ParentMessage> {
        let mut widgets = Vec::with_capacity(10);
        if !self.error_message.is_empty() {
            widgets.push(label!(&self.error_message).color(ERROR_COLOR).size(DEF_SIZE + 2).into());
        }
        // If there are any invites, create invite list
        if let Some(invites) = &meta_data.invites {
            // Create header for invite list
            let invites_table = row(vec![
                label!("Invite Id").width(length!(% 19)).into(),
                space!(w % 20).into(),
                label!("Possible uses").width(length!(% 4)).into(),
                space!(w % 3).into(),
                label!("Uses").width(length!(% 3)).into(),
                space!(w % 3).into(),
            ]);
            let homeserver_url = client.inner().homeserver_url();
            let mut url = Url::parse(
                format!(
                    "harmony://{}:{}/",
                    homeserver_url.host().unwrap(),
                    homeserver_url.port().unwrap_or(2289)
                )
                .as_str(),
            )
            .unwrap();
            self.delete_invite_but_states
                .resize_with(invites.len(), Default::default);
            let mut invites_scrollable = Scrollable::new(&mut self.invite_list_state)
                .style(theme)
                .align_items(Align::Center);
            // Create each line of the invite list
            for (n, (cur_invite, del_but_state)) in
                invites.iter().zip(self.delete_invite_but_states.iter_mut()).enumerate()
            {
                url.set_path(&cur_invite.invite_id);
                invites_scrollable = invites_scrollable.push(row(vec![
                    label!(url.as_str()).width(length!(% 19)).into(),
                    space!(w % 22).into(),
                    label!(cur_invite.possible_uses.to_string()).width(length!(% 3)).into(),
                    space!(w % 3).into(),
                    label!(cur_invite.use_count.to_string()).width(length!(% 3)).into(),
                    space!(w % 1).into(),
                    Button::new(del_but_state, icon(Icon::Trash))
                        .style(theme)
                        .on_press(ParentMessage::Invite(InviteMessage::DeleteInvitePressed(n)))
                        .into(),
                ]));
            }
            widgets.push(invites_table.into());
            widgets.push(invites_scrollable.into());
        // If there aren't any invites
        } else {
            widgets.push(label!("Fetching invites").into());
        }
        widgets.push(space!(h = 20).into());
        // Invite Creation fields
        widgets.push(
            row(vec![
                TextInput::new(
                    &mut self.invite_name_state,
                    "Enter invite name...",
                    self.invite_name_value.as_str(),
                    |s| ParentMessage::Invite(InviteMessage::InviteNameChanged(s)),
                )
                .style(theme)
                .padding(PADDING / 2)
                .into(),
                TextInput::new(
                    &mut self.invite_uses_state,
                    "Enter possible uses...",
                    self.invite_uses_value.as_str(),
                    |s| ParentMessage::Invite(InviteMessage::InviteUsesChanged(s)),
                )
                .width(length!(= 200))
                .padding(PADDING / 2)
                .style(theme)
                .into(),
                Button::new(&mut self.create_invite_but_state, label!("Create"))
                    .style(theme)
                    .on_press(ParentMessage::Invite(InviteMessage::CreateInvitePressed))
                    .into(),
            ])
            .into(),
        );
        widgets.push(space!(h = 20).into());
        //Back button
        widgets.push(
            label_button!(&mut self.back_but_state, "Back")
                .style(theme)
                .on_press(ParentMessage::Invite(InviteMessage::GoBack))
                .into(),
        );

        column(widgets).into()
    }
}
