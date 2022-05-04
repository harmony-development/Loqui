#![feature(let_else)]
#![allow(clippy::field_reassign_with_default)]

pub mod channel;
pub mod content;
pub mod emotes;
pub mod error;
pub mod guild;
pub mod member;
pub mod message;
pub mod role;

use channel::Channel;
use guild::Guild;
pub use harmony_rust_sdk::{
    self,
    api::exports::hrpc::exports::http::Uri,
    client::{AuthStatus, Client as InnerClient},
};
use harmony_rust_sdk::{
    api::{
        auth::Session as InnerSession,
        chat::{
            all_permissions, color, embed,
            get_channel_messages_request::Direction,
            send_message_request,
            stream_event::{Event as ChatEvent, *},
            Attachment, BanUserRequest, ChannelKind, CreateChannelRequest, CreateGuildRequest, CreateInviteRequest,
            DeleteChannelRequest, DeleteInviteRequest, DeleteMessageRequest, Embed, Event, GetChannelMessagesRequest,
            GetGuildChannelsRequest, GetGuildInvitesRequest, GetGuildListRequest, GetGuildMembersRequest,
            GetGuildRequest, GetGuildRolesRequest, GetMessageRequest, GetPermissionsRequest, GetPinnedMessagesRequest,
            GetPrivateChannelListRequest, GetPrivateChannelRequest, GetUserRolesRequest, HasPermissionRequest, Invite,
            JoinGuildRequest, KickUserRequest, LeaveGuildRequest, Message as HarmonyMessage, Permission,
            PinMessageRequest, Role, SendMessageRequest, SetPermissionsRequest, TypingRequest, UnbanUserRequest,
            UnpinMessageRequest, UpdateChannelInformationRequest, UpdateGuildInformationRequest,
            UpdateMessageContentRequest,
        },
        emote::{stream_event::Event as EmoteEvent, *},
        mediaproxy::{fetch_link_metadata_response::metadata::Data as FetchLinkData, FetchLinkMetadataRequest},
        profile::{stream_event::Event as ProfileEvent, UserStatus, *},
        rest::{About, FileId},
    },
    client::{rest::DownloadedFile, EventsSocket},
};

use error::ClientResult;
use instant::Instant;
use member::Member;
use message::{AttachmentExt, MessageId, Messages, ReadMessagesView, WriteMessagesView};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use std::{
    collections::HashMap,
    fmt::{self, Debug, Display, Formatter},
    ops::Not,
};

use crate::emotes::EmotePack;

use self::message::Message;

pub use ahash::{AHashMap, AHashSet, AHasher};
pub use smol_str;
pub use tracing;
pub use urlencoding;

pub type IndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;
pub type EventSender = tokio::sync::mpsc::UnboundedSender<FetchEvent>;
pub type EventReceiver = tokio::sync::mpsc::UnboundedReceiver<FetchEvent>;
pub type PostEventSender = tokio::sync::mpsc::UnboundedSender<PostProcessEvent>;
pub type PostEventReceiver = tokio::sync::mpsc::UnboundedReceiver<PostProcessEvent>;

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
            guest_token: None,
        }
    }
}

#[derive(Debug)]
pub enum PostProcessEvent {
    FetchProfile(u64),
    FetchGuildData(u64),
    FetchPrivateChannel(u64),
    FetchThumbnail(Attachment),
    CheckPermsForChannel(u64, u64),
    FetchMessage {
        guild_id: Option<u64>,
        channel_id: u64,
        message_id: u64,
    },
    FetchLinkMetadata(Vec<Uri>),
    FetchEmotes(u64),
}

pub enum FetchEvent {
    Harmony(Event),
    AddInvite {
        guild_id: u64,
        id: String,
        invite: Invite,
    },
    FetchedMsgsPins(u64, u64),
    FetchedInvites(u64),
    LinkMetadata {
        url: Uri,
        data: FetchLinkData,
    },
    Attachment {
        attachment: Attachment,
        file: DownloadedFile,
    },
    FetchedReply {
        guild_id: Option<u64>,
        channel_id: u64,
        message_id: u64,
        message: HarmonyMessage,
    },
    FailedToSendMessage {
        guild_id: Option<u64>,
        channel_id: u64,
        echo_id: u64,
    },
    FetchedMessageHistory {
        guild_id: Option<u64>,
        channel_id: u64,
        messages: Vec<(u64, HarmonyMessage)>,
        anchor: Option<u64>,
        direction: Direction,
        reached_top: bool,
    },
    FetchedGuild {
        id: u64,
        name: String,
        picture: Option<String>,
        owners: Vec<u64>,
    },
    FetchedPrivateChannel {
        id: u64,
        name: Option<String>,
        members: Vec<u64>,
        is_dm: bool,
    },
    FetchedProfile {
        id: u64,
        name: String,
        picture: Option<String>,
        status: UserStatus,
        kind: AccountKind,
    },
    InitialSyncComplete,
}

impl FetchEvent {
    fn new_chat(ev: ChatEvent) -> Self {
        Self::Harmony(Event::Chat(ev))
    }
}

pub struct Cache {
    users: AHashMap<u64, Member>,
    guilds: AHashMap<u64, Guild>,
    channels: AHashMap<(u64, u64), Channel>,
    link_embeds: AHashMap<Uri, FetchLinkData>,
    emote_packs: AHashMap<u64, EmotePack>,
    initial_sync_complete: bool,
    event_receiver: EventReceiver,
    post_sender: PostEventSender,
}

impl Cache {
    pub fn new(event_receiver: EventReceiver, post_sender: PostEventSender) -> Self {
        let mut this = Self {
            event_receiver,
            post_sender,
            channels: Default::default(),
            emote_packs: Default::default(),
            guilds: Default::default(),
            initial_sync_complete: false,
            link_embeds: Default::default(),
            users: Default::default(),
        };

        // setup dm guild
        let dms = this.get_guild_mut(0);
        dms.name = "DMs".into();
        dms.fetched = true;
        dms.fetched_invites = true;

        this
    }

    pub fn maintain(&mut self, mut event_fn: impl FnMut(FetchEvent) -> Option<FetchEvent>) {
        for member in self.users.values_mut() {
            if let Some((_, _, time)) = member.typing_in_channel {
                if time.elapsed().as_secs() > 5 {
                    member.typing_in_channel = None;
                }
            }
        }

        while let Ok(ev) = self.event_receiver.try_recv() {
            if let Some(ev) = (event_fn)(ev) {
                self.process_event(ev);
            }
        }
    }

    fn get_guild_mut(&mut self, guild_id: u64) -> &mut Guild {
        self.guilds.entry(guild_id).or_default()
    }

    fn get_channel_mut(&mut self, guild_id: u64, channel_id: u64) -> &mut Channel {
        self.channels.entry((guild_id, channel_id)).or_default()
    }

    fn get_priv_channel_mut(&mut self, channel_id: u64) -> &mut Channel {
        self.get_channel_mut(0, channel_id)
    }

    fn get_user_mut(&mut self, user_id: u64) -> &mut Member {
        self.users.entry(user_id).or_default()
    }

    fn get_emote_pack_mut(&mut self, pack_id: u64) -> &mut EmotePack {
        self.emote_packs.entry(pack_id).or_default()
    }

    fn get_messages(&mut self, guild_id: Option<u64>, channel_id: u64) -> &mut Messages {
        &mut self.get_channel_mut(guild_id.unwrap_or(0), channel_id).messages
    }

    pub fn is_initial_sync_complete(&self) -> bool {
        self.initial_sync_complete
    }

    pub fn get_guild(&self, guild_id: u64) -> Option<&Guild> {
        self.guilds.get(&guild_id)
    }

    pub fn get_guilds(&self) -> impl Iterator<Item = (u64, &Guild)> + '_ {
        self.guilds.iter().map(|(id, g)| (*id, g))
    }

    pub fn get_channel(&self, guild_id: u64, channel_id: u64) -> Option<&Channel> {
        self.channels.get(&(guild_id, channel_id))
    }

    pub fn get_channels(&self, guild_id: u64) -> Vec<(u64, &Channel)> {
        let ids = if let Some(g) = self.get_guild(guild_id) {
            g.channels.as_slice()
        } else {
            return Vec::new();
        };
        let mut channels = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(chan) = self.get_channel(guild_id, *id) {
                channels.push((*id, chan));
            }
        }
        channels
    }

    pub fn get_user(&self, user_id: u64) -> Option<&Member> {
        self.users.get(&user_id)
    }

    pub fn get_emote_pack(&self, pack_id: u64) -> Option<&EmotePack> {
        self.emote_packs.get(&pack_id)
    }

    pub fn get_emote_name(&self, image_id: &str) -> Option<&str> {
        self.emote_packs
            .iter()
            .filter_map(|(_, pack)| pack.emotes.get(image_id))
            .next()
            .map(|s| s.as_str())
    }

    pub fn get_all_emotes(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.emote_packs
            .iter()
            .flat_map(|(_, pack)| pack.emotes.iter())
            .map(|(id, name)| (id.as_str(), name.as_str()))
    }

    pub fn get_link_data(&self, url: &Uri) -> Option<&FetchLinkData> {
        self.link_embeds.get(url)
    }

    pub fn has_perm(&self, guild_id: u64, channel_id: Option<u64>, query: &str) -> bool {
        channel_id
            .and_then(|channel_id| self.get_channel(guild_id, channel_id))
            .map_or(false, |c| c.has_perm(query))
            || self.get_guild(guild_id).map_or(false, |g| g.has_perm(query))
    }

    pub fn process_event(&mut self, event: FetchEvent) {
        match event {
            FetchEvent::FetchedPrivateChannel {
                id,
                is_dm,
                members,
                name,
            } => {
                for member in &members {
                    if self.users.contains_key(member).not() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchProfile(*member));
                    }
                }

                let mut channel = self.get_channel_mut(0, id);
                channel.name = name.map_or_else(|| SmolStr::new_inline(""), Into::into);
                if let Some(data) = channel.private_channel_data.as_mut() {
                    data.members = members.into_iter().collect();
                    data.is_dm = is_dm;
                }
                channel.get_priv_mut().fetched = true;
            }
            FetchEvent::FetchedGuild {
                id,
                name,
                picture,
                owners,
            } => {
                let mut guild = self.get_guild_mut(id);
                guild.name = name.into();
                guild.picture = picture;
                guild.owners = owners;
                guild.fetched = true;
                if let Some(id) = guild.picture.clone() {
                    let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(Attachment {
                        mimetype: "image".into(),
                        name: "guild".into(),
                        id,
                        ..Default::default()
                    }));
                }
            }
            FetchEvent::FetchedMessageHistory {
                guild_id,
                channel_id,
                messages,
                anchor,
                reached_top,
                direction,
            } => {
                self.process_get_message_history_response(
                    guild_id,
                    channel_id,
                    anchor,
                    messages,
                    reached_top,
                    direction,
                );
            }
            FetchEvent::FailedToSendMessage {
                guild_id,
                channel_id,
                echo_id,
            } => {
                let messages = self.get_messages(guild_id, channel_id);
                let mut view = messages.view_mut();
                if let Some(msg) = view.get_message_mut(&MessageId::Unack(echo_id)) {
                    msg.failed_to_send = true;
                }
            }
            FetchEvent::Harmony(event) => self.process_harmony_event(event),
            FetchEvent::AddInvite { guild_id, id, invite } => {
                self.get_guild_mut(guild_id).invites.insert(id, invite);
            }
            FetchEvent::FetchedMsgsPins(guild_id, channel_id) => {
                self.get_channel_mut(guild_id, channel_id).fetched_msgs_pins = true;
            }
            FetchEvent::FetchedInvites(guild_id) => {
                self.get_guild_mut(guild_id).fetched_invites = true;
            }
            FetchEvent::LinkMetadata { url, data } => {
                match &data {
                    FetchLinkData::IsSite(site) => {
                        if let Some(url) = site.thumbnail.first().and_then(|i| i.url.parse::<Uri>().ok()) {
                            let id = FileId::External(url);
                            let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(Attachment {
                                id: id.into(),
                                ..Default::default()
                            }));
                        }
                    }
                    FetchLinkData::IsMedia(media) => {
                        let attachment = Attachment {
                            name: media.name.clone(),
                            mimetype: media.mimetype.clone(),
                            size: media.size.unwrap_or(u32::MAX),
                            id: url.to_string(),
                            ..Default::default()
                        };

                        if attachment.is_thumbnail() {
                            let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(attachment));
                        }
                    }
                }
                self.link_embeds.insert(url, data);
            }
            FetchEvent::FetchedReply {
                guild_id,
                channel_id,
                message_id,
                message,
            } => {
                let message: Message = message.into();
                let id = MessageId::Ack(message_id);
                let mut urls = Vec::new();
                message.post_process(&self.post_sender, &mut urls, guild_id, channel_id);
                if urls.is_empty().not() {
                    let _ = self.post_sender.send(PostProcessEvent::FetchLinkMetadata(urls));
                }

                let messages = self.get_messages(guild_id, channel_id);

                messages.create_reply_view(id).insert_message(id, message);
            }
            FetchEvent::Attachment { .. } => {}
            FetchEvent::InitialSyncComplete => {
                self.initial_sync_complete = true;
            }
            FetchEvent::FetchedProfile {
                id,
                name,
                picture,
                status,
                kind,
            } => {
                if let Some(id) = picture.clone() {
                    let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(Attachment {
                        mimetype: "image".into(),
                        name: "avatar".into(),
                        id,
                        ..Default::default()
                    }));
                }
                let mut user = self.get_user_mut(id);
                user.username = name.into();
                user.avatar_url = picture;
                user.kind = kind;
                user.status = status;
                user.fetched = true;
            }
        }
    }

    pub fn process_harmony_event(&mut self, event: Event) {
        match event {
            Event::Chat(ev) => match ev {
                ChatEvent::PermissionUpdated(perm) => {
                    let PermissionUpdated {
                        guild_id,
                        channel_id,
                        query,
                        ok,
                    } = perm;
                    let perm = Permission { matches: query, ok };

                    if let Some(channel_id) = channel_id {
                        self.get_channel_mut(guild_id, channel_id).perms.push(perm);
                    } else {
                        self.get_guild_mut(guild_id).perms.push(perm);
                    }
                }
                ChatEvent::SentMessage(MessageSent {
                    echo_id,
                    guild_id,
                    channel_id,
                    message_id,
                    message,
                }) => {
                    if let Some(message) = message {
                        let message = Message::from(message);
                        let mut urls = Vec::new();
                        message.post_process(&self.post_sender, &mut urls, guild_id, channel_id);
                        if urls.is_empty().not() {
                            let _ = self.post_sender.send(PostProcessEvent::FetchLinkMetadata(urls));
                        }

                        let messages = self.get_messages(guild_id, channel_id);

                        let message_view = messages.continuous_view_mut();
                        if let Some(echo_id) = echo_id {
                            message_view.ack_message(MessageId::Unack(echo_id), MessageId::Ack(message_id), message);
                        } else {
                            message_view.insert_message(MessageId::Ack(message_id), message);
                        }
                    }
                }
                ChatEvent::DeletedMessage(MessageDeleted {
                    guild_id,
                    channel_id,
                    message_id,
                }) => {
                    let messages = self.get_messages(guild_id, channel_id);

                    messages
                        .continuous_view_mut()
                        .remove_message(&MessageId::Ack(message_id));
                }
                ChatEvent::EditedMessage(message_updated) => {
                    let guild_id = message_updated.guild_id;
                    let channel_id = message_updated.channel_id;
                    let message_id = MessageId::Ack(message_updated.message_id);

                    let messages = self.get_messages(guild_id, channel_id);

                    if let Some(msg) = messages.view_mut().get_message_mut(&message_id) {
                        msg.content.text = message_updated.new_content.map_or_else(String::new, |f| f.text);
                    }

                    let maybe_view = self
                        .get_channel(guild_id.unwrap_or(0), channel_id)
                        .map(|chan| chan.messages.view());
                    if let Some(msg) = maybe_view.as_ref().and_then(|view| view.get_message(&message_id)) {
                        let mut urls = Vec::new();
                        msg.post_process(&self.post_sender, &mut urls, guild_id, channel_id);
                        if urls.is_empty().not() {
                            let _ = self.post_sender.send(PostProcessEvent::FetchLinkMetadata(urls));
                        }
                    }
                }
                ChatEvent::DeletedChannel(ChannelDeleted { guild_id, channel_id }) => {
                    self.channels.remove(&(guild_id, channel_id));
                    let guild = self.get_guild_mut(guild_id);
                    if let Some(pos) = guild.channels.iter().position(|id| channel_id.eq(id)) {
                        guild.channels.remove(pos);
                    }
                }
                ChatEvent::EditedChannel(ChannelUpdated {
                    guild_id,
                    channel_id,
                    new_name,
                    new_metadata: _,
                }) => {
                    if let Some(name) = new_name {
                        self.get_channel_mut(guild_id, channel_id).name = name.into();
                    }
                }
                ChatEvent::EditedChannelPosition(ChannelPositionUpdated {
                    guild_id,
                    channel_id,
                    new_position,
                }) => {
                    if let Some(position) = new_position {
                        self.get_guild_mut(guild_id).update_channel_order(position, channel_id);
                    }
                }
                ChatEvent::ChannelsReordered(ChannelsReordered { guild_id, channel_ids }) => {
                    self.get_guild_mut(guild_id).channels = channel_ids;
                }
                ChatEvent::CreatedChannel(ChannelCreated {
                    guild_id,
                    channel_id,
                    name,
                    position,
                    kind,
                    metadata: _,
                }) => {
                    let channel = self.get_channel_mut(guild_id, channel_id);
                    channel.name = name.into();
                    channel.is_category = kind == i32::from(ChannelKind::Category);

                    let guild = self.get_guild_mut(guild_id);
                    // [tag:channel_added_to_client]
                    guild.channels.push(channel_id);
                    if let Some(position) = position {
                        guild.update_channel_order(position, channel_id);
                    }

                    let _ = self
                        .post_sender
                        .send(PostProcessEvent::CheckPermsForChannel(guild_id, channel_id));
                }
                ChatEvent::Typing(Typing {
                    guild_id,
                    channel_id,
                    user_id,
                }) => {
                    self.get_user_mut(user_id).typing_in_channel = Some((guild_id, channel_id, Instant::now()));
                }
                ChatEvent::JoinedMember(MemberJoined { guild_id, member_id }) => {
                    if member_id == 0 {
                        return;
                    }

                    self.get_guild_mut(guild_id).members.insert(member_id, Vec::new());

                    if !self.users.contains_key(&member_id) {
                        let _ = self.post_sender.send(PostProcessEvent::FetchProfile(member_id));
                    }
                }
                ChatEvent::LeftMember(MemberLeft {
                    guild_id,
                    member_id,
                    leave_reason: _,
                }) => {
                    self.get_guild_mut(guild_id).members.remove(&member_id);
                }
                ChatEvent::GuildAddedToList(GuildAddedToList { guild_id, server_id }) => {
                    let guild = self.get_guild_mut(guild_id);
                    guild.homeserver = server_id;

                    if guild.fetched.not() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchGuildData(guild_id));
                    }
                }
                ChatEvent::GuildRemovedFromList(GuildRemovedFromList { guild_id, server_id: _ }) => {
                    self.guilds.remove(&guild_id);
                }
                ChatEvent::DeletedGuild(GuildDeleted { guild_id }) => {
                    self.guilds.remove(&guild_id);
                }
                ChatEvent::EditedGuild(GuildUpdated {
                    guild_id,
                    new_name,
                    new_picture,
                    new_metadata: _,
                }) => {
                    let fetched = new_name.is_some() && new_picture.is_some();
                    if let Some(id) = new_picture.clone() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(Attachment {
                            mimetype: "image".into(),
                            name: "guild".into(),
                            id,
                            ..Default::default()
                        }));
                    }
                    let mut guild = self.get_guild_mut(guild_id);

                    if fetched {
                        guild.fetched = true;
                    }

                    if let Some(name) = new_name {
                        guild.name = name.into();
                    }
                    if let Some(picture) = new_picture {
                        guild.picture = Some(picture);
                    }
                }
                ChatEvent::RoleCreated(RoleCreated {
                    guild_id,
                    role_id,
                    color,
                    hoist,
                    name,
                    pingable,
                }) => {
                    self.get_guild_mut(guild_id)
                        .roles
                        .insert(role_id, Role::new(name, color, hoist, pingable).into());
                }
                ChatEvent::RoleDeleted(RoleDeleted { guild_id, role_id }) => {
                    self.get_guild_mut(guild_id).roles.remove(&role_id);
                }
                ChatEvent::RoleUpdated(RoleUpdated {
                    guild_id,
                    role_id,
                    new_color,
                    new_hoist,
                    new_name,
                    new_pingable,
                }) => {
                    if let Some(role) = self.get_guild_mut(guild_id).roles.get_mut(&role_id) {
                        if let Some(pingable) = new_pingable {
                            role.pingable = pingable;
                        }
                        if let Some(color) = new_color {
                            role.color = color::decode_rgb(color);
                        }
                        if let Some(name) = new_name {
                            role.name = name.into();
                        }
                        if let Some(hoist) = new_hoist {
                            role.hoist = hoist;
                        }
                    }
                }
                ChatEvent::RoleMoved(RoleMoved {
                    guild_id,
                    role_id,
                    new_position,
                }) => {
                    if let Some(position) = new_position {
                        self.get_guild_mut(guild_id).update_role_order(position, role_id);
                    }
                }
                ChatEvent::UserRolesUpdated(UserRolesUpdated {
                    guild_id,
                    user_id,
                    new_role_ids,
                }) => {
                    self.get_guild_mut(guild_id).members.insert(user_id, new_role_ids);
                }
                ChatEvent::RolePermsUpdated(RolePermissionsUpdated {
                    guild_id,
                    channel_id,
                    role_id,
                    new_perms,
                }) => {
                    if let Some(channel_id) = channel_id {
                        self.get_channel_mut(guild_id, channel_id)
                            .role_perms
                            .insert(role_id, new_perms);
                    } else {
                        self.get_guild_mut(guild_id).role_perms.insert(role_id, new_perms);
                    }
                }
                ChatEvent::MessagePinned(MessagePinned {
                    guild_id,
                    channel_id,
                    message_id,
                }) => {
                    self.get_channel_mut(guild_id.unwrap_or(0), channel_id)
                        .pinned_messages
                        .insert(message_id);
                }
                ChatEvent::MessageUnpinned(MessageUnpinned {
                    guild_id,
                    channel_id,
                    message_id,
                }) => {
                    self.get_channel_mut(guild_id.unwrap_or(0), channel_id)
                        .pinned_messages
                        .remove(&message_id);
                }
                ChatEvent::PrivateChannelAddedToList(PrivateChannelAddedToList { channel_id, server_id }) => {
                    let c = self.get_priv_channel_mut(channel_id);
                    c.get_priv_mut().server_id = server_id;

                    if c.get_priv().fetched.not() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchPrivateChannel(channel_id));
                    }
                }
                ChatEvent::PrivateChannelRemovedFromList(PrivateChannelRemovedFromList {
                    channel_id,
                    server_id: _,
                }) => {
                    self.channels.remove(&(0, channel_id));
                    let g = self.get_guild_mut(0);
                    if let Some(index) = g.channels.iter().position(|id| channel_id.eq(id)) {
                        g.channels.remove(index);
                    }
                }
                ChatEvent::PrivateChannelDeleted(PrivateChannelDeleted { channel_id }) => {
                    self.channels.remove(&(0, channel_id));
                    let g = self.get_guild_mut(0);
                    if let Some(index) = g.channels.iter().position(|id| channel_id.eq(id)) {
                        g.channels.remove(index);
                    }
                }
                ChatEvent::UserLeftPrivateChannel(UserLeftPrivateChannel { channel_id, user_id }) => {
                    self.get_priv_channel_mut(channel_id)
                        .get_priv_mut()
                        .members
                        .remove(&user_id);
                }
                ChatEvent::UserJoinedPrivateChannel(UserJoinedPrivateChannel { channel_id, user_id }) => {
                    self.get_priv_channel_mut(channel_id)
                        .get_priv_mut()
                        .members
                        .insert(user_id);

                    if !self.users.contains_key(&user_id) {
                        let _ = self.post_sender.send(PostProcessEvent::FetchProfile(user_id));
                    }
                }
                ev => tracing::error!("event not implemented: {:?}", ev),
            },
            Event::Profile(ev) => match ev {
                ProfileEvent::StatusUpdated(StatusUpdated { user_id, new_status }) => {
                    let mut user = self.get_user_mut(user_id);
                    if let Some(new_status) = new_status {
                        user.status = new_status;
                    }
                }
                ProfileEvent::ProfileUpdated(ProfileUpdated {
                    user_id,
                    new_username,
                    new_avatar,
                    ..
                }) => {
                    if let Some(id) = new_avatar.clone() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(Attachment {
                            mimetype: "image".into(),
                            name: "avatar".into(),
                            id,
                            ..Default::default()
                        }));
                    }
                    let mut user = self.get_user_mut(user_id);
                    if let Some(new_username) = new_username {
                        user.username = new_username.into();
                    }
                    if let Some(new_avatar) = new_avatar {
                        user.avatar_url = Some(new_avatar);
                    }
                    user.fetched = true;
                }
            },
            Event::Emote(ev) => match ev {
                EmoteEvent::EmotePackUpdated(EmotePackUpdated { pack_id, new_pack_name }) => {
                    if let Some(pack_name) = new_pack_name {
                        self.get_emote_pack_mut(pack_id).pack_name = pack_name.into();
                    }
                }
                EmoteEvent::EmotePackEmotesUpdated(EmotePackEmotesUpdated {
                    pack_id,
                    added_emotes,
                    deleted_emotes,
                }) => {
                    let evs = added_emotes.iter().map(|emote| {
                        PostProcessEvent::FetchThumbnail(Attachment {
                            mimetype: "image".to_string(),
                            name: "emote".to_string(),
                            id: emote.image_id.clone(),
                            ..Default::default()
                        })
                    });
                    for ev in evs {
                        let _ = self.post_sender.send(ev);
                    }

                    let pack = self.get_emote_pack_mut(pack_id);
                    pack.emotes.extend(
                        added_emotes
                            .into_iter()
                            .map(|emote| (emote.image_id.into(), emote.name.into())),
                    );
                    for image_id in deleted_emotes {
                        pack.emotes.remove(image_id.as_str());
                    }
                }
                EmoteEvent::EmotePackDeleted(EmotePackDeleted { pack_id }) => {
                    self.emote_packs.remove(&pack_id);
                }
                EmoteEvent::EmotePackAdded(EmotePackAdded { pack }) => {
                    if let Some(pack) = pack {
                        self.emote_packs.insert(
                            pack.pack_id,
                            EmotePack {
                                pack_name: pack.pack_name.into(),
                                pack_owner: pack.pack_owner,
                                emotes: Default::default(),
                            },
                        );
                        let _ = self.post_sender.send(PostProcessEvent::FetchEmotes(pack.pack_id));
                    }
                }
            },
        }
    }

    pub fn process_get_message_history_response(
        &mut self,
        guild_id: Option<u64>,
        channel_id: u64,
        message_id: Option<u64>,
        messages: Vec<(u64, HarmonyMessage)>,
        reached_top: bool,
        direction: Direction,
    ) {
        let anchor_id = message_id.map(MessageId::Ack);
        let messages = messages
            .into_iter()
            .map(|(id, msg)| (MessageId::Ack(id), Message::from(msg)))
            .collect::<Vec<_>>();

        let mut urls = Vec::new();
        messages.iter().for_each(|(_, m)| {
            m.post_process(&self.post_sender, &mut urls, guild_id, channel_id);
        });
        if urls.is_empty().not() {
            let _ = self.post_sender.send(PostProcessEvent::FetchLinkMetadata(urls));
        }

        let messages_view = {
            let channel = self.get_channel_mut(guild_id.unwrap_or(0), channel_id);
            channel.reached_top = reached_top;
            &mut channel.messages
        };
        messages_view
            .continuous_view_mut()
            .append_messages(anchor_id.as_ref(), direction, messages);
    }

    pub fn prepare_send_message(&mut self, user_id: u64, request: SendMessageRequest) -> u64 {
        let echo_id = get_random_u64();
        self.get_channel_mut(request.guild_id.unwrap_or(0), request.channel_id)
            .messages
            .continuous_view_mut()
            .insert_message(MessageId::Unack(echo_id), Message::from_request(user_id, request));
        echo_id
    }
}

#[derive(Clone)]
pub struct Client {
    inner: InnerClient,
}

impl Debug for Client {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Client")
            .field(
                "user_id",
                &format!("{:?}", self.auth_status().session().map_or(0, |s| s.user_id)),
            )
            .finish()
    }
}

impl Client {
    pub async fn new(homeserver_url: Uri, session: Option<InnerSession>) -> ClientResult<Self> {
        Ok(Self {
            inner: InnerClient::new(homeserver_url, session).await?,
        })
    }

    pub async fn read_latest_session() -> Option<Session> {
        content::get_latest_session()
    }

    pub async fn logout(self) -> ClientResult<()> {
        self.inner
            .call(UpdateStatusRequest::update_kind(user_status::Kind::OfflineUnspecified))
            .await?;
        self.remove_session().await
    }

    pub async fn remove_session(&self) -> ClientResult<()> {
        content::delete_latest_session();
        Ok(())
    }

    pub async fn save_session_to(&self) -> ClientResult<()> {
        if let AuthStatus::Complete(session) = self.inner.auth_status() {
            let homeserver = self.inner.homeserver_url().to_string();
            let session = Session {
                session_token: session.session_token.into(),
                homeserver: homeserver.into(),
                user_id: format!("{}", session.user_id).into(),
                user_name: SmolStr::new("a"),
            };
            content::put_session(session);
        }
        Ok(())
    }

    #[inline(always)]
    pub fn auth_status(&self) -> AuthStatus {
        self.inner.auth_status()
    }

    #[inline(always)]
    pub fn user_id(&self) -> u64 {
        self.inner.user_id().unwrap()
    }

    #[inline(always)]
    pub fn inner(&self) -> &InnerClient {
        &self.inner
    }

    #[inline(always)]
    pub fn inner_arc(&self) -> InnerClient {
        self.inner.clone()
    }

    pub async fn update_profile(&self, username: Option<String>, avatar: Option<String>) -> ClientResult<()> {
        self.inner
            .call(UpdateProfileRequest {
                new_user_avatar: avatar.map(Into::into),
                new_user_name: username,
            })
            .await?;
        Ok(())
    }

    pub async fn connect_socket(&self) -> ClientResult<EventsSocket> {
        let resp = self.inner.subscribe_events(false).await?;
        Ok(resp)
    }

    pub async fn ban_member(&self, guild_id: u64, user_id: u64) -> ClientResult<()> {
        self.inner.call(BanUserRequest::new(guild_id, user_id)).await?;
        Ok(())
    }

    pub async fn kick_member(&self, guild_id: u64, user_id: u64) -> ClientResult<()> {
        self.inner.call(KickUserRequest::new(guild_id, user_id)).await?;
        Ok(())
    }

    pub async fn unban_member(&self, guild_id: u64, user_id: u64) -> ClientResult<()> {
        self.inner.call(UnbanUserRequest::new(guild_id, user_id)).await?;
        Ok(())
    }

    pub async fn edit_channel(&self, guild_id: u64, channel_id: u64, new_name: impl Into<String>) -> ClientResult<()> {
        self.inner
            .call(
                UpdateChannelInformationRequest::default()
                    .with_guild_id(guild_id)
                    .with_channel_id(channel_id)
                    .with_new_name(new_name.into()),
            )
            .await?;
        Ok(())
    }

    pub async fn edit_guild(
        &self,
        guild_id: u64,
        new_name: Option<String>,
        new_picture: Option<String>,
    ) -> ClientResult<()> {
        self.inner
            .call(UpdateGuildInformationRequest {
                guild_id,
                new_name,
                new_picture,
                ..Default::default()
            })
            .await?;
        Ok(())
    }

    pub async fn create_channel(&self, guild_id: u64, name: impl Into<String>) -> ClientResult<()> {
        self.inner
            .call(
                CreateChannelRequest::default()
                    .with_guild_id(guild_id)
                    .with_channel_name(name),
            )
            .await?;
        Ok(())
    }

    pub async fn delete_channel(&self, guild_id: u64, channel_id: u64) -> ClientResult<()> {
        self.inner.call(DeleteChannelRequest::new(guild_id, channel_id)).await?;
        Ok(())
    }

    pub async fn create_invite(&self, guild_id: u64, name: impl Into<String>, uses: u32) -> ClientResult<()> {
        self.inner
            .call(CreateInviteRequest::new(guild_id, name.into(), uses))
            .await?;
        Ok(())
    }

    pub async fn delete_invite(&self, guild_id: u64, name: impl Into<String>) -> ClientResult<()> {
        self.inner.call(DeleteInviteRequest::new(guild_id, name.into())).await?;
        Ok(())
    }

    pub async fn fetch_about(&self) -> ClientResult<About> {
        let about = self.inner.about().await?;
        Ok(about)
    }

    pub async fn delete_message(&self, guild_id: u64, channel_id: u64, message_id: u64) -> ClientResult<()> {
        self.inner
            .call(DeleteMessageRequest::new(guild_id, channel_id, message_id))
            .await?;
        Ok(())
    }

    pub async fn unpin_message(&self, guild_id: u64, channel_id: u64, message_id: u64) -> ClientResult<()> {
        self.inner
            .call(UnpinMessageRequest::new(guild_id, channel_id, message_id))
            .await?;
        Ok(())
    }

    pub async fn pin_message(&self, guild_id: u64, channel_id: u64, message_id: u64) -> ClientResult<()> {
        self.inner
            .call(PinMessageRequest::new(guild_id, channel_id, message_id))
            .await?;
        Ok(())
    }

    pub async fn leave_guild(&self, guild_id: u64) -> ClientResult<()> {
        self.inner.call(LeaveGuildRequest::new(guild_id)).await?;
        Ok(())
    }

    pub async fn join_guild(&self, invite_id: String) -> ClientResult<()> {
        self.inner.call(JoinGuildRequest::new(invite_id)).await?;
        Ok(())
    }

    pub async fn create_guild(&self, name: String) -> ClientResult<()> {
        self.inner.call(CreateGuildRequest::default().with_name(name)).await?;
        Ok(())
    }

    pub async fn send_typing(&self, guild_id: Option<u64>, channel_id: u64) -> ClientResult<()> {
        self.inner.call(TypingRequest { guild_id, channel_id }).await?;
        Ok(())
    }

    pub async fn edit_message(
        &self,
        guild_id: Option<u64>,
        channel_id: u64,
        message_id: u64,
        new_content: String,
    ) -> ClientResult<()> {
        self.inner
            .call(UpdateMessageContentRequest {
                guild_id,
                channel_id,
                message_id,
                new_content: Some(send_message_request::Content {
                    text: new_content,
                    ..Default::default()
                }),
            })
            .await?;
        Ok(())
    }

    pub async fn send_message(&self, request: SendMessageRequest, event_sender: &EventSender) -> ClientResult<u64> {
        let echo_id = request.echo_id();
        let guild_id = request.guild_id;
        let channel_id = request.channel_id;

        let res = self.inner.call(request).await;

        let resp = match res {
            Ok(resp) => resp,
            Err(err) => {
                let _ = event_sender.send(FetchEvent::FailedToSendMessage {
                    echo_id,
                    channel_id,
                    guild_id,
                });
                return Err(err.into());
            }
        };

        let message_id = resp.message_id;

        Ok(message_id)
    }

    pub async fn fetch_invites(&self, guild_id: u64, event_sender: &EventSender) -> ClientResult<()> {
        let invites = self.inner.call(GetGuildInvitesRequest::new(guild_id)).await?;
        let evs = invites
            .invites
            .into_iter()
            .filter_map(|invite| {
                Some(FetchEvent::AddInvite {
                    guild_id,
                    id: invite.invite_id,
                    invite: invite.invite?,
                })
            })
            .chain(std::iter::once(FetchEvent::FetchedInvites(guild_id)));
        for ev in evs {
            let _ = event_sender.send(ev);
        }
        Ok(())
    }

    pub async fn fetch_pinned_messages(
        &self,
        guild_id: u64,
        channel_id: u64,
        event_sender: &EventSender,
    ) -> ClientResult<()> {
        let resp = self
            .inner
            .call(GetPinnedMessagesRequest::new(guild_id, channel_id))
            .await?;
        let evs = resp.pinned_message_ids.into_iter().map(|id| {
            FetchEvent::Harmony(Event::Chat(ChatEvent::MessagePinned(MessagePinned::new(
                guild_id, channel_id, id,
            ))))
        });
        for ev in evs {
            let _ = event_sender.send(ev);
        }

        Ok(())
    }

    pub async fn upload_file(&self, name: String, mimetype: String, data: Vec<u8>) -> ClientResult<String> {
        let id = self.inner.upload_extract_id(name, mimetype, data).await?;
        Ok(id)
    }

    pub async fn fetch_attachment(&self, id: String) -> ClientResult<DownloadedFile> {
        let resp = self.inner.download_extract_file(id).await?;
        Ok(resp)
    }

    pub async fn fetch_messages(
        &self,
        guild_id: Option<u64>,
        channel_id: u64,
        anchor: Option<u64>,
        direction: Option<Direction>,
        event_sender: &EventSender,
    ) -> ClientResult<()> {
        let resp = self
            .inner
            .call(GetChannelMessagesRequest {
                guild_id,
                channel_id,
                direction: direction.map(Into::into),
                message_id: anchor,
                ..Default::default()
            })
            .await?;
        let messages = resp
            .messages
            .into_iter()
            .rev()
            .filter_map(move |message| {
                let message_id = message.message_id;
                Some((message_id, message.message?))
            })
            .collect();
        let _ = event_sender.send(FetchEvent::FetchedMessageHistory {
            guild_id,
            channel_id,
            anchor,
            direction: direction.unwrap_or_default(),
            messages,
            reached_top: resp.reached_top,
        });

        Ok(())
    }

    pub async fn set_role_perms(
        &self,
        guild_id: u64,
        channel_id: Option<u64>,
        role_id: u64,
        perms_to_give: Vec<Permission>,
    ) -> ClientResult<()> {
        self.inner
            .call(SetPermissionsRequest {
                guild_id,
                channel_id,
                role_id,
                perms_to_give,
            })
            .await?;
        Ok(())
    }

    pub async fn fetch_role_perms(
        &self,
        guild_id: u64,
        channel_ids: Vec<u64>,
        role_id: u64,
        event_sender: &EventSender,
    ) -> ClientResult<()> {
        let resp = self
            .inner
            .call(GetPermissionsRequest {
                guild_id,
                channel_ids,
                role_id,
            })
            .await?;
        let send = |channel_id, perms| {
            let _ = event_sender.send(FetchEvent::Harmony(Event::Chat(ChatEvent::RolePermsUpdated(
                RolePermissionsUpdated {
                    guild_id,
                    channel_id,
                    new_perms: perms,
                    role_id,
                },
            ))));
        };
        if let Some(perms) = resp.guild_perms {
            send(None, perms.perms);
        }
        for (channel_id, perms) in resp.channel_perms {
            send(Some(channel_id), perms.perms);
        }
        Ok(())
    }

    pub async fn fetch_private_channels(&self, event_sender: &EventSender) -> ClientResult<()> {
        let channels = self.inner.call(GetPrivateChannelListRequest::new()).await?.channels;

        let channel_ids = channels.iter().map(|c| c.channel_id).collect::<Vec<_>>();
        let resp = self.inner.call(GetPrivateChannelRequest::new(channel_ids)).await?;
        for (id, data) in resp.channels {
            let _ = event_sender.send(FetchEvent::FetchedPrivateChannel {
                id,
                is_dm: data.is_dm,
                members: data.members,
                name: data.name,
            });
        }

        for entry in channels {
            let _ = event_sender.send(FetchEvent::new_chat(ChatEvent::PrivateChannelAddedToList(
                PrivateChannelAddedToList {
                    channel_id: entry.channel_id,
                    server_id: entry.server_id,
                },
            )));
        }

        Ok(())
    }

    pub async fn fetch_channels(&self, guild_id: u64, event_sender: &EventSender) -> ClientResult<()> {
        let resp = self.inner.call(GetGuildChannelsRequest::new(guild_id)).await?;
        let evs = resp.channels.into_iter().filter_map(move |channel| {
            let channel_id = channel.channel_id;
            let channel = channel.channel?;
            Some(FetchEvent::new_chat(ChatEvent::CreatedChannel(ChannelCreated {
                guild_id,
                channel_id,
                name: channel.channel_name,
                kind: channel.kind,
                position: None,
                metadata: channel.metadata,
            })))
        });
        for ev in evs {
            let _ = event_sender.send(ev);
        }

        Ok(())
    }

    pub async fn fetch_members(&self, guild_id: u64, event_sender: &EventSender) -> ClientResult<()> {
        let resp = self.inner.call(GetGuildRolesRequest::new(guild_id)).await?;
        let evs = resp.roles.into_iter().filter_map(|role| {
            let role_id = role.role_id;
            let role = role.role?;
            Some(FetchEvent::new_chat(ChatEvent::RoleCreated(RoleCreated {
                role_id,
                guild_id,
                name: role.name,
                color: role.color,
                pingable: role.pingable,
                hoist: role.hoist,
            })))
        });
        for ev in evs {
            let _ = event_sender.send(ev);
        }
        let members = self.inner.call(GetGuildMembersRequest::new(guild_id)).await?.members;
        let resp_user_roles = self
            .inner
            .call(GetUserRolesRequest {
                guild_id,
                user_ids: members.clone(),
            })
            .await?;
        for (user_id, roles) in resp_user_roles.user_roles {
            let _ = event_sender.send(FetchEvent::new_chat(ChatEvent::UserRolesUpdated(UserRolesUpdated {
                guild_id,
                user_id,
                new_role_ids: roles.roles,
            })));
        }
        let resp_user_profiles = self.inner.call(GetProfileRequest::new(members.clone())).await?;
        let evs = resp_user_profiles
            .profile
            .into_iter()
            .map(|(user_id, profile)| FetchEvent::FetchedProfile {
                id: user_id,
                kind: profile.account_kind(),
                name: profile.user_name,
                picture: profile.user_avatar,
                status: profile.user_status.unwrap_or_default(),
            });
        let evs =
            evs.chain(members.into_iter().map(move |id| {
                FetchEvent::Harmony(Event::Chat(ChatEvent::JoinedMember(MemberJoined::new(id, guild_id))))
            }));
        for ev in evs {
            let _ = event_sender.send(ev);
        }
        Ok(())
    }

    pub async fn initial_sync(&self, event_sender: &EventSender) -> ClientResult<()> {
        let self_id = self.user_id();
        let self_profile = self
            .inner
            .call(GetProfileRequest::new_one(self_id))
            .await?
            .profile
            .one();

        let guilds = self.inner.call(GetGuildListRequest::new()).await?.guilds;

        let resp_guild_info = self
            .inner
            .call(GetGuildRequest::new(
                guilds.iter().map(|g| g.guild_id).collect::<Vec<_>>(),
            ))
            .await?;

        for (guild_id, info) in resp_guild_info.guild {
            let _ = event_sender.send(FetchEvent::FetchedGuild {
                id: guild_id,
                name: info.name,
                owners: info.owner_ids,
                picture: info.picture,
            });
        }

        for guild in guilds {
            let _ = event_sender.send(FetchEvent::Harmony(Event::Chat(ChatEvent::GuildAddedToList(
                GuildAddedToList {
                    guild_id: guild.guild_id,
                    server_id: guild.server_id,
                },
            ))));
        }

        let resp_emote_packs = self.inner.call(GetEmotePacksRequest::new()).await?;
        for pack in resp_emote_packs.packs {
            let _ = event_sender.send(FetchEvent::Harmony(Event::Emote(EmoteEvent::EmotePackAdded(
                EmotePackAdded::new(pack),
            ))));
        }

        self.inner
            .call(UpdateStatusRequest::update_kind(user_status::Kind::Online))
            .await?;

        let _ = event_sender.send(FetchEvent::Harmony(Event::Profile(ProfileEvent::ProfileUpdated(
            ProfileUpdated {
                new_avatar: self_profile.user_avatar,
                new_username: Some(self_profile.user_name),
                user_id: self_id,
            },
        ))));
        let _ = event_sender.send(FetchEvent::Harmony(Event::Profile(ProfileEvent::StatusUpdated(
            StatusUpdated {
                user_id: self_id,
                new_status: Some(UserStatus::default().with_kind(user_status::Kind::Online)),
            },
        ))));

        let _ = event_sender.send(FetchEvent::InitialSyncComplete);

        Ok(())
    }

    pub async fn fetch_guild_perms(&self, guild_id: u64, event_sender: &EventSender) -> ClientResult<()> {
        let perm_queries = [
            "guild.manage.change-information",
            "user.manage.kick",
            "user.manage.ban",
            "user.manage.unban",
            "invites.manage.create",
            "invites.manage.delete",
            "invites.view",
            "channels.manage.move",
            "channels.manage.create",
            "channels.manage.delete",
            "roles.manage",
            "roles.get",
            "roles.user.manage",
            "roles.user.get",
            "permissions.manage.set",
            "permissions.manage.get",
            all_permissions::MESSAGES_SEND,
        ];
        let resp = self
            .inner
            .call(HasPermissionRequest {
                guild_id,
                check_for: perm_queries.into_iter().map(str::to_string).collect(),
                ..Default::default()
            })
            .await?;
        for perm in resp.perms {
            let _ = event_sender.send(FetchEvent::Harmony(Event::Chat(ChatEvent::PermissionUpdated(
                PermissionUpdated {
                    guild_id,
                    channel_id: None,
                    ok: perm.ok,
                    query: perm.matches,
                },
            ))));
        }
        Ok(())
    }

    pub async fn process_post(&self, event_sender: &EventSender, post: PostProcessEvent) -> ClientResult<()> {
        tracing::debug!("processing post event: {:?}", post);
        match post {
            PostProcessEvent::CheckPermsForChannel(guild_id, channel_id) => {
                let perm_queries = [
                    all_permissions::MESSAGES_VIEW,
                    all_permissions::CHANNELS_MANAGE_CHANGE_INFORMATION,
                    all_permissions::MESSAGES_SEND,
                    all_permissions::MESSAGES_PINS_ADD,
                    all_permissions::MESSAGES_PINS_REMOVE,
                ];
                let resp = self
                    .inner
                    .call(HasPermissionRequest {
                        guild_id,
                        check_for: perm_queries.into_iter().map(str::to_string).collect(),
                        ..Default::default()
                    })
                    .await?;
                for perm in resp.perms {
                    let _ = event_sender.send(FetchEvent::Harmony(Event::Chat(ChatEvent::PermissionUpdated(
                        PermissionUpdated {
                            guild_id,
                            channel_id: Some(channel_id),
                            ok: perm.ok,
                            query: perm.matches,
                        },
                    ))));
                }

                Ok(())
            }
            PostProcessEvent::FetchThumbnail(attachment) => {
                let resp = self.fetch_attachment(attachment.id.clone()).await?;
                tracing::debug!("fetched attachment: {} {}", attachment.id, resp.mimetype);
                let _ = event_sender.send(FetchEvent::Attachment {
                    attachment: Attachment {
                        mimetype: resp.mimetype.clone(),
                        size: resp.data.len() as u32,
                        ..attachment
                    },
                    file: resp,
                });
                Ok(())
            }
            PostProcessEvent::FetchProfile(user_id) => {
                let resp = self.inner.call(GetProfileRequest::new_one(user_id)).await?;
                let profile = resp.profile.one();
                let _ = event_sender.send(FetchEvent::FetchedProfile {
                    kind: profile.account_kind(),
                    id: user_id,
                    name: profile.user_name,
                    picture: profile.user_avatar,
                    status: profile.user_status.unwrap_or_default(),
                });

                Ok(())
            }
            PostProcessEvent::FetchPrivateChannel(channel_id) => {
                let resp = self.inner.call(GetPrivateChannelRequest::new_one(channel_id)).await?;
                let channel = resp.channels.one();
                let _ = event_sender.send(FetchEvent::FetchedPrivateChannel {
                    id: channel_id,
                    name: channel.name,
                    members: channel.members,
                    is_dm: channel.is_dm,
                });
                Ok(())
            }
            PostProcessEvent::FetchGuildData(guild_id) => {
                let resp = self.inner.call(GetGuildRequest::new_one(guild_id)).await?;
                let guild = resp.guild.one();
                let _ = event_sender.send(FetchEvent::Harmony(Event::Chat(ChatEvent::EditedGuild(GuildUpdated {
                    guild_id,
                    new_metadata: guild.metadata,
                    new_name: Some(guild.name),
                    new_picture: guild.picture,
                }))));
                tracing::debug!("fetched guild data: {}", guild_id);
                Ok(())
            }
            PostProcessEvent::FetchMessage {
                guild_id,
                channel_id,
                message_id,
            } => {
                let message = self
                    .inner
                    .call(GetMessageRequest {
                        guild_id,
                        channel_id,
                        message_id,
                    })
                    .await?
                    .message;
                if let Some(message) = message {
                    let _ = event_sender.send(FetchEvent::FetchedReply {
                        guild_id,
                        channel_id,
                        message_id,
                        message,
                    });
                }
                Ok(())
            }
            PostProcessEvent::FetchLinkMetadata(urls) => {
                let resp = self
                    .inner
                    .call(FetchLinkMetadataRequest::new(
                        urls.iter().map(Uri::to_string).collect::<Vec<_>>(),
                    ))
                    .await?;
                for (url, data) in resp
                    .metadata
                    .into_iter()
                    .filter_map(|(k, data)| data.data.map(|data| (k, data)))
                {
                    let _ = event_sender.send(FetchEvent::LinkMetadata {
                        data,
                        url: url.parse().unwrap(),
                    });
                }
                Ok(())
            }
            PostProcessEvent::FetchEmotes(pack_id) => {
                let _ = event_sender.send(self.inner.call(GetEmotePackEmotesRequest::new_one(pack_id)).await.map(
                    |resp| {
                        FetchEvent::Harmony(Event::Emote(EmoteEvent::EmotePackEmotesUpdated(
                            EmotePackEmotesUpdated {
                                pack_id,
                                added_emotes: resp.pack_emotes.one().emotes,
                                deleted_emotes: Vec::new(),
                            },
                        )))
                    },
                )?);
                Ok(())
            }
        }
    }
}

fn post_heading(post: &PostEventSender, embeds: &[Embed]) {
    for embed in embeds {
        let inner = |h: Option<&embed::Heading>| {
            if let Some(id) = h.and_then(|h| h.icon.clone()) {
                let _ = post.send(PostProcessEvent::FetchThumbnail(Attachment {
                    mimetype: "image".into(),
                    id,
                    ..Default::default()
                }));
            }
        };
        inner(embed.header.as_ref());
        inner(embed.footer.as_ref());
    }
}

pub fn get_random_u64() -> u64 {
    let mut bytes = [0; 8];
    getrandom::getrandom(&mut bytes).expect("cant get random");
    u64::from_ne_bytes(bytes)
}

pub fn has_perm(guild: &Guild, channel: &Channel, query: &str) -> bool {
    channel.has_perm(query) || guild.has_perm(query)
}

pub trait HashMapExt<T> {
    fn one(self) -> T;
}

impl<K, V> HashMapExt<V> for HashMap<K, V> {
    fn one(self) -> V {
        self.into_values().next().expect("expected at least one item")
    }
}

pub trait U64Ext {
    fn if_not_zero(self) -> Option<u64>;
}

impl U64Ext for u64 {
    fn if_not_zero(self) -> Option<u64> {
        self.eq(&0).then(|| self)
    }
}
