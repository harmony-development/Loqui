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
        chat::event::Event,
        harmonytypes::{Message as HarmonyMessage, UserStatus},
    },
    client::api::{chat::EventSource, rest::FileId},
};

use content::{ContentStore, ContentType};
use error::{ClientError, ClientResult};
use member::{Member, Members};
use message::{harmony_messages_to_ui_messages, Attachment, MessageId, Override};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Debug, Formatter},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use crate::ui::component::event_history::SHOWN_MSGS_LIMIT;

use self::{guild::Guilds, message::Message};

/// A sesssion struct with our requirements (unlike the `InnerSession` type)
#[derive(Clone, Deserialize, Serialize)]
pub struct Session {
    pub session_token: String,
    pub user_id: u64,
    pub homeserver: String,
}

impl Debug for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user_id.to_string())
            .field("homeserver", &self.homeserver)
            .finish()
    }
}

impl Into<InnerSession> for Session {
    fn into(self) -> InnerSession {
        InnerSession {
            user_id: self.user_id,
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

    pub fn content_store(&self) -> &ContentStore {
        &self.content_store
    }

    pub fn content_store_arc(&self) -> Arc<ContentStore> {
        self.content_store.clone()
    }

    pub fn auth_status(&self) -> AuthStatus {
        self.inner.auth_status()
    }

    pub fn inner(&self) -> &InnerClient {
        &self.inner
    }

    pub fn get_guild(&mut self, guild_id: u64) -> Option<&mut Guild> {
        self.guilds.get_mut(&guild_id)
    }

    pub fn get_channel(&mut self, guild_id: u64, channel_id: u64) -> Option<&mut Channel> {
        self.get_guild(guild_id)
            .map(|guild| guild.channels.get_mut(&channel_id))
            .flatten()
    }

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

                        if let Some(msg) = channel
                            .messages
                            .iter_mut()
                            .find(|message| message.id == MessageId::Unack(echo_id))
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
            Event::DeletedMessage(message_deleted) => {
                let guild_id = message_deleted.guild_id;
                let channel_id = message_deleted.channel_id;
                let message_id = message_deleted.message_id;

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
                                .flat_map(|attachment| {
                                    Some(Attachment {
                                        id: FileId::from_str(&attachment.id).ok()?,
                                        kind: ContentType::new(&attachment.r#type),
                                        name: attachment.name,
                                        size: attachment.size as u32,
                                    })
                                })
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
                            if let Some((parsed, overrides)) =
                                message_updated.overrides.map(|overrides| {
                                    let parsed = FileId::from_str(&overrides.avatar).ok();
                                    (
                                        parsed.clone(),
                                        Override {
                                            avatar_url: parsed,
                                            name: overrides.name,
                                            reason: overrides.reason,
                                        },
                                    )
                                })
                            {
                                msg.overrides = Some(overrides);
                                if let Some(id) = parsed {
                                    post.push(PostProcessEvent::FetchThumbnail(id));
                                }
                            } else {
                                msg.overrides = None;
                            }
                        }
                    }
                }
            }
            Event::DeletedChannel(channel_deleted) => {
                let guild_id = channel_deleted.guild_id;
                let channel_id = channel_deleted.channel_id;

                if let Some(guild) = self.get_guild(guild_id) {
                    guild.channels.remove(&channel_id);
                }
            }
            Event::EditedChannel(channel_updated) => {
                let guild_id = channel_updated.guild_id;
                let channel_id = channel_updated.channel_id;

                if let Some(channel) = self.get_channel(guild_id, channel_id) {
                    if channel_updated.update_name {
                        channel.name = channel_updated.name;
                    }
                }
            }
            Event::CreatedChannel(channel_created) => {
                let guild_id = channel_created.guild_id;
                let channel_id = channel_created.channel_id;

                if let Some(guild) = self.get_guild(guild_id) {
                    guild.channels.insert(
                        channel_id,
                        Channel {
                            is_category: channel_created.is_category,
                            name: channel_created.name,
                            loading_messages_history: false,
                            looking_at_message: 0,
                            messages: Vec::new(),
                        },
                    );
                }
            }
            Event::Typing(typing) => {
                let channel_id = typing.channel_id;
                let user_id = typing.user_id;

                if let Some(member) = self.get_member(user_id) {
                    member.typing_in_channel = Some(channel_id);
                }
            }
            Event::JoinedMember(member_joined) => {
                let guild_id = member_joined.guild_id;
                let member_id = member_joined.member_id;

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
            Event::LeftMember(member_left) => {
                let guild_id = member_left.guild_id;
                let member_id = member_left.member_id;

                if let Some(guild) = self.get_guild(guild_id) {
                    guild.members.remove(&member_id);
                }
            }
            Event::ProfileUpdated(profile_updated) => {
                let user_id = profile_updated.user_id;

                let member = self.members.entry(user_id).or_default();

                if profile_updated.update_username {
                    member.username = profile_updated.new_username;
                }

                if profile_updated.update_status {
                    member.status = UserStatus::from_i32(profile_updated.new_status).unwrap();
                }

                if profile_updated.update_avatar {
                    let parsed = FileId::from_str(&profile_updated.new_avatar).ok();
                    member.avatar_url = parsed.clone();
                    if let Some(id) = parsed {
                        post.push(PostProcessEvent::FetchThumbnail(id));
                    }
                };
            }
            Event::GuildAddedToList(guild_added) => {
                let guild_id = guild_added.guild_id;
                self.guilds.insert(guild_id, Default::default());
                post.push(PostProcessEvent::FetchGuildData(guild_id));
            }
            Event::GuildRemovedFromList(guild_removed) => {
                self.guilds.remove(&guild_removed.guild_id);
            }
            Event::DeletedGuild(guild_deleted) => {
                self.guilds.remove(&guild_deleted.guild_id);
            }
            Event::EditedGuild(guild_updated) => {
                let guild_id = guild_updated.guild_id;
                let guild = self.guilds.entry(guild_id).or_default();

                if guild_updated.update_name {
                    guild.name = guild_updated.name;
                }
                if guild_updated.update_picture {
                    let parsed = FileId::from_str(&guild_updated.picture).ok();
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

        if let Some(channel) = self.get_channel(guild_id, channel_id) {
            messages.append(&mut channel.messages);
            channel.messages = messages;
        }

        post
    }

    pub fn subscribe_to(&self) -> Vec<EventSource> {
        self.guilds
            .keys()
            .map(|guild_id| EventSource::Guild(*guild_id))
            .collect()
    }
}
