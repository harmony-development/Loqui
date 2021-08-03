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
    style::Theme,
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

const POS_USES_WIDTH: u16 = 200;
const USES_WIDTH: u16 = 80;
const DEL_WIDTH: u16 = 30;

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

#[derive(Debug, Default)]
pub struct InviteTab {
    invite_name_state: text_input::State,
    invite_name_value: String,
    invite_uses_state: text_input::State,
    invite_uses_value: String,
    create_invite_but_state: button::State,
    invite_list_state: scrollable::State,
    back_but_state: button::State,
    delete_invite_but_states: Vec<button::State>,
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
        let mut widgets = vec![];
        // If there are any invites, create invite list
        if let Some(invites) = &meta_data.invites {
            // Create header for invite list
            let mut invites_column = vec![row(vec![
                label!("Invite Id").width(length!(+)).into(),
                label!("Possible uses").width(length!(= POS_USES_WIDTH)).into(),
                label!("Uses").width(length!(= USES_WIDTH)).into(),
                space!(w = DEL_WIDTH).into(),
            ])
            .into()];
            // Recreation of Url is neccessary because otherwise the scheme of the url can't be set to `harmony://`
            let homeserver_url = client.inner().homeserver_url();
            let mut url;
            if let Some(port) = homeserver_url.port() {
                url = Url::parse(format!("harmony://{}:{}/", homeserver_url.host().unwrap(), port).as_str()).unwrap();
            } else {
                url = Url::parse(format!("harmony://{}/", homeserver_url.host().unwrap()).as_str()).unwrap();
            }
            url.set_scheme("harmony").unwrap();
            self.delete_invite_but_states
                .resize_with(invites.len(), Default::default);
            // Create each line of the invite list
            for (n, (cur_invite, del_but_state)) in
                invites.iter().zip(self.delete_invite_but_states.iter_mut()).enumerate()
            {
                url.set_path(&cur_invite.invite_id);
                invites_column.push(
                    row(vec![
                        label!(url.as_str()).width(length!(+)).into(),
                        label!(cur_invite.possible_uses.to_string())
                            .width(length!(= POS_USES_WIDTH))
                            .into(),
                        label!(cur_invite.use_count.to_string())
                            .width(length!(= USES_WIDTH))
                            .into(),
                        Button::new(del_but_state, icon(Icon::Trash))
                            .width(length!(= DEL_WIDTH))
                            .style(theme)
                            .on_press(ParentMessage::Invite(InviteMessage::DeleteInvitePressed(n)))
                            .into(),
                    ])
                    .into(),
                );
            }
            widgets.push(column(invites_column).into());
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
                .into(),
                TextInput::new(
                    &mut self.invite_uses_state,
                    "Enter possible uses...",
                    self.invite_uses_value.as_str(),
                    |s| ParentMessage::Invite(InviteMessage::InviteUsesChanged(s)),
                )
                .width(length!(= 200))
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
