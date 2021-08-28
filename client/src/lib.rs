#![allow(clippy::field_reassign_with_default)]

pub mod channel;
pub mod content;
pub mod emotes;
pub mod error;
pub mod guild;
pub mod member;
pub mod message;
pub mod role;

use bool_ext::BoolExt;
use channel::Channel;
use emotes::EmotePacks;
use guild::Guild;
pub use harmony_rust_sdk::{
    self,
    api::exports::hrpc::url::Url,
    client::{api::auth::Session as InnerSession, AuthStatus, Client as InnerClient},
};
use harmony_rust_sdk::{
    api::{
        chat::{event::*, get_channel_messages_request::Direction, DeleteMessageRequest},
        harmonytypes::{Message as HarmonyMessage, UserStatus},
        mediaproxy::fetch_link_metadata_response::Data as FetchLinkData,
    },
    client::api::{
        chat::{
            message::{SendMessage, SendMessageSelfBuilder, UpdateMessageTextRequest},
            profile::{ProfileUpdate, ProfileUpdateSelfBuilder},
            EventSource,
        },
        rest::FileId,
    },
};

use content::ContentStore;
use error::{ClientError, ClientResult};
use member::{Member, Members};
use message::{Attachment, Content, Embed, MessageId};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::{
    fmt::{self, Debug, Display, Formatter},
    future::Future,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::task::JoinError;

use crate::{channel::ChanPerms, emotes::EmotePack};

use self::{
    guild::Guilds,
    message::{EmbedHeading, Message},
};

pub type IndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;
pub use ahash::AHashMap;
pub use bool_ext;
pub use linemd;
pub use smol_str;
pub use tracing;
pub use urlencoding;

/// A sesssion struct with our requirements (unlike the `InnerSession` type)
#[derive(Clone, Default, Deserialize, Serialize, Hash, PartialEq, Eq)]
pub struct Session {
    pub session_token: SmolStr,
    pub user_name: SmolStr,
    pub user_id: SmolStr,
    pub homeserver: SmolStr,
}

impl Debug for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user_id)
            .field("user_name", &self.user_name)
            .field("homeserver", &self.homeserver)
            .finish()
    }
}

impl Display for Session {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.user_name)?;
        if !self.homeserver.is_empty() {
            write!(f, " on {}", self.homeserver)?;
        }
        Ok(())
    }
}

impl From<Session> for InnerSession {
    fn from(session: Session) -> Self {
        InnerSession {
            user_id: session.user_id.parse().unwrap(),
            session_token: session.session_token.into(),
        }
    }
}

#[derive(Debug)]
pub enum PostProcessEvent {
    FetchProfile(u64),
    FetchGuildData(u64),
    FetchThumbnail(Attachment),
    GoToFirstMsgOnChannel(u64),
    CheckPermsForChannel(u64, u64),
    FetchMessage {
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
    },
    SendNotification {
        unread_message: bool,
        mention: bool,
        title: String,
        content: String,
    },
    FetchLinkMetadata(Url),
    FetchEmotes(u64),
}

#[derive(Clone)]
pub struct Client {
    inner: InnerClient,
    pub guilds: Guilds,
    pub members: Members,
    pub user_id: Option<u64>,
    pub link_datas: AHashMap<Url, FetchLinkData>,
    pub emote_packs: EmotePacks,
    content_store: Arc<ContentStore>,
}

impl Debug for Client {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Client")
            .field(
                "user_id",
                &format!("{:?}", self.auth_status().session().map_or(0, |s| s.user_id)),
            )
            .field("content_store", &self.content_store)
            .finish()
    }
}

impl Client {
    pub async fn new(
        homeserver_url: Url,
        session: Option<InnerSession>,
        content_store: Arc<ContentStore>,
    ) -> ClientResult<Self> {
        Ok(Self {
            guilds: Guilds::default(),
            members: Members::new(),
            user_id: session.as_ref().map(|s| s.user_id),
            content_store,
            inner: InnerClient::new(homeserver_url, session).await?,
            link_datas: AHashMap::new(),
            emote_packs: EmotePacks::default(),
        })
    }

    pub fn logout(
        &self,
        full_logout: bool,
    ) -> impl Future<Output = Result<ClientResult<()>, JoinError>> + Send + Sync + 'static {
        let inner = self.inner_arc();
        let content_store = self.content_store_arc();
        let user_id = self.user_id.unwrap();

        tokio::spawn(async move {
            let _ = inner
                .chat()
                .await
                .profile_update(ProfileUpdate::default().new_status(UserStatus::Offline))
                .await;
            Self::remove_session(user_id, inner.homeserver_url().as_str(), &content_store, full_logout).await
        })
    }

    pub async fn remove_session(
        user_id: u64,
        homeserver_url: &str,
        content_store: &Arc<ContentStore>,
        full_logout: bool,
    ) -> ClientResult<()> {
        if full_logout {
            tokio::fs::remove_file(content_store.session_path(homeserver_url, user_id)).await?;
        }
        tokio::fs::remove_file(content_store.latest_session_file()).await?;
        Ok(())
    }

    #[inline(always)]
    pub fn content_store(&self) -> &ContentStore {
        &self.content_store
    }

    #[inline(always)]
    pub fn content_store_arc(&self) -> Arc<ContentStore> {
        self.content_store.clone()
    }

    #[inline(always)]
    pub fn auth_status(&self) -> AuthStatus {
        self.inner.auth_status()
    }

    #[inline(always)]
    pub fn inner(&self) -> &InnerClient {
        &self.inner
    }

    #[inline(always)]
    pub fn inner_arc(&self) -> InnerClient {
        self.inner.clone()
    }

    #[inline(always)]
    pub fn get_guild(&mut self, guild_id: u64) -> Option<&mut Guild> {
        self.guilds.get_mut(&guild_id)
    }

    pub fn get_channel(&mut self, guild_id: u64, channel_id: u64) -> Option<&mut Channel> {
        self.get_guild(guild_id)
            .and_then(|guild| guild.channels.get_mut(&channel_id))
    }

    #[inline(always)]
    pub fn get_member(&mut self, user_id: u64) -> Option<&mut Member> {
        self.members.get_mut(&user_id)
    }

    pub fn get_emote_name(&self, image_id: &str) -> Option<&str> {
        self.emote_packs
            .values()
            .flat_map(|pack| pack.emotes.get(image_id))
            .map(String::as_str)
            .next()
    }

    pub fn get_all_emotes(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.emote_packs
            .values()
            .map(|p| p.emotes.iter().map(|(a, b)| (a.as_str(), b.as_str())))
            .flatten()
    }

    pub fn send_msg_cmd(
        &mut self,
        guild_id: u64,
        channel_id: u64,
        retry_after: Duration,
        message: Message,
    ) -> Option<impl Future<Output = (u64, u64, u64, Message, Duration, Option<u64>)>> {
        if let Some(channel) = self.get_channel(guild_id, channel_id) {
            if retry_after.as_secs() == 0 {
                channel.messages.insert(message.id, message.clone());
            }

            let inner = self.inner().clone();

            Some(async move {
                tokio::time::sleep(retry_after).await;

                let echo_id = message.id.transaction_id().unwrap();
                let msg = SendMessage::new(guild_id, channel_id)
                    .content(harmony_rust_sdk::api::harmonytypes::Content {
                        content: Some(message.content.clone().into()),
                    })
                    .in_reply_to(message.reply_to.unwrap_or(0))
                    .echo_id(echo_id)
                    .overrides(message.overrides.clone().map(Into::into));

                let send_result = inner.chat().await.send_message(msg).await;
                (
                    guild_id,
                    channel_id,
                    echo_id,
                    message,
                    send_result
                        .is_err()
                        .map_or(retry_after, || retry_after + Duration::from_secs(1)),
                    send_result
                        .map(|resp| resp.message_id)
                        .map_err(|err| tracing::error!("error occured when sending message: {}", err))
                        .ok(),
                )
            })
        } else {
            None
        }
    }

    pub fn delete_msg_cmd(
        &self,
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
    ) -> impl Future<Output = ClientResult<()>> {
        let inner = self.inner().clone();

        async move {
            inner
                .chat()
                .await
                .delete_message(DeleteMessageRequest {
                    guild_id,
                    channel_id,
                    message_id,
                })
                .await
                .map_err(Into::into)
        }
    }

    pub fn edit_msg_cmd(
        &self,
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
        new_content: String,
    ) -> impl Future<Output = (u64, u64, u64, Option<Box<ClientError>>)> {
        let inner = self.inner().clone();

        async move {
            let result = inner
                .chat()
                .await
                .update_message_text(UpdateMessageTextRequest {
                    guild_id,
                    channel_id,
                    message_id,
                    new_content,
                })
                .await;

            (
                guild_id,
                channel_id,
                message_id,
                result.err().map(|err| Box::new(err.into())),
            )
        }
    }

    pub fn process_event(&mut self, event: Event) -> Vec<PostProcessEvent> {
        let mut post = Vec::new();

        match event {
            Event::PermissionUpdated(perm) => {
                let PermissionUpdated {
                    guild_id,
                    channel_id,
                    query,
                    ok,
                } = perm;

                if let Some(g) = self.get_guild(guild_id) {
                    match query.as_str() {
                        "channel.manage.*" | "channel.*" => {
                            g.user_perms.create_channel = ok;
                            g.user_perms.delete_channel = ok;
                            g.user_perms.update_channel_order = ok;
                            g.channels
                                .get_mut(&channel_id)
                                .and_do(|c| c.user_perms.manage_channel = ok);
                        }
                        "channels.manage.move" => g.user_perms.update_channel_order = ok,
                        "channels.manage.create" => g.user_perms.create_channel = ok,
                        "channels.manage.delete" => g.user_perms.delete_channel = ok,
                        "channels.manage.change-information" => {
                            g.channels
                                .get_mut(&channel_id)
                                .and_do(|c| c.user_perms.manage_channel = ok);
                        }
                        "messages.send" | "messages.*" => {
                            g.channels.get_mut(&channel_id).and_do(|c| c.user_perms.send_msg = ok);
                        }
                        "guild.manage.change-information" | "guild.manage.*" | "guild.*" => {
                            g.user_perms.change_info = ok;
                        }
                        "user.*" | "user.manage.*" => {
                            g.user_perms.kick_user = ok;
                            g.user_perms.ban_user = ok;
                            g.user_perms.unban_user = ok;
                        }
                        "user.manage.ban" => g.user_perms.ban_user = ok,
                        "user.manage.kick" => g.user_perms.kick_user = ok,
                        "user.manage.unban" => g.user_perms.unban_user = ok,
                        "roles.*" => {
                            g.user_perms.manage_roles = ok;
                            g.user_perms.get_roles = ok;
                            g.user_perms.manage_user_roles = ok;
                            g.user_perms.get_user_roles = ok;
                        }
                        "roles.manage" => g.user_perms.manage_roles = ok,
                        "roles.get" => g.user_perms.get_roles = ok,
                        "roles.user.*" => {
                            g.user_perms.manage_user_roles = ok;
                            g.user_perms.get_user_roles = ok;
                        }
                        "roles.user.manage" => g.user_perms.manage_user_roles = ok,
                        "roles.user.get" => g.user_perms.get_user_roles = ok,
                        "invites.*" => {
                            g.user_perms.delete_invite = ok;
                            g.user_perms.create_invite = ok;
                            g.user_perms.view_invites = ok;
                        }
                        "invites.view" => g.user_perms.view_invites = ok,
                        "invites.manage.*" => {
                            g.user_perms.delete_invite = ok;
                            g.user_perms.create_invite = ok;
                        }
                        "invites.manage.delete" => g.user_perms.delete_invite = ok,
                        "invites.manage.create" => g.user_perms.create_invite = ok,
                        "permissions.*" | "permissions.manage.*" => {
                            g.user_perms.get_permission = ok;
                            g.user_perms.set_permission = ok;
                        }
                        "permissions.manage.set" => g.user_perms.set_permission = ok,
                        "permissions.manage.get" => g.user_perms.get_permission = ok,
                        _ => {}
                    }
                }
            }
            Event::SentMessage(message_sent) => {
                let echo_id = message_sent.echo_id;

                if let Some(message) = message_sent.message {
                    let guild_id = message.guild_id;
                    let channel_id = message.channel_id;
                    let message_id = message.message_id;

                    if let Some(guild) = self.guilds.get_mut(&guild_id) {
                        if let Some(channel) = guild.channels.get_mut(&channel_id) {
                            let message = Message::from(message);

                            message.post_process(&mut post, guild_id, channel_id);

                            if let Content::Text(text) = &message.content {
                                if !channel.looking_at_channel {
                                    use byte_writer::Writer;
                                    use std::fmt::Write;

                                    let current_user_id = self.user_id.unwrap_or(0);

                                    let mut pattern_arr = [b'0'; 23];
                                    write!(Writer(&mut pattern_arr), "<@{}>", current_user_id).unwrap();

                                    if text.contains(
                                        (unsafe { std::str::from_utf8_unchecked(&pattern_arr) })
                                            .trim_end_matches(|c| c != '>'),
                                    ) {
                                        let member_name = self
                                            .members
                                            .get(&message.sender)
                                            .map_or("unknown", |m| m.username.as_str());
                                        post.push(PostProcessEvent::SendNotification {
                                            unread_message: false,
                                            mention: true,
                                            title: format!("{} | #{}", guild.name, channel.name),
                                            content: format!(
                                                "@{}: {}",
                                                member_name,
                                                render_text(text, &self.members, &self.emote_packs)
                                            ),
                                        });
                                    }
                                }
                            }

                            if let Some(msg_index) = channel.messages.get_index_of(&MessageId::Unack(echo_id)) {
                                channel.messages.insert(MessageId::Ack(message_id), message);
                                channel
                                    .messages
                                    .swap_indices(msg_index, channel.messages.len().saturating_sub(1));
                                channel.messages.pop();
                            } else if let Some(msg) = channel.messages.get_mut(&MessageId::Ack(message_id)) {
                                *msg = message;
                            } else {
                                channel.messages.insert(message.id, message);
                            }

                            let disp = channel.messages.len();
                            if channel.looking_at_message >= disp.saturating_sub(32) {
                                channel.looking_at_message = disp.saturating_sub(1);
                                post.push(PostProcessEvent::GoToFirstMsgOnChannel(channel_id));
                            }
                            if !channel.looking_at_channel {
                                channel.has_unread = true;
                            }
                        }
                    }
                }
            }
            Event::DeletedMessage(MessageDeleted {
                guild_id,
                channel_id,
                message_id,
            }) => {
                if let Some(channel) = self.get_channel(guild_id, channel_id) {
                    channel.messages.remove(&MessageId::Ack(message_id));
                }
            }
            Event::EditedMessage(message_updated) => {
                let guild_id = message_updated.guild_id;
                let channel_id = message_updated.channel_id;

                if let Some(channel) = self.get_channel(guild_id, channel_id) {
                    if let Some(msg) = channel.messages.get_mut(&MessageId::Ack(message_updated.message_id)) {
                        msg.content = Content::Text(message_updated.new_content);
                        msg.post_process(&mut post, guild_id, channel_id);
                    }
                }
            }
            Event::DeletedChannel(ChannelDeleted { guild_id, channel_id }) => {
                if let Some(guild) = self.get_guild(guild_id) {
                    guild.channels.remove(&channel_id);
                }
            }
            Event::EditedChannel(ChannelUpdated {
                guild_id,
                channel_id,
                new_name,
                new_position,
                new_metadata: _,
            }) => {
                if let Some(guild) = self.get_guild(guild_id) {
                    if let Some(name) = new_name {
                        if let Some(channel) = guild.channels.get_mut(&channel_id) {
                            channel.name = name.into();
                        }
                    }

                    if let Some(position) = new_position {
                        guild.update_channel_order(position, channel_id);
                    }
                }
            }
            Event::CreatedChannel(ChannelCreated {
                guild_id,
                channel_id,
                name,
                position,
                is_category,
                metadata: _,
            }) => {
                if let Some(guild) = self.get_guild(guild_id) {
                    // [tag:channel_added_to_client]
                    guild.channels.insert(
                        channel_id,
                        Channel {
                            is_category,
                            name: name.into(),
                            loading_messages_history: false,
                            looking_at_message: 0,
                            messages: Default::default(),
                            last_known_message_id: 0,
                            reached_top: false,
                            has_unread: false,
                            looking_at_channel: false,
                            user_perms: ChanPerms {
                                manage_channel: false,
                                send_msg: false,
                            },
                            init_fetching: false,
                            role_perms: AHashMap::new(),
                            uploading_files: Vec::new(),
                        },
                    );
                    if let Some(position) = position {
                        guild.update_channel_order(position, channel_id);
                    }
                    post.push(PostProcessEvent::CheckPermsForChannel(guild_id, channel_id));
                }
            }
            Event::Typing(Typing {
                guild_id,
                channel_id,
                user_id,
            }) => {
                if let Some(member) = self.get_member(user_id) {
                    member.typing_in_channel = Some((guild_id, channel_id, Instant::now()));
                }
            }
            Event::JoinedMember(MemberJoined { guild_id, member_id }) => {
                if member_id == 0 {
                    return post;
                }

                if let Some(guild) = self.get_guild(guild_id) {
                    guild.members.entry(member_id).or_default();
                }

                if !self.members.contains_key(&member_id) {
                    post.push(PostProcessEvent::FetchProfile(member_id));
                }
            }
            Event::LeftMember(MemberLeft {
                guild_id,
                member_id,
                leave_reason: _,
            }) => {
                if let Some(guild) = self.get_guild(guild_id) {
                    guild.members.remove(&member_id);
                }
            }
            Event::ProfileUpdated(ProfileUpdated {
                user_id,
                new_username,
                new_avatar,
                new_status,
                new_is_bot,
            }) => {
                let member = self.members.entry(user_id).or_default();
                if let Some(new_username) = new_username {
                    member.username = new_username.into();
                }
                if let Some(new_status) = new_status {
                    member.status = UserStatus::from_i32(new_status).unwrap_or(UserStatus::Offline);
                }
                if let Some(new_avatar) = new_avatar {
                    let parsed = FileId::from_str(&new_avatar).ok();
                    member.avatar_url = parsed.clone();
                    if let Some(id) = parsed {
                        post.push(PostProcessEvent::FetchThumbnail(Attachment {
                            kind: "image".into(),
                            name: "avatar".into(),
                            ..Attachment::new_unknown(id)
                        }));
                    }
                }
                if let Some(is_bot) = new_is_bot {
                    member.is_bot = is_bot;
                }
            }
            Event::GuildAddedToList(GuildAddedToList { guild_id, homeserver }) => {
                self.guilds.insert(
                    guild_id,
                    Guild {
                        homeserver,
                        ..Default::default()
                    },
                );
                post.push(PostProcessEvent::FetchGuildData(guild_id));
            }
            Event::GuildRemovedFromList(GuildRemovedFromList {
                guild_id,
                homeserver: _,
            }) => {
                self.guilds.remove(&guild_id);
            }
            Event::DeletedGuild(GuildDeleted { guild_id }) => {
                self.guilds.remove(&guild_id);
            }
            Event::EditedGuild(GuildUpdated {
                guild_id,
                new_name,
                new_picture,
                new_metadata: _,
            }) => {
                let guild = self.guilds.entry(guild_id).or_default();

                if let Some(name) = new_name {
                    guild.name = name;
                }
                if let Some(picture) = new_picture {
                    let parsed = FileId::from_str(&picture).ok();
                    guild.picture = parsed.clone();
                    if let Some(id) = parsed {
                        post.push(PostProcessEvent::FetchThumbnail(Attachment {
                            kind: "image".into(),
                            name: "guild".into(),
                            ..Attachment::new_unknown(id)
                        }));
                    }
                }
            }
            Event::RoleCreated(RoleCreated {
                guild_id,
                role_id,
                role,
            }) => {
                self.get_guild(guild_id).and_do(|g| {
                    role.and_do(|role| {
                        g.roles.insert(role_id, role.into());
                    });
                });
            }
            Event::RoleDeleted(RoleDeleted { guild_id, role_id }) => {
                self.get_guild(guild_id).and_do(|g| {
                    g.roles.remove(&role_id);
                });
            }
            Event::RoleUpdated(RoleUpdated {
                guild_id,
                role_id,
                new_role,
            }) => {
                self.get_guild(guild_id).and_do(|g| {
                    new_role.and_do(|role| {
                        g.roles.insert(role_id, role.into());
                    });
                });
            }
            Event::RoleMoved(RoleMoved {
                guild_id,
                role_id,
                new_position,
            }) => {
                if let Some(position) = new_position {
                    self.get_guild(guild_id).and_do(|g| {
                        g.update_role_order(position, role_id);
                    });
                }
            }
            Event::UserRolesUpdated(UserRolesUpdated {
                guild_id,
                user_id,
                new_role_ids,
            }) => {
                self.get_guild(guild_id).and_do(|g| {
                    g.members.insert(user_id, new_role_ids);
                });
            }
            Event::RolePermsUpdated(RolePermissionsUpdated {
                guild_id,
                channel_id,
                role_id,
                new_perms,
            }) => {
                if let Some(perms) = new_perms {
                    self.get_guild(guild_id).and_do(|g| {
                        if channel_id != 0 {
                            g.channels.get_mut(&channel_id).and_do(|c| {
                                c.role_perms.insert(role_id, perms.permissions);
                            });
                        } else {
                            g.role_perms.insert(role_id, perms.permissions);
                        }
                    });
                }
            }
            Event::EmotePackUpdated(EmotePackUpdated { pack_id, new_pack_name }) => {
                if let Some(pack) = self.emote_packs.get_mut(&pack_id) {
                    if let Some(pack_name) = new_pack_name {
                        pack.pack_name = pack_name;
                    }
                }
            }
            Event::EmotePackEmotesUpdated(EmotePackEmotesUpdated {
                pack_id,
                added_emotes,
                deleted_emotes,
            }) => {
                if let Some(pack) = self.emote_packs.get_mut(&pack_id) {
                    post.extend(added_emotes.iter().map(|emote| {
                        PostProcessEvent::FetchThumbnail(Attachment {
                            kind: "image".to_string(),
                            name: "emote".to_string(),
                            ..Attachment::new_unknown(FileId::Id(emote.image_id.clone()))
                        })
                    }));
                    pack.emotes
                        .extend(added_emotes.into_iter().map(|emote| (emote.image_id, emote.name)));
                    for image_id in deleted_emotes {
                        pack.emotes.remove(&image_id);
                    }
                }
            }
            Event::EmotePackDeleted(EmotePackDeleted { pack_id }) => {
                self.emote_packs.remove(&pack_id);
            }
            Event::EmotePackAdded(EmotePackAdded { pack }) => {
                if let Some(pack) = pack {
                    post.push(PostProcessEvent::FetchEmotes(pack.pack_id));
                    self.emote_packs.insert(
                        pack.pack_id,
                        EmotePack {
                            pack_name: pack.pack_name,
                            pack_owner: pack.pack_owner,
                            emotes: Default::default(),
                        },
                    );
                }
            }
            x => tracing::warn!("implement {:?}", x),
        }

        post
    }

    pub fn process_reply_message(
        &mut self,
        guild_id: u64,
        channel_id: u64,
        message: HarmonyMessage,
    ) -> Vec<PostProcessEvent> {
        let mut post = Vec::new();

        if let Some(channel) = self.get_channel(guild_id, channel_id) {
            let message: Message = message.into();
            message.post_process(&mut post, guild_id, channel_id);
            if channel.messages.contains_key(&message.id) {
                channel.messages.insert(message.id, message);
            } else {
                channel.messages.reverse();
                channel.messages.insert(message.id, message);
                channel.messages.reverse();
            }
        }

        post
    }

    pub fn process_get_message_history_response(
        &mut self,
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
        messages: Vec<HarmonyMessage>,
        reached_top: bool,
        direction: Direction,
    ) -> Vec<PostProcessEvent> {
        let mut post = Vec::new();

        if let Some(channel) = self.get_channel(guild_id, channel_id) {
            let mut messages: IndexMap<_, _> = messages
                .into_iter()
                .map(|msg| (MessageId::Ack(msg.message_id), Message::from(msg)))
                .collect();

            messages.values().for_each(|m| {
                m.post_process(&mut post, guild_id, channel_id);
            });

            let msg_pos = channel.messages.get_index_of(&MessageId::Ack(message_id));
            let process_before = |mut pos: usize,
                                  messages: IndexMap<MessageId, Message>,
                                  chan_messages: &mut Vec<(MessageId, Message)>| {
                for message in messages {
                    if chan_messages.get(pos).map_or(true, |(id, _)| message.0.eq(id)) {
                        continue;
                    }
                    if let Some(at) = chan_messages.iter().position(|(id, _)| message.0.eq(id)) {
                        if at < pos {
                            pos -= 1;
                        }
                        chan_messages.remove(at);
                    }
                    chan_messages.insert(pos, message);
                }
            };
            let process_after = |mut pos: usize,
                                 messages: IndexMap<MessageId, Message>,
                                 chan_messages: &mut Vec<(MessageId, Message)>| {
                pos += 1;
                for message in messages {
                    if chan_messages.get(pos).map_or(true, |(id, _)| message.0.eq(id)) {
                        pos += 1;
                        continue;
                    }
                    if let Some(at) = chan_messages.iter().position(|(id, _)| message.0.eq(id)) {
                        if at < pos {
                            pos -= 1;
                        }
                        chan_messages.remove(at);
                    }
                    chan_messages.insert(pos, message);
                    pos += 1;
                }
            };

            match direction {
                Direction::Before => {
                    let last_message_id = messages.keys().last().copied();
                    match msg_pos.or_else(|| {
                        channel
                            .messages
                            .get_index_of(&MessageId::Ack(channel.last_known_message_id))
                    }) {
                        Some(pos) => {
                            let mut chan_messages = channel.messages.drain(..).collect::<Vec<_>>();
                            process_before(pos, messages, &mut chan_messages);
                            channel.messages = chan_messages.into_iter().collect();
                        }
                        None => {
                            messages.reverse();
                            channel.messages.extend(messages);
                        }
                    }
                    if let Some(MessageId::Ack(id)) = last_message_id {
                        channel.last_known_message_id = id;
                    }
                }
                Direction::After => match msg_pos {
                    Some(pos) => {
                        let mut chan_messages = channel.messages.drain(..).collect::<Vec<_>>();
                        process_after(pos, messages, &mut chan_messages);
                        channel.messages = chan_messages.into_iter().collect();
                    }
                    None => {
                        channel.messages.extend(messages);
                    }
                },
                Direction::Around => match msg_pos {
                    Some(pos) => {
                        let mut chan_messages = channel.messages.drain(..).collect::<Vec<_>>();
                        let message_pos = messages.get_index_of(&MessageId::Ack(message_id)).unwrap();
                        process_after(pos, messages.drain(message_pos + 1..).collect(), &mut chan_messages);
                        process_before(pos, messages.drain(..message_pos).collect(), &mut chan_messages);
                        channel.messages = chan_messages.into_iter().collect();
                    }
                    None => {
                        messages.extend(channel.messages.drain(..));
                        channel.messages = messages;
                    }
                },
            }
            channel.reached_top = reached_top;
        }

        post
    }

    pub fn subscribe_to(&self) -> Vec<EventSource> {
        let mut subs = self
            .guilds
            .keys()
            .map(|guild_id| EventSource::Guild(*guild_id))
            .collect::<Vec<_>>();
        subs.push(EventSource::Homeserver);
        subs
    }
}

pub fn render_text(textt: &str, members: &Members, emote_packs: &EmotePacks) -> String {
    // TODO: this is horribly inefficient
    let mut text = textt.to_string();
    for tok in textt.parse_md_custom(HarmonyToken::parse) {
        if let Token::Custom(HarmonyToken::Mention(id)) = tok {
            let member_name = members.get(&id).map_or_else(|| "unknown user", |m| m.username.as_str());
            text = text.replace(&format!("<@{}>", id), &format!("@{}", member_name));
        } else if let Token::Custom(HarmonyToken::Emote(image_id)) = tok {
            if let Some(name) = emote_packs
                .values()
                .flat_map(|pack| pack.emotes.get(image_id))
                .map(String::as_str)
                .next()
            {
                text = text.replace(&format!("<:{}:>", image_id), &format!(":{}:", name));
            }
        }
    }
    text
}

fn post_heading(post: &mut Vec<PostProcessEvent>, embed: &Embed) {
    let mut inner = |h: Option<&EmbedHeading>| {
        if let Some(id) = h.and_then(|h| h.icon.clone()) {
            post.push(PostProcessEvent::FetchThumbnail(Attachment {
                kind: "image".into(),
                ..Attachment::new_unknown(id)
            }));
        }
    };
    inner(embed.header.as_ref());
    inner(embed.footer.as_ref());
}

pub trait ResultExt<T, E> {
    fn ok_do<F: FnOnce(T)>(self, f: F);
    fn err_do<F: FnOnce(E)>(self, f: F);
}

pub trait OptionExt<T> {
    fn and_do<F: FnOnce(T)>(self, f: F) -> Self;
    fn or_do<F: FnOnce()>(self, f: F) -> Self;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    #[inline(always)]
    fn ok_do<F: FnOnce(T)>(self, f: F) {
        if let Ok(val) = self {
            f(val);
        }
    }

    #[inline(always)]
    fn err_do<F: FnOnce(E)>(self, f: F) {
        if let Err(val) = self {
            f(val);
        }
    }
}

impl<T> OptionExt<T> for Option<T> {
    #[inline(always)]
    fn and_do<F: FnOnce(T)>(self, f: F) -> Self {
        if let Some(val) = self {
            f(val);
            None
        } else {
            None
        }
    }

    #[inline(always)]
    fn or_do<F: FnOnce()>(self, f: F) -> Self {
        self.is_none().and_do(f);
        None
    }
}

pub mod byte_writer {
    use core::{
        fmt::{Error, Write},
        mem,
    };

    pub struct Writer<'a>(pub &'a mut [u8]);

    impl Write for Writer<'_> {
        fn write_str(&mut self, data: &str) -> Result<(), Error> {
            if data.len() > self.0.len() {
                return Err(Error);
            }

            let (a, b) = mem::replace(&mut self.0, &mut []).split_at_mut(data.len());
            a.copy_from_slice(data.as_bytes());
            self.0 = b;

            Ok(())
        }
    }
}

use linemd::{
    parser::{AtToken, Token},
    Parser,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HarmonyToken<'a> {
    Emote(&'a str),
    Mention(u64),
}

impl<'a> HarmonyToken<'a> {
    pub fn parse(value: &'a &str, at: usize) -> Option<AtToken<'a, HarmonyToken<'a>>> {
        if let Some(nat) = value.consume_char_if(at, |c| c == '<') {
            if let Some(nat) = value.consume_char_if(nat, |c| c == '@') {
                value
                    .consume_while(nat, |c| c != '>')
                    .ok()
                    .flatten()
                    .map(|(maybe_id, nat)| {
                        maybe_id
                            .parse::<u64>()
                            .ok()
                            .map(|id| (Token::Custom(HarmonyToken::Mention(id)), nat + 1))
                    })
                    .flatten()
            } else if let Some(nat) = value.consume_char_if(nat, |c| c == ':') {
                if let Some((id, nat)) = value.consume_until_str(nat, ":>").ok().flatten() {
                    Some((Token::Custom(HarmonyToken::Emote(id)), nat + 2))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

pub mod color {
    pub fn encode_rgb(color: (u8, u8, u8)) -> i64 {
        let mut c = color.0 as i64;
        c = (c << 8) + color.1 as i64;
        c = (c << 8) + color.2 as i64;
        c as i64
    }

    pub fn decode_rgb(color: i64) -> (u8, u8, u8) {
        (
            ((color >> 16) & 255) as u8,
            ((color >> 8) & 255) as u8,
            (color & 255) as u8,
        )
    }
}
