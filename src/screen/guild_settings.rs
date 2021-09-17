mod channel_ordering;
mod create_channel;
mod create_edit_role;
mod edit_channel;
mod general;
mod invite;
mod manage_role_permissions;
mod manage_user_roles;
mod members;
mod roles;

use std::ops::Not;

use crate::{
    client::{error::ClientError, Client},
    component::*,
    screen::{
        guild_settings::{
            channel_ordering::{OrderingMessage, OrderingTab},
            general::{GeneralMessage, GeneralTab},
            invite::{InviteMessage, InviteTab},
        },
        Message as TopLevelMessage,
    },
    style::*,
};
use client::harmony_rust_sdk::{
    api::chat::{
        self,
        all_permissions::{INVITES_VIEW, PERMISSIONS_MANAGE_GET},
        stream_event::RolePermissionsUpdated,
        Event, InviteWithId,
    },
    client::api::chat::{invite::GetGuildInvitesRequest, permissions::GetPermissions},
};
use iced::Element;
use iced_aw::{modal, Modal, TabLabel, Tabs, ICON_FONT};

use self::{
    create_channel::ChannelCreationModal,
    create_edit_role::RoleModal,
    edit_channel::UpdateChannelModal,
    manage_role_permissions::ManageRolePermissionsModal,
    manage_user_roles::ManageUserRolesModal,
    members::{MembersMessage, MembersTab},
    roles::{RolesMessage, RolesTab},
};

use super::{sub_escape_pop_screen, ClientExt};

const TAB_PADDING: u16 = 16;

#[derive(Debug, Clone, Default)]
pub struct GuildMetadata {
    invites: Option<Vec<InviteWithId>>,
}

#[derive(Debug, Clone, Default)]
pub struct GuildSettings {
    guild_id: u64,
    active_tab: usize,
    general_tab: GeneralTab,
    invite_tab: InviteTab,
    ordering_tab: OrderingTab,
    roles_tab: RolesTab,
    members_tab: MembersTab,
    current_error: String,
    meta_data: GuildMetadata,
    back_button: button::State,
    update_channel_modal: modal::State<UpdateChannelModal>,
    create_channel_modal: modal::State<ChannelCreationModal>,
    role_modal: modal::State<RoleModal>,
    manage_user_roles_modal: modal::State<ManageUserRolesModal>,
    manage_role_permissions_modal: modal::State<ManageRolePermissionsModal>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(usize),
    General(GeneralMessage),
    Invite(InviteMessage),
    Ordering(OrderingMessage),
    Roles(RolesMessage),
    Members(MembersMessage),
    UpdateChannelMessage(edit_channel::Message),
    ChannelCreationMessage(create_channel::Message),
    RoleMessage(create_edit_role::Message),
    ManageUserRolesMessage(manage_user_roles::Message),
    ManageRolePermissionsMessage(manage_role_permissions::Message),
    /// Sent when the permission check for channel edits are complete.
    ShowUpdateChannelModal(u64),
    /// Sent when the user triggers an ID copy (guild ID, message ID etc.)
    CopyIdToClipboard(u64),
    CopyToClipboard(String),
    NewChannel,
    ShowEditRoleModal(u64),
    ShowManageUserRoles(u64),
    NewRole,
    ShowManagePermsModal(u64, Option<u64>),
}

impl GuildSettings {
    pub fn new(guild_id: u64) -> Self {
        GuildSettings {
            guild_id,
            ..Default::default()
        }
    }

    pub fn update(
        &mut self,
        message: Message,
        client: &Client,
        clip: &mut iced::Clipboard,
    ) -> Command<TopLevelMessage> {
        match message {
            Message::ShowUpdateChannelModal(channel_id) => {
                self.update_channel_modal.show(true);
                self.current_error.clear();
                let modal_state = self.update_channel_modal.inner_mut();
                let chan = client
                    .guilds
                    .get(&self.guild_id)
                    .and_then(|g| g.channels.get(&channel_id))
                    .expect("channel not found in client?"); // should never panic, if it does it means client data is corrupted
                modal_state.channel_name_field.clear();
                modal_state.channel_name_field.push_str(&chan.name);
                modal_state.guild_id = self.guild_id;
                modal_state.channel_id = channel_id;
            }
            Message::CopyIdToClipboard(id) => clip.write(id.to_string()),
            Message::CopyToClipboard(msg) => clip.write(msg),
            Message::TabSelected(selected) => {
                self.active_tab = selected;
                match selected {
                    0 => {
                        self.general_tab.error_message.clear();
                    }
                    1 => {
                        self.invite_tab.error_message.clear();
                        // Invite tab
                        // Is Triggered when the invite tab is clicked
                        // Triggers the fetch of the invites, receiving is handled in invite.rs
                        if client
                            .guilds
                            .get(&self.guild_id)
                            .map_or(false, |g| g.has_perm(INVITES_VIEW))
                        {
                            let guild_id = self.guild_id;
                            return client.mk_cmd(
                                |inner| async move {
                                    inner
                                        .call(GetGuildInvitesRequest::new(guild_id))
                                        .await
                                        .map(|resp| resp.invites)
                                },
                                |invites| {
                                    TopLevelMessage::guild_settings(Message::Invite(InviteMessage::InvitesLoaded(
                                        invites,
                                    )))
                                },
                            );
                        }
                    }
                    2 => {
                        self.ordering_tab.error_message.clear();
                    }
                    3 => {
                        self.roles_tab.error_message.clear();
                    }
                    4 => {
                        self.members_tab.error_message.clear();
                    }
                    _ => {}
                };
            }
            Message::General(message) => {
                return self
                    .general_tab
                    .update(message, client, &mut self.meta_data, self.guild_id)
            }
            Message::Invite(message) => {
                return self
                    .invite_tab
                    .update(message, client, &mut self.meta_data, self.guild_id, clip)
            }
            Message::Ordering(message) => {
                return self
                    .ordering_tab
                    .update(message, client, &mut self.meta_data, self.guild_id)
            }
            Message::Roles(message) => {
                return self
                    .roles_tab
                    .update(message, client, &mut self.meta_data, self.guild_id)
            }
            Message::Members(message) => {
                return self
                    .members_tab
                    .update(message, client, &mut self.meta_data, self.guild_id)
            }
            Message::UpdateChannelMessage(msg) => {
                let (cmd, go_back) = self.update_channel_modal.inner_mut().update(msg, client);
                self.update_channel_modal.show(!go_back);
                return cmd;
            }
            Message::ChannelCreationMessage(msg) => {
                let (cmd, go_back) = self.create_channel_modal.inner_mut().update(msg, client);
                self.create_channel_modal.show(!go_back);
                return cmd;
            }
            Message::RoleMessage(msg) => {
                let (cmd, go_back) = self.role_modal.inner_mut().update(msg, client);
                self.role_modal.show(!go_back);
                return cmd;
            }
            Message::NewChannel => {
                self.create_channel_modal.inner_mut().guild_id = self.guild_id;
                self.create_channel_modal.show(true);
                self.current_error.clear();
            }
            Message::ShowEditRoleModal(role_id) => {
                let mut modal_state = self.role_modal.inner_mut();
                modal_state.guild_id = self.guild_id;
                modal_state.editing = Some(role_id);
                let role = client
                    .guilds
                    .get(&self.guild_id)
                    .and_then(|g| g.roles.get(&role_id))
                    .cloned()
                    .unwrap_or_default();
                modal_state.is_hoist = role.hoist;
                modal_state.is_pingable = role.pingable;
                modal_state.role_name_field = role.name.to_string();
                self.role_modal.show(true);
                self.current_error.clear();
            }
            Message::ShowManageUserRoles(user_id) => {
                let mut modal_state = self.manage_user_roles_modal.inner_mut();
                modal_state.user_id = user_id;
                self.manage_user_roles_modal.show(true);
                self.current_error.clear();
            }
            Message::ShowManagePermsModal(role_id, channel_id) => {
                let mut modal_state = self.manage_role_permissions_modal.inner_mut();
                modal_state.role_id = role_id;
                modal_state.channel_id = channel_id;
                self.manage_role_permissions_modal.show(true);
                self.current_error.clear();
                if let Some(guild) = client.guilds.get(&self.guild_id) {
                    if guild.has_perm(PERMISSIONS_MANAGE_GET) {
                        let guild_id = self.guild_id;
                        let mut cmds = Vec::with_capacity(guild.channels.len() + 1);
                        let mk_cmd = |channel_id| {
                            client.mk_cmd(
                                |inner| async move {
                                    let mut request = GetPermissions::new(guild_id, role_id);
                                    if let Some(channel_id) = channel_id {
                                        request = request.with_channel_id(channel_id);
                                    }
                                    inner.call(request).await.map(|p| {
                                        vec![Event::Chat(chat::stream_event::Event::RolePermsUpdated(
                                            RolePermissionsUpdated {
                                                guild_id,
                                                channel_id,
                                                new_perms: p.perms,
                                                role_id,
                                            },
                                        ))]
                                    })
                                },
                                TopLevelMessage::EventsReceived,
                            )
                        };

                        if guild.role_perms.iter().filter(|(id, _)| role_id.eq(*id)).count() == 0 {
                            cmds.push(mk_cmd(None));
                        }

                        for (channel_id, channel) in &guild.channels {
                            if channel.role_perms.iter().filter(|(id, _)| role_id.eq(*id)).count() == 0 {
                                cmds.push(mk_cmd(Some(*channel_id)));
                            }
                        }

                        return Command::batch(cmds);
                    }
                }
            }
            Message::NewRole => {
                let mut modal_state = self.role_modal.inner_mut();
                modal_state.guild_id = self.guild_id;
                modal_state.editing = None;
                modal_state.is_hoist = false;
                modal_state.is_pingable = false;
                modal_state.role_name_field.clear();
                self.role_modal.show(true);
                self.current_error.clear();
            }
            Message::ManageUserRolesMessage(message) => {
                let (cmd, go_back) = self
                    .manage_user_roles_modal
                    .inner_mut()
                    .update(message, client, self.guild_id);
                self.manage_user_roles_modal.show(!go_back);
                return cmd;
            }
            Message::ManageRolePermissionsMessage(message) => {
                let (cmd, go_back) =
                    self.manage_role_permissions_modal
                        .inner_mut()
                        .update(message, client, self.guild_id);
                self.manage_role_permissions_modal.show(!go_back);
                return cmd;
            }
        }

        Command::none()
    }

    pub fn view<'a>(
        &'a mut self,
        theme: &'a Theme,
        client: &'a Client,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<'_, Message> {
        let position = iced_aw::TabBarPosition::Top;
        let content = Tabs::new(self.active_tab, Message::TabSelected)
            .push(
                self.general_tab.tab_label(),
                self.general_tab
                    .view(client, self.guild_id, &mut self.meta_data, theme, thumbnail_cache),
            )
            .push(
                self.invite_tab.tab_label(),
                self.invite_tab
                    .view(client, self.guild_id, &mut self.meta_data, theme, thumbnail_cache)
                    .map(Message::Invite),
            )
            .push(
                self.ordering_tab.tab_label(),
                self.ordering_tab
                    .view(client, self.guild_id, &mut self.meta_data, theme, thumbnail_cache),
            )
            .push(
                self.roles_tab.tab_label(),
                self.roles_tab
                    .view(client, self.guild_id, &mut self.meta_data, theme, thumbnail_cache),
            )
            .push(
                self.members_tab.tab_label(),
                self.members_tab
                    .view(client, self.guild_id, &mut self.meta_data, theme, thumbnail_cache),
            )
            .tab_bar_style(theme)
            .icon_font(ICON_FONT)
            .tab_bar_position(position);

        // Show CreateChannelModal
        let content = Modal::new(&mut self.create_channel_modal, content, move |state| {
            state.view(theme).map(Message::ChannelCreationMessage)
        })
        .style(theme)
        .backdrop(Message::ChannelCreationMessage(create_channel::Message::GoBack))
        .on_esc(Message::ChannelCreationMessage(create_channel::Message::GoBack));

        // Show UpdateChannelModal
        let content = Modal::new(&mut self.update_channel_modal, content, move |state| {
            state.view(theme).map(Message::UpdateChannelMessage)
        })
        .style(theme)
        .backdrop(Message::UpdateChannelMessage(edit_channel::Message::GoBack))
        .on_esc(Message::UpdateChannelMessage(edit_channel::Message::GoBack));

        // Show RoleModal
        let content = Modal::new(&mut self.role_modal, content, move |state| {
            state.view(theme).map(Message::RoleMessage)
        })
        .style(theme)
        .backdrop(Message::RoleMessage(create_edit_role::Message::GoBack))
        .on_esc(Message::RoleMessage(create_edit_role::Message::GoBack));

        // Show RoleModal
        let guild_id = self.guild_id;
        let content = Modal::new(&mut self.manage_user_roles_modal, content, move |state| {
            state.view(theme, client, guild_id).map(Message::ManageUserRolesMessage)
        })
        .style(theme)
        .backdrop(Message::ManageUserRolesMessage(manage_user_roles::Message::GoBack))
        .on_esc(Message::ManageUserRolesMessage(manage_user_roles::Message::GoBack));

        // Show RoleModal
        let guild_id = self.guild_id;
        let content = Modal::new(&mut self.manage_role_permissions_modal, content, move |state| {
            state
                .view(theme, client, guild_id)
                .map(Message::ManageRolePermissionsMessage)
        })
        .style(theme)
        .backdrop(Message::ManageRolePermissionsMessage(
            manage_role_permissions::Message::GoBack,
        ))
        .on_esc(Message::ManageRolePermissionsMessage(
            manage_role_permissions::Message::GoBack,
        ));

        content.into()
    }

    pub fn subscription(&self) -> Subscription<TopLevelMessage> {
        (self.role_modal.is_shown()
            || self.create_channel_modal.is_shown()
            || self.update_channel_modal.is_shown()
            || self.manage_user_roles_modal.is_shown()
            || self.manage_role_permissions_modal.is_shown())
        .not()
        .then(sub_escape_pop_screen)
        .unwrap_or_else(Subscription::none)
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        if self.update_channel_modal.is_shown() {
            return self.update_channel_modal.inner_mut().on_error(&error);
        }
        if self.create_channel_modal.is_shown() {
            return self.create_channel_modal.inner_mut().on_error(&error);
        }
        if self.role_modal.is_shown() {
            return self.role_modal.inner_mut().on_error(&error);
        }
        match self.active_tab {
            0 => self.general_tab.on_error(error),
            1 => self.invite_tab.on_error(error),
            2 => self.ordering_tab.on_error(error),
            3 => self.roles_tab.on_error(error),
            4 => self.members_tab.on_error(error),
            _ => Command::none(),
        }
    }
}

trait Tab {
    type Message;

    fn title(&self) -> String;

    fn tab_label(&self) -> TabLabel;

    fn view(
        &mut self,
        client: &Client,
        guild_id: u64,
        meta_data: &mut GuildMetadata,
        theme: &Theme,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<'_, Self::Message> {
        fill_container(self.content(client, guild_id, meta_data, theme, thumbnail_cache))
            .padding(TAB_PADDING)
            .style(theme)
            .into()
    }

    fn content(
        &mut self,
        client: &Client,
        guild_id: u64,
        meta_data: &mut GuildMetadata,
        theme: &Theme,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<'_, Self::Message>;
}
