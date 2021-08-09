#![allow(clippy::field_reassign_with_default)]

pub mod channel;
pub mod content;
pub mod error;
pub mod guild;
pub mod member;
pub mod message;
pub mod role;

use bool_ext::BoolExt;
use channel::Channel;
use guild::Guild;
pub use harmony_rust_sdk::{
    self,
    api::exports::hrpc::url::Url,
    client::{api::auth::Session as InnerSession, AuthStatus, Client as InnerClient},
};
use harmony_rust_sdk::{
    api::{
        chat::{event::*, DeleteMessageRequest},
        harmonytypes::{Message as HarmonyMessage, UserStatus},
    },
    client::api::{
        chat::{
            message::{
                delete_message, send_message, update_message_text, SendMessage, SendMessageSelfBuilder,
                UpdateMessageTextRequest,
            },
            profile::{self, ProfileUpdate},
            EventSource,
        },
        rest::FileId,
    },
};

use content::ContentStore;
use error::{ClientError, ClientResult};
use member::{Member, Members};
use message::{harmony_messages_to_ui_messages, Attachment, Content, Embed, MessageId};
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

use crate::channel::ChanPerms;

use self::{
    guild::Guilds,
    message::{EmbedHeading, Message},
};

pub type IndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;
pub use ahash::AHashMap;
pub use bool_ext;
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
}

#[derive(Clone)]
pub struct Client {
    inner: InnerClient,
    pub guilds: Guilds,
    pub members: Members,
    pub user_id: Option<u64>,
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
            let _ = profile::profile_update(&inner, ProfileUpdate::default().new_status(UserStatus::Offline)).await;
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

                let send_result = send_message(&inner, msg).await;
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
            delete_message(
                &inner,
                DeleteMessageRequest {
                    guild_id,
                    channel_id,
                    message_id,
                },
            )
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
            let result = update_message_text(
                &inner,
                UpdateMessageTextRequest {
                    guild_id,
                    channel_id,
                    message_id,
                    new_content,
                },
            )
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

                match query.as_str() {
                    "channels.manage.change-information" | "channel.manage.*" => {
                        self.get_channel(guild_id, channel_id)
                            .and_do(|c| c.user_perms.manage_channel = ok);
                    }
                    "messages.send" | "messages.*" => {
                        self.get_channel(guild_id, channel_id)
                            .and_do(|c| c.user_perms.send_msg = ok);
                    }
                    "guild.manage.change-information" | "guild.manage.*" => {
                        self.get_guild(guild_id).and_do(|g| g.user_perms.change_info = ok);
                    }
                    _ => {}
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

                            if let Some(id) = message
                                .overrides
                                .as_ref()
                                .and_then(|overrides| overrides.avatar_url.clone())
                            {
                                post.push(PostProcessEvent::FetchThumbnail(Attachment {
                                    kind: "image".into(),
                                    name: "avatar".into(),
                                    ..Attachment::new_unknown(id)
                                }));
                            }

                            /*if let Some(message_id) = message.reply_to {
                                post.push(PostProcessEvent::FetchMessage {
                                    guild_id,
                                    channel_id,
                                    message_id,
                                });
                            }*/

                            message.post_process(&mut post);

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
                                            content: format!("@{}: {}", member_name, render_text(text, &self.members)),
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
                        msg.content = Content::Text(message_updated.content);
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
                name,
                update_name,
                previous_id,
                next_id,
                update_order,
                metadata: _,
                update_metadata: _,
            }) => {
                if let Some(guild) = self.get_guild(guild_id) {
                    if update_name {
                        if let Some(channel) = guild.channels.get_mut(&channel_id) {
                            channel.name = name.into();
                        }
                    }

                    if update_order {
                        guild.update_channel_order(previous_id, next_id, channel_id);
                    }
                }
            }
            Event::CreatedChannel(ChannelCreated {
                guild_id,
                channel_id,
                name,
                previous_id,
                next_id,
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
                            reached_top: false,
                            has_unread: false,
                            looking_at_channel: false,
                            user_perms: ChanPerms {
                                manage_channel: false,
                                send_msg: false,
                            },
                            init_fetching: false,
                        },
                    );
                    if previous_id != 0 || next_id != 0 {
                        guild.update_channel_order(previous_id, next_id, channel_id);
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
                update_username,
                new_avatar,
                update_avatar,
                new_status,
                update_status,
                is_bot,
                update_is_bot,
            }) => {
                let member = self.members.entry(user_id).or_default();
                if update_username {
                    member.username = new_username.into();
                }
                if update_status {
                    member.status = UserStatus::from_i32(new_status).unwrap();
                }
                if update_avatar {
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
                if update_is_bot {
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
                name,
                update_name,
                picture,
                update_picture,
                metadata: _,
                update_metadata: _,
            }) => {
                let guild = self.guilds.entry(guild_id).or_default();

                if update_name {
                    guild.name = name;
                }
                if update_picture {
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
                role,
            }) => {
                self.get_guild(guild_id).and_do(|g| {
                    role.and_do(|role| {
                        g.roles.insert(role_id, role.into());
                    });
                });
            }
            Event::RoleMoved(RoleMoved {
                guild_id,
                role_id,
                previous_id,
                next_id,
            }) => {
                if previous_id != 0 && next_id != 0 {
                    self.get_guild(guild_id).and_do(|g| {
                        g.update_role_order(previous_id, next_id, role_id);
                    });
                }
            }
            Event::UserRolesUpdated(UserRolesUpdated {
                guild_id,
                user_id,
                role_ids,
            }) => {
                self.get_guild(guild_id).and_do(|g| {
                    g.members.insert(user_id, role_ids);
                });
            }
            x => tracing::warn!("implement {:?}", x),
        }

        post
    }

    pub fn process_get_message_history_response(
        &mut self,
        guild_id: u64,
        channel_id: u64,
        messages: Vec<HarmonyMessage>,
        reached_top: bool,
    ) -> Vec<PostProcessEvent> {
        let mut post = Vec::new();
        let mut messages = harmony_messages_to_ui_messages(
            messages
                .into_iter()
                .map(|msg| (MessageId::Ack(msg.message_id), msg))
                .collect(),
        );

        for message in messages.values() {
            message.post_process(&mut post);
        }

        for overrides in messages.values().flat_map(|msg| msg.overrides.as_ref()) {
            if let Some(id) = overrides.avatar_url.clone() {
                post.push(PostProcessEvent::FetchThumbnail(Attachment {
                    kind: "image".into(),
                    name: "avatar".into(),
                    ..Attachment::new_unknown(id)
                }));
            }
        }

        if let Some(channel) = self.get_channel(guild_id, channel_id) {
            messages.extend(channel.messages.drain(..));
            channel.messages = messages;
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

pub fn render_text(textt: &str, members: &Members) -> String {
    use byte_writer::Writer;
    use regex::Regex;
    use std::fmt::Write;

    lazy_static::lazy_static! {
        static ref MENTION: Regex = Regex::new("<@(?P<id>[0-9]*)>").unwrap();
        static ref EMOTE: Regex = Regex::new("<:(.*):>").unwrap();
    }

    // TODO: this is horribly inefficient
    let mut text = textt.to_string();
    for capture in MENTION.captures_iter(textt) {
        let user_id = capture.name("id").unwrap().as_str();
        if let Ok(parsed_user_id) = user_id.parse::<u64>() {
            let member_name = members
                .get(&parsed_user_id)
                .map_or_else(|| "unknown user", |m| m.username.as_str());
            let mut pattern_arr = [b'0'; 23];
            write!(Writer(&mut pattern_arr), "<@{}>", user_id).unwrap();
            text = text.replace(
                (unsafe { std::str::from_utf8_unchecked(&pattern_arr) }).trim_end_matches(|c| c != '>'),
                &format!("@{}", member_name),
            );
        }
    }
    text
}

pub mod color {
    pub fn encode_rgb(color: (u8, u8, u8)) -> i64 {
        let mut c = (color.0 * 255) as i64;
        c = (c << 8) + (color.1 * 255) as i64;
        c = (c << 8) + (color.2 * 255) as i64;
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
