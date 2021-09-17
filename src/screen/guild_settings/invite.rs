use std::convert::identity;

use super::{super::Screen as TopLevelScreen, GuildMetadata};
use crate::{
    client::error::ClientError,
    component::*,
    label_button, length,
    screen::{
        guild_settings::{Message as ParentMessage, Tab},
        Client, ClientExt, Message as TopLevelMessage,
    },
    space,
    style::{Theme, DEF_SIZE, PADDING},
};
use client::{
    error::ClientResult,
    harmony_rust_sdk::{
        api::{
            chat::{
                all_permissions::{INVITES_MANAGE_CREATE, INVITES_MANAGE_DELETE, INVITES_VIEW},
                Invite, InviteWithId,
            },
            exports::hrpc::url::Url,
        },
        client::api::chat::invite::{CreateInviteRequest, DeleteInviteRequest},
    },
};
use iced::{Element, Tooltip};
use iced_aw::{Icon, TabLabel};

#[derive(Debug, Clone)]
pub enum InviteMessage {
    InviteNameChanged(String),
    InviteUsesChanged(String),
    CreateInvitePressed,
    InviteCreated(String, u32),
    InvitesLoaded(Vec<InviteWithId>),
    GoBack,
    DeleteInvitePressed(usize),
    InviteDeleted(usize),
    CopyToClipboard(String),
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
    but_states: Vec<(button::State, button::State)>,
    pub error_message: String,
}

impl InviteTab {
    pub fn update(
        &mut self,
        message: InviteMessage,
        client: &Client,
        meta_data: &mut GuildMetadata,
        guild_id: u64,
        clip: &mut iced::Clipboard,
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
                        let uses: u32 = match uses.parse() {
                            Ok(val) => val,
                            Err(err) => return Err(ClientError::Custom(err.to_string())),
                        };
                        let request = CreateInviteRequest {
                            name,
                            possible_uses: uses,
                            guild_id,
                        };
                        let name = inner.call(request).await?.invite_id;
                        Ok(TopLevelMessage::guild_settings(ParentMessage::Invite(
                            InviteMessage::InviteCreated(name, uses),
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
            InviteMessage::InviteCreated(name, uses) => {
                let new_invite = InviteWithId {
                    invite: Some(Invite {
                        possible_uses: uses,
                        use_count: 0,
                    }),
                    invite_id: name,
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
                return TopLevelScreen::pop_screen_cmd();
            }
            InviteMessage::DeleteInvitePressed(n) => {
                let invite_id = meta_data.invites.as_ref().unwrap()[n].invite_id.clone();
                return client.mk_cmd(
                    |inner| async move {
                        inner
                            .chat()
                            .await
                            .delete_invite(DeleteInviteRequest { guild_id, invite_id })
                            .await?;
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
            InviteMessage::CopyToClipboard(string) => clip.write(string),
        }

        Command::none()
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_message = error.to_string();
        Command::none()
    }
}

impl Tab for InviteTab {
    type Message = InviteMessage;

    fn title(&self) -> String {
        String::from("Invites")
    }

    fn tab_label(&self) -> TabLabel {
        TabLabel::IconText(Icon::Heart.into(), self.title())
    }

    fn content(
        &mut self,
        client: &Client,
        guild_id: u64,
        meta_data: &mut GuildMetadata,
        theme: &Theme,
        _: &ThumbnailCache,
    ) -> Element<'_, InviteMessage> {
        let guild = client.guilds.get(&guild_id).unwrap();
        let mut widgets = Vec::with_capacity(10);
        if !self.error_message.is_empty() {
            widgets.push(
                label!(&self.error_message)
                    .color(theme.user_theme.error)
                    .size(DEF_SIZE + 2)
                    .into(),
            );
        }
        // If there are any invites, create invite list
        if guild.has_perm(INVITES_VIEW) {
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
                self.but_states.resize_with(invites.len(), Default::default);
                let mut invites_scrollable = Scrollable::new(&mut self.invite_list_state)
                    .style(theme)
                    .align_items(Align::Center);
                // Create each line of the invite list
                for (n, (cur_invite, (del_but_state, copy_url_state))) in
                    invites.iter().zip(self.but_states.iter_mut()).enumerate()
                {
                    if let Some(invite) = &cur_invite.invite {
                        url.set_path(&cur_invite.invite_id);
                        let mut row_widgets = vec![
                            Tooltip::new(
                                label_button!(copy_url_state, url.as_str())
                                    .on_press(InviteMessage::CopyToClipboard(url.to_string()))
                                    .style(theme)
                                    .width(length!(% 19)),
                                "Click to copy",
                                iced::tooltip::Position::Top,
                            )
                            .gap(PADDING / 3)
                            .style(theme)
                            .into(),
                            space!(w % 22).into(),
                            label!(invite.possible_uses.to_string()).width(length!(% 3)).into(),
                            space!(w % 3).into(),
                            label!(invite.use_count.to_string()).width(length!(% 3)).into(),
                            space!(w % 1).into(),
                        ];
                        if guild.has_perm(INVITES_MANAGE_DELETE) {
                            row_widgets.push(
                                Button::new(del_but_state, icon(Icon::Trash))
                                    .style(theme)
                                    .on_press(InviteMessage::DeleteInvitePressed(n))
                                    .into(),
                            );
                        }
                        invites_scrollable = invites_scrollable.push(Container::new(row(row_widgets)).style(theme));
                    }
                }
                widgets.push(invites_table.into());
                widgets.push(fill_container(invites_scrollable).style(theme).into());
            // If there aren't any invites
            } else {
                widgets.push(label!("Fetching invites").into());
            }
        } else {
            widgets.push(
                label!("You don't have permission to view invites")
                    .color(theme.user_theme.error)
                    .into(),
            );
        }
        widgets.push(space!(h = 20).into());
        // Invite Creation fields
        if guild.has_perm(INVITES_MANAGE_CREATE) {
            widgets.push(
                row(vec![
                    TextInput::new(
                        &mut self.invite_name_state,
                        "Enter invite name...",
                        self.invite_name_value.as_str(),
                        InviteMessage::InviteNameChanged,
                    )
                    .style(theme)
                    .padding(PADDING / 2)
                    .into(),
                    TextInput::new(
                        &mut self.invite_uses_state,
                        "Enter possible uses...",
                        self.invite_uses_value.as_str(),
                        InviteMessage::InviteUsesChanged,
                    )
                    .width(length!(= 200))
                    .padding(PADDING / 2)
                    .style(theme)
                    .into(),
                    Button::new(&mut self.create_invite_but_state, label!("Create"))
                        .style(theme)
                        .on_press(InviteMessage::CreateInvitePressed)
                        .into(),
                ])
                .into(),
            );
        }
        widgets.push(space!(h = 20).into());
        //Back button
        widgets.push(
            label_button!(&mut self.back_but_state, "Back")
                .style(theme)
                .on_press(InviteMessage::GoBack)
                .into(),
        );

        column(widgets).into()
    }
}
