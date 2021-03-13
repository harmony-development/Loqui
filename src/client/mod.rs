#![allow(clippy::field_reassign_with_default)]

pub mod channel;
pub mod content;
pub mod error;
pub mod guild;
pub mod member;
pub mod message;

use channel::Channel;
use guild::Guild;
pub use harmony_rust_sdk::{
    api::exports::hrpc::url::Url,
    client::{api::auth::Session as InnerSession, AuthStatus, Client as InnerClient},
};
use harmony_rust_sdk::{
    api::{
        chat::event::*,
        harmonytypes::{Message as HarmonyMessage, UserStatus},
    },
    client::api::{chat::EventSource, rest::FileId},
};

use content::ContentStore;
use error::{ClientError, ClientResult};
use member::{Member, Members};
use message::{harmony_messages_to_ui_messages, Attachment, Embed, MessageId, Override};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Debug, Formatter},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Instant,
};

use crate::ui::component::event_history::SHOWN_MSGS_LIMIT;

use self::{
    guild::Guilds,
    message::{EmbedHeading, Message},
};

/// A sesssion struct with our requirements (unlike the `InnerSession` type)
#[derive(Clone, Deserialize, Serialize)]
pub struct Session {
    pub session_token: String,
    pub user_id: String,
    pub homeserver: String,
}

impl Debug for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user_id)
            .field("homeserver", &self.homeserver)
            .finish()
    }
}

impl Into<InnerSession> for Session {
    fn into(self) -> InnerSession {
        InnerSession {
            user_id: self.user_id.parse().unwrap(),
            session_token: self.session_token,
        }
    }
}

#[derive(Debug)]
pub enum PostProcessEvent {
    FetchProfile(u64),
    FetchGuildData(u64),
    FetchThumbnail(FileId),
    GoToFirstMsgOnChannel(u64),
    Nothing,
}

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
                &format!(
                    "{:?}",
                    self.auth_status().session().map_or(0, |s| s.user_id)
                ),
            )
            .field("session_file", &self.content_store.session_file())
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
            guilds: Guilds::new(),
            members: Members::new(),
            user_id: session.as_ref().map(|s| s.user_id),
            content_store,
            inner: InnerClient::new(homeserver_url, session).await?,
        })
    }

    pub async fn logout(_inner: InnerClient, session_file: PathBuf) -> ClientResult<()> {
        tokio::fs::remove_file(session_file).await?;
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
    pub fn get_guild(&mut self, guild_id: u64) -> Option<&mut Guild> {
        self.guilds.get_mut(&guild_id)
    }

    pub fn get_channel(&mut self, guild_id: u64, channel_id: u64) -> Option<&mut Channel> {
        self.get_guild(guild_id)
            .map(|guild| guild.channels.get_mut(&channel_id))
            .flatten()
    }

    #[inline(always)]
    pub fn get_member(&mut self, user_id: u64) -> Option<&mut Member> {
        self.members.get_mut(&user_id)
    }

    pub fn process_event(&mut self, event: Event) -> Vec<PostProcessEvent> {
        let mut post = Vec::new();

        match event {
            Event::SentMessage(message_sent) => {
                let echo_id = message_sent.echo_id;

                if let Some(message) = message_sent.message {
                    let guild_id = message.guild_id;
                    let channel_id = message.channel_id;
                    let message_id = message.message_id;

                    if let Some(channel) = self.get_channel(guild_id, channel_id) {
                        let message = Message::from(message);

                        if let Some(id) = message
                            .overrides
                            .as_ref()
                            .map(|overrides| overrides.avatar_url.clone())
                            .flatten()
                        {
                            post.push(PostProcessEvent::FetchThumbnail(id));
                        }

                        for attachment in &message.attachments {
                            if attachment.is_thumbnail() {
                                post.push(PostProcessEvent::FetchThumbnail(attachment.id.clone()));
                            }
                        }

                        for embed in &message.embeds {
                            post_heading(&mut post, &embed);
                        }

                        if let Some(msg) = channel
                            .messages
                            .iter_mut()
                            .find(|omsg| omsg.id == MessageId::Unack(echo_id))
                        {
                            *msg = message;
                        } else if let Some(msg) = channel
                            .messages
                            .iter_mut()
                            .find(|omsg| omsg.id == MessageId::Ack(message_id))
                        {
                            *msg = message;
                        } else {
                            channel.messages.push(message);
                        }

                        let disp = channel.messages.len();
                        if channel.looking_at_message >= disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                            channel.looking_at_message = disp.saturating_sub(1);
                            post.push(PostProcessEvent::GoToFirstMsgOnChannel(channel_id));
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
                    if let Some(pos) = channel
                        .messages
                        .iter()
                        .position(|msg| msg.id == MessageId::Ack(message_id))
                    {
                        channel.messages.remove(pos);
                    }
                }
            }
            Event::EditedMessage(message_updated) => {
                let guild_id = message_updated.guild_id;
                let channel_id = message_updated.channel_id;

                if let Some(channel) = self.get_channel(guild_id, channel_id) {
                    if let Some(msg) = channel
                        .messages
                        .iter_mut()
                        .find(|message| message.id == MessageId::Ack(message_updated.message_id))
                    {
                        if message_updated.update_content {
                            msg.content = message_updated.content;
                        }
                        if message_updated.update_attachments {
                            msg.attachments = message_updated
                                .attachments
                                .into_iter()
                                .flat_map(Attachment::from_harmony_attachment)
                                .collect();
                            for attachment in &msg.attachments {
                                if attachment.is_thumbnail() {
                                    post.push(PostProcessEvent::FetchThumbnail(
                                        attachment.id.clone(),
                                    ));
                                }
                            }
                        }
                        if message_updated.update_overrides {
                            msg.overrides = message_updated.overrides.map(|overrides| {
                                let overrides: Override = overrides.into();
                                if let Some(id) = overrides.avatar_url.clone() {
                                    post.push(PostProcessEvent::FetchThumbnail(id));
                                }
                                overrides
                            });
                        }
                        if message_updated.update_embeds {
                            msg.embeds =
                                message_updated.embeds.into_iter().map(From::from).collect();
                            for embed in &msg.embeds {
                                post_heading(&mut post, &embed);
                            }
                        }
                    }
                }
            }
            Event::DeletedChannel(ChannelDeleted {
                guild_id,
                channel_id,
            }) => {
                if let Some(guild) = self.get_guild(guild_id) {
                    guild.channels.remove(&channel_id);
                }
            }
            Event::EditedChannel(ChannelUpdated {
                guild_id,
                channel_id,
                name,
                update_name,
                previous_id: _,
                next_id: _,
                update_order: _,
                metadata: _,
                update_metadata: _,
            }) => {
                if let Some(channel) = self.get_channel(guild_id, channel_id) {
                    if update_name {
                        channel.name = name;
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
                    let prev_pos = guild.channels.keys().position(|id| *id == previous_id);
                    let next_pos = guild.channels.keys().position(|id| *id == next_id);

                    guild.channels.insert(
                        channel_id,
                        Channel {
                            is_category,
                            name,
                            loading_messages_history: false,
                            looking_at_message: 0,
                            messages: Vec::new(),
                        },
                    );

                    if let Some(pos) = prev_pos {
                        if pos != guild.channels.len() - 1 {
                            guild
                                .channels
                                .swap_indices(pos + 1, guild.channels.len() - 1);
                        }
                    } else if let Some(pos) = next_pos {
                        if pos != 0 {
                            guild
                                .channels
                                .swap_indices(pos - 1, guild.channels.len() - 1);
                        } else {
                            let (k, v) = guild.channels.pop().unwrap();
                            guild.channels.reverse();
                            guild.channels.insert(k, v);
                            guild.channels.reverse();
                        }
                    }
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
            Event::JoinedMember(MemberJoined {
                guild_id,
                member_id,
            }) => {
                if member_id == 0 {
                    return post;
                }

                if let Some(guild) = self.get_guild(guild_id) {
                    guild.members.insert(member_id);
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
                is_bot: _,
                update_is_bot: _,
            }) => {
                let member = self.members.entry(user_id).or_default();
                if update_username {
                    member.username = new_username;
                }
                if update_status {
                    member.status = UserStatus::from_i32(new_status).unwrap();
                }
                if update_avatar {
                    let parsed = FileId::from_str(&new_avatar).ok();
                    member.avatar_url = parsed.clone();
                    if let Some(id) = parsed {
                        post.push(PostProcessEvent::FetchThumbnail(id));
                    }
                };
            }
            Event::GuildAddedToList(GuildAddedToList {
                guild_id,
                homeserver: _,
            }) => {
                self.guilds.insert(guild_id, Default::default());
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
                        post.push(PostProcessEvent::FetchThumbnail(id));
                    }
                }
            }
            x => todo!("implement {:?}", x),
        }

        post
    }

    pub fn process_get_message_history_response(
        &mut self,
        guild_id: u64,
        channel_id: u64,
        messages: Vec<HarmonyMessage>,
        _reached_top: bool,
    ) -> Vec<PostProcessEvent> {
        let mut post = Vec::new();
        let mut messages = harmony_messages_to_ui_messages(messages);

        for attachment in messages.iter().flat_map(|msg| &msg.attachments) {
            if attachment.is_thumbnail() {
                post.push(PostProcessEvent::FetchThumbnail(attachment.id.clone()));
            }
        }

        for overrides in messages.iter().flat_map(|msg| msg.overrides.as_ref()) {
            if let Some(id) = overrides.avatar_url.clone() {
                post.push(PostProcessEvent::FetchThumbnail(id));
            }
        }

        for embed in messages.iter().flat_map(|msg| &msg.embeds) {
            post_heading(&mut post, &embed);
        }

        if let Some(channel) = self.get_channel(guild_id, channel_id) {
            messages.append(&mut channel.messages);
            channel.messages = messages;
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
        if let Some(id) = h.map(|h| h.icon.clone()).flatten() {
            post.push(PostProcessEvent::FetchThumbnail(id));
        }
    };
    inner(embed.header.as_ref());
    inner(embed.footer.as_ref());
}
