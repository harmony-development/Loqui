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
    client::{api::auth::Session as InnerSession, AuthStatus, Client as InnerClient},
};
use harmony_rust_sdk::{
    api::{
        chat::{
            all_permissions, color,
            get_channel_messages_request::Direction,
            stream_event::{Event as ChatEvent, *},
            BanUserRequest, ChannelKind, Content as HarmonyContent, CreateChannelRequest, CreateGuildRequest,
            CreateInviteRequest, DeleteChannelRequest, DeleteInviteRequest, DeleteMessageRequest, Event, EventSource,
            FormattedText, GetGuildChannelsRequest, GetGuildInvitesRequest, GetGuildListRequest,
            GetGuildMembersRequest, GetGuildRequest, GetGuildRolesRequest, GetMessageRequest, GetPinnedMessagesRequest,
            GetUserRolesRequest, Invite, JoinGuildRequest, KickUserRequest, LeaveGuildRequest,
            Message as HarmonyMessage, Permission, PinMessageRequest, QueryHasPermissionRequest, Role, TypingRequest,
            UnbanUserRequest, UnpinMessageRequest, UpdateMessageTextRequest,
        },
        emote::{stream_event::Event as EmoteEvent, *},
        mediaproxy::{fetch_link_metadata_response::Data as FetchLinkData, FetchLinkMetadataRequest},
        profile::{stream_event::Event as ProfileEvent, UserStatus, *},
        rest::About,
    },
    client::{
        api::{
            chat::{
                channel::{GetChannelMessages, UpdateChannelInformation},
                guild::UpdateGuildInformation,
                message::SendMessage,
            },
            profile::UpdateProfile,
            rest::{DownloadedFile, FileId},
        },
        EventsSocket,
    },
};

use error::ClientResult;
use instant::Instant;
use member::Member;
use message::{Attachment, Content, Embed, MessageId, ReadMessagesView, WriteMessagesView};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use std::{
    fmt::{self, Debug, Display, Formatter},
    ops::Not,
    str::FromStr,
};

use crate::emotes::EmotePack;

use self::message::{EmbedHeading, Message};

pub use ahash::{AHashMap, AHashSet, AHasher};
pub use smol_str;
pub use tracing;
pub use urlencoding;

pub type IndexMap<K, V> = indexmap::IndexMap<K, V, ahash::RandomState>;
pub type EventSender = tokio::sync::mpsc::UnboundedSender<FetchEvent>;
pub type EventReceiver = tokio::sync::mpsc::UnboundedReceiver<FetchEvent>;
pub type PostEventSender = tokio::sync::mpsc::UnboundedSender<PostProcessEvent>;
pub type PostEventReceiver = tokio::sync::mpsc::UnboundedReceiver<PostProcessEvent>;
pub type EventFn = Box<dyn FnMut(FetchEvent) -> Option<FetchEvent>>;

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
    FetchThumbnail(Attachment),
    CheckPermsForChannel(u64, u64),
    FetchMessage {
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
    },
    FetchLinkMetadata(Uri),
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
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
        message: HarmonyMessage,
    },
    FailedToSendMessage {
        guild_id: u64,
        channel_id: u64,
        echo_id: u64,
    },
    InitialSyncComplete,
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
    event_fn: EventFn,
}

impl Cache {
    pub fn new(event_receiver: EventReceiver, post_sender: PostEventSender, event_fn: EventFn) -> Self {
        Self {
            event_receiver,
            post_sender,
            channels: Default::default(),
            emote_packs: Default::default(),
            guilds: Default::default(),
            initial_sync_complete: false,
            link_embeds: Default::default(),
            users: Default::default(),
            event_fn,
        }
    }

    pub fn maintain(&mut self) {
        for member in self.users.values_mut() {
            if let Some((_, _, time)) = member.typing_in_channel {
                if time.elapsed().as_secs() > 5 {
                    member.typing_in_channel = None;
                }
            }
        }

        while let Ok(ev) = self.event_receiver.try_recv() {
            if let Some(ev) = (self.event_fn)(ev) {
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

    fn get_user_mut(&mut self, user_id: u64) -> &mut Member {
        self.users.entry(user_id).or_default()
    }

    fn get_emote_pack_mut(&mut self, pack_id: u64) -> &mut EmotePack {
        self.emote_packs.entry(pack_id).or_default()
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

    pub fn process_event(&mut self, event: FetchEvent) {
        match event {
            FetchEvent::FailedToSendMessage {
                guild_id,
                channel_id,
                echo_id,
            } => {
                let mut view = self.get_channel_mut(guild_id, channel_id).messages.view_mut();
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
                if let FetchLinkData::IsSite(site) = &data {
                    if let Ok(url) = site.image.parse::<Uri>() {
                        let id = FileId::External(url);
                        let _ = self
                            .post_sender
                            .send(PostProcessEvent::FetchThumbnail(Attachment::new_unknown(id)));
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
                message.post_process(&self.post_sender, guild_id, channel_id);

                let channel = self.get_channel_mut(guild_id, channel_id);

                channel.messages.create_reply_view(id).insert_message(id, message);
            }
            FetchEvent::Attachment { .. } => {}
            FetchEvent::InitialSyncComplete => {
                self.initial_sync_complete = true;
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
                ChatEvent::SentMessage(message_sent) => {
                    let MessageSent {
                        echo_id,
                        guild_id,
                        channel_id,
                        message_id,
                        message,
                    } = *message_sent;

                    if let Some(message) = message {
                        let message = Message::from(message);
                        message.post_process(&self.post_sender, guild_id, channel_id);

                        let channel = self.get_channel_mut(guild_id, channel_id);

                        let message_view = channel.messages.continuous_view_mut();
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
                    self.get_channel_mut(guild_id, channel_id)
                        .messages
                        .continuous_view_mut()
                        .remove_message(&MessageId::Ack(message_id));
                }
                ChatEvent::EditedMessage(message_updated) => {
                    let guild_id = message_updated.guild_id;
                    let channel_id = message_updated.channel_id;
                    let message_id = MessageId::Ack(message_updated.message_id);

                    if let Some(msg) = self
                        .get_channel_mut(guild_id, channel_id)
                        .messages
                        .view_mut()
                        .get_message_mut(&message_id)
                    {
                        msg.content = Content::Text(message_updated.new_content.map_or_else(String::new, |f| f.text));
                    }

                    let maybe_view = self.get_channel(guild_id, channel_id).map(|chan| chan.messages.view());
                    if let Some(msg) = maybe_view.as_ref().and_then(|view| view.get_message(&message_id)) {
                        msg.post_process(&self.post_sender, guild_id, channel_id);
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
                ChatEvent::GuildAddedToList(GuildAddedToList { guild_id, homeserver }) => {
                    let guild = self.get_guild_mut(guild_id);
                    guild.homeserver = homeserver.into();

                    if guild.fetched.not() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchGuildData(guild_id));
                    }
                }
                ChatEvent::GuildRemovedFromList(GuildRemovedFromList {
                    guild_id,
                    homeserver: _,
                }) => {
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
                    let parsed_pic = new_picture.and_then(|new_picture| FileId::from_str(&new_picture).ok());
                    if let Some(id) = parsed_pic.clone() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(Attachment {
                            kind: "image".into(),
                            name: "guild".into(),
                            ..Attachment::new_unknown(id)
                        }));
                    }
                    let mut guild = self.get_guild_mut(guild_id);

                    if let Some(name) = new_name {
                        guild.name = name.into();
                    }
                    if let Some(picture) = parsed_pic {
                        guild.picture = Some(picture);
                    }

                    guild.fetched = true;
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
                    self.get_channel_mut(guild_id, channel_id)
                        .pinned_messages
                        .insert(message_id);
                }
                ChatEvent::MessageUnpinned(MessageUnpinned {
                    guild_id,
                    channel_id,
                    message_id,
                }) => {
                    self.get_channel_mut(guild_id, channel_id)
                        .pinned_messages
                        .remove(&message_id);
                }
                ev => tracing::error!("event not implemented: {:?}", ev),
            },
            Event::Profile(ev) => match ev {
                ProfileEvent::ProfileUpdated(ProfileUpdated {
                    user_id,
                    new_username,
                    new_avatar,
                    new_status,
                    new_is_bot,
                    new_account_kind,
                }) => {
                    let parsed_avatar = new_avatar.and_then(|new_avatar| FileId::from_str(&new_avatar).ok());
                    if let Some(id) = parsed_avatar.clone() {
                        let _ = self.post_sender.send(PostProcessEvent::FetchThumbnail(Attachment {
                            kind: "image".into(),
                            name: "avatar".into(),
                            ..Attachment::new_unknown(id)
                        }));
                    }
                    let mut user = self.get_user_mut(user_id);
                    if let Some(new_username) = new_username {
                        user.username = new_username.into();
                    }
                    if let Some(new_status) = new_status {
                        user.status = UserStatus::from_i32(new_status).unwrap_or(UserStatus::OfflineUnspecified);
                    }
                    if let Some(is_bot) = new_is_bot {
                        user.is_bot = is_bot;
                    }
                    if let Some(new_avatar) = parsed_avatar {
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
                            kind: "image".to_string(),
                            name: "emote".to_string(),
                            ..Attachment::new_unknown(FileId::Id(emote.image_id.clone()))
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
        guild_id: u64,
        channel_id: u64,
        message_id: Option<u64>,
        messages: Vec<(u64, HarmonyMessage)>,
        reached_top: bool,
        direction: Direction,
        post: &PostEventSender,
    ) {
        let anchor_id = message_id.map(MessageId::Ack);
        let messages = messages
            .into_iter()
            .map(|(id, msg)| (MessageId::Ack(id), Message::from(msg)))
            .collect::<Vec<_>>();

        messages.iter().for_each(|(_, m)| {
            m.post_process(post, guild_id, channel_id);
        });

        let channel = self.get_channel_mut(guild_id, channel_id);
        channel.reached_top = reached_top;
        channel
            .messages
            .continuous_view_mut()
            .append_messages(anchor_id.as_ref(), direction, messages);
    }

    pub fn prepare_send_message(&mut self, guild_id: u64, channel_id: u64, message: Message) -> u64 {
        let echo_id = get_random_u64();
        self.get_channel_mut(guild_id, channel_id)
            .messages
            .continuous_view_mut()
            .insert_message(MessageId::Unack(echo_id), message);
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
            .call(UpdateProfile::default().with_new_status(UserStatus::OfflineUnspecified))
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
        if let AuthStatus::Complete(session) = self.inner.auth_status() {
            session.user_id
        } else {
            panic!()
        }
    }

    #[inline(always)]
    pub fn inner(&self) -> &InnerClient {
        &self.inner
    }

    #[inline(always)]
    pub fn inner_arc(&self) -> InnerClient {
        self.inner.clone()
    }

    pub async fn update_profile(
        &self,
        username: Option<String>,
        avatar: Option<FileId>,
        status: Option<UserStatus>,
    ) -> ClientResult<()> {
        self.inner
            .call(
                UpdateProfileRequest::default()
                    .with_new_user_name(username)
                    .with_new_user_avatar(avatar.map(Into::into))
                    .with_new_user_status(status.map(Into::into)),
            )
            .await?;
        Ok(())
    }

    pub async fn connect_socket(&self, guild_ids: Vec<u64>) -> ClientResult<EventsSocket> {
        let mut subs = vec![EventSource::Homeserver, EventSource::Action];
        subs.extend(guild_ids.into_iter().map(EventSource::Guild));
        let resp = self.inner.subscribe_events(subs).await?;
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
            .call(UpdateChannelInformation::new(guild_id, channel_id).with_new_name(Some(new_name.into())))
            .await?;
        Ok(())
    }

    pub async fn edit_guild(
        &self,
        guild_id: u64,
        new_name: Option<String>,
        new_picture: Option<FileId>,
    ) -> ClientResult<()> {
        self.inner
            .call(
                UpdateGuildInformation::new(guild_id)
                    .with_new_guild_name(new_name)
                    .with_new_guild_picture(new_picture),
            )
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
        let about = harmony_rust_sdk::client::api::rest::about(&self.inner).await?;
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

    pub async fn send_typing(&self, guild_id: u64, channel_id: u64) -> ClientResult<()> {
        self.inner.call(TypingRequest::new(guild_id, channel_id)).await?;
        Ok(())
    }

    pub async fn edit_message(
        &self,
        guild_id: u64,
        channel_id: u64,
        message_id: u64,
        new_content: String,
    ) -> ClientResult<()> {
        self.inner
            .call(UpdateMessageTextRequest::new(
                guild_id,
                channel_id,
                message_id,
                Some(FormattedText::new(new_content, Vec::new())),
            ))
            .await?;
        Ok(())
    }

    pub async fn send_message(
        &self,
        echo_id: u64,
        guild_id: u64,
        channel_id: u64,
        message: Message,
        event_sender: &EventSender,
    ) -> ClientResult<u64> {
        let res = self
            .inner
            .call(
                SendMessage::new(guild_id, channel_id)
                    .with_content(HarmonyContent::new(Some(message.content.into())))
                    .with_echo_id(echo_id)
                    .with_in_reply_to(message.reply_to)
                    .with_overrides(message.overrides.map(Into::into)),
            )
            .await;

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

    pub async fn upload_file(&self, name: String, mimetype: String, data: Vec<u8>) -> ClientResult<FileId> {
        let id = harmony_rust_sdk::client::api::rest::upload_extract_id(&self.inner, name, mimetype, data).await?;
        Ok(FileId::Id(id))
    }

    pub async fn fetch_attachment(&self, id: FileId) -> ClientResult<(FileId, DownloadedFile)> {
        let resp = harmony_rust_sdk::client::api::rest::download_extract_file(&self.inner, id.clone()).await?;
        Ok((id, resp))
    }

    pub async fn fetch_messages(&self, guild_id: u64, channel_id: u64, event_sender: &EventSender) -> ClientResult<()> {
        let resp = self.inner.call(GetChannelMessages::new(guild_id, channel_id)).await?;
        let evs = resp.messages.into_iter().rev().map(move |message| {
            let message_id = message.message_id;
            FetchEvent::Harmony(Event::Chat(ChatEvent::new_sent_message(Box::new(MessageSent {
                guild_id,
                channel_id,
                message_id,
                echo_id: None,
                message: message.message,
            }))))
        });
        for ev in evs {
            let _ = event_sender.send(ev);
        }

        Ok(())
    }

    pub async fn fetch_channels(&self, guild_id: u64, event_sender: &EventSender) -> ClientResult<()> {
        let resp = self.inner.call(GetGuildChannelsRequest::new(guild_id)).await?;
        let evs = resp.channels.into_iter().filter_map(move |channel| {
            let channel_id = channel.channel_id;
            let channel = channel.channel?;
            Some(FetchEvent::Harmony(Event::Chat(ChatEvent::new_created_channel(
                ChannelCreated {
                    guild_id,
                    channel_id,
                    name: channel.channel_name,
                    kind: channel.kind,
                    position: None,
                    metadata: channel.metadata,
                },
            ))))
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
            Some(FetchEvent::Harmony(Event::Chat(ChatEvent::new_role_created(
                RoleCreated {
                    role_id,
                    guild_id,
                    name: role.name,
                    color: role.color,
                    pingable: role.pingable,
                    hoist: role.hoist,
                },
            ))))
        });
        for ev in evs {
            let _ = event_sender.send(ev);
        }
        let members = self.inner.call(GetGuildMembersRequest::new(guild_id)).await?.members;
        let resp_user_roles = self
            .inner
            .batch_call(
                members
                    .iter()
                    .map(|user_id| GetUserRolesRequest::new(guild_id, *user_id))
                    .collect(),
            )
            .await?;
        let evs = resp_user_roles.into_iter().zip(members.iter()).map(|(resp, user_id)| {
            FetchEvent::Harmony(Event::Chat(ChatEvent::new_user_roles_updated(UserRolesUpdated::new(
                guild_id, *user_id, resp.roles,
            ))))
        });
        for ev in evs {
            let _ = event_sender.send(ev);
        }
        let resp_user_profiles = self
            .inner
            .batch_call(members.iter().map(|user_id| GetProfileRequest::new(*user_id)).collect())
            .await?;
        let evs = resp_user_profiles
            .into_iter()
            .zip(members.iter())
            .filter_map(|(resp, user_id)| {
                let profile = resp.profile?;
                Some(FetchEvent::Harmony(Event::Profile(ProfileEvent::new_profile_updated(
                    ProfileUpdated {
                        user_id: *user_id,
                        new_avatar: profile.user_avatar,
                        new_username: Some(profile.user_name),
                        new_status: Some(profile.user_status),
                        new_is_bot: Some(profile.is_bot),
                        new_account_kind: Some(profile.account_kind),
                    },
                ))))
            });
        let evs = evs.chain(members.iter().map(move |id| {
            FetchEvent::Harmony(Event::Chat(ChatEvent::new_joined_member(MemberJoined::new(
                *id, guild_id,
            ))))
        }));
        for ev in evs {
            let _ = event_sender.send(ev);
        }
        Ok(())
    }

    pub async fn initial_sync(&self, event_sender: &EventSender) -> ClientResult<()> {
        let self_id = if let AuthStatus::Complete(session) = self.inner.auth_status() {
            session.user_id
        } else {
            todo!("return err")
        };
        let self_profile = self
            .inner
            .call(GetProfileRequest::new(self_id))
            .await?
            .profile
            .unwrap_or_default();

        let guilds = self.inner.call(GetGuildListRequest::new()).await?.guilds;

        let mut guild_info_reqs = Vec::with_capacity(guilds.len());
        let mut guild_added_to_list_events = Vec::with_capacity(guilds.len());

        for guild in guilds {
            guild_info_reqs.push(GetGuildRequest::new(guild.guild_id));
            guild_added_to_list_events.push(FetchEvent::Harmony(Event::Chat(ChatEvent::GuildAddedToList(
                GuildAddedToList {
                    guild_id: guild.guild_id,
                    homeserver: guild.server_id,
                },
            ))));
        }

        let chunked_guild_info_reqs = guild_info_reqs.into_iter().fold(vec![Vec::new()], |mut tot, item| {
            if tot.last().unwrap().len() < 64 {
                tot.last_mut().unwrap().push(item);
            } else {
                tot.push(vec![item]);
            }
            tot
        });

        for chunk in chunked_guild_info_reqs {
            let guild_infos = self.inner.batch_call(chunk.clone()).await?;
            let evs = guild_infos.into_iter().zip(chunk.into_iter()).map(|(resp, req)| {
                let guild = resp.guild.unwrap_or_default();
                FetchEvent::Harmony(Event::Chat(ChatEvent::EditedGuild(GuildUpdated {
                    guild_id: req.guild_id,
                    new_metadata: guild.metadata,
                    new_name: Some(guild.name),
                    new_picture: guild.picture,
                })))
            });
            for ev in evs {
                let _ = event_sender.send(ev);
            }
        }

        for ev in guild_added_to_list_events {
            let _ = event_sender.send(ev);
        }

        let evs = self.inner.call(GetEmotePacksRequest::new()).await.map(|resp| {
            resp.packs.into_iter().map(|pack| {
                FetchEvent::Harmony(Event::Emote(EmoteEvent::EmotePackAdded(EmotePackAdded {
                    pack: Some(pack),
                })))
            })
        })?;
        for ev in evs {
            let _ = event_sender.send(ev);
        }

        self.inner
            .call(UpdateProfile::default().with_new_status(UserStatus::Online))
            .await?;

        let _ = event_sender.send(FetchEvent::Harmony(Event::Profile(ProfileEvent::ProfileUpdated(
            ProfileUpdated {
                new_is_bot: Some(self_profile.is_bot),
                new_avatar: self_profile.user_avatar,
                new_status: Some(UserStatus::Online.into()),
                new_username: Some(self_profile.user_name),
                user_id: self_id,
                new_account_kind: None,
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
        ];
        let queries = perm_queries
            .into_iter()
            .map(|query| QueryHasPermissionRequest::new(guild_id, None, None, query.to_string()))
            .collect();
        let evs = self.inner.batch_call(queries).await.map(move |response| {
            response
                .into_iter()
                .zip(perm_queries.into_iter())
                .map(move |(resp, query)| {
                    FetchEvent::Harmony(Event::Chat(ChatEvent::PermissionUpdated(PermissionUpdated {
                        guild_id,
                        channel_id: None,
                        ok: resp.ok,
                        query: query.to_string(),
                    })))
                })
        })?;
        for ev in evs {
            let _ = event_sender.send(ev);
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
                let queries = perm_queries
                    .into_iter()
                    .map(|query| QueryHasPermissionRequest::new(guild_id, Some(channel_id), None, query.to_string()))
                    .collect();
                let evs = self
                    .inner
                    .batch_call(queries)
                    .await?
                    .into_iter()
                    .zip(perm_queries.into_iter())
                    .map(move |(resp, query)| {
                        FetchEvent::Harmony(Event::Chat(ChatEvent::PermissionUpdated(PermissionUpdated {
                            guild_id,
                            channel_id: Some(channel_id),
                            ok: resp.ok,
                            query: query.to_string(),
                        })))
                    });
                for ev in evs {
                    let _ = event_sender.send(ev);
                }

                Ok(())
            }
            PostProcessEvent::FetchThumbnail(attachment) => {
                let (id, resp) = self.fetch_attachment(attachment.id.clone()).await?;
                let _ = event_sender.send(FetchEvent::Attachment {
                    attachment: Attachment {
                        id,
                        kind: resp.mimetype().to_string(),
                        size: resp.data().len() as u32,
                        ..attachment
                    },
                    file: resp,
                });
                Ok(())
            }
            PostProcessEvent::FetchProfile(user_id) => {
                let _ = event_sender.send(self.inner.call(GetProfileRequest::new(user_id)).await.map(|resp| {
                    let profile = resp.profile.unwrap_or_default();
                    FetchEvent::Harmony(Event::Profile(ProfileEvent::ProfileUpdated(ProfileUpdated {
                        user_id,
                        new_avatar: profile.user_avatar,
                        new_status: Some(profile.user_status),
                        new_username: Some(profile.user_name),
                        new_is_bot: Some(profile.is_bot),
                        new_account_kind: Some(profile.account_kind),
                    })))
                })?);

                Ok(())
            }
            PostProcessEvent::FetchGuildData(guild_id) => {
                let _ = event_sender.send(self.inner.call(GetGuildRequest::new(guild_id)).await.map(|resp| {
                    let guild = resp.guild.unwrap_or_default();
                    FetchEvent::Harmony(Event::Chat(ChatEvent::EditedGuild(GuildUpdated {
                        guild_id,
                        new_metadata: guild.metadata,
                        new_name: Some(guild.name),
                        new_picture: guild.picture,
                    })))
                })?);
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
                    .call(GetMessageRequest::new(guild_id, channel_id, message_id))
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
            PostProcessEvent::FetchLinkMetadata(url) => {
                let resp = self.inner.call(FetchLinkMetadataRequest::new(url.to_string())).await?;
                if let Some(data) = resp.data {
                    let _ = event_sender.send(FetchEvent::LinkMetadata { data, url });
                }
                Ok(())
            }
            PostProcessEvent::FetchEmotes(pack_id) => {
                let _ = event_sender.send(self.inner.call(GetEmotePackEmotesRequest { pack_id }).await.map(
                    |resp| {
                        FetchEvent::Harmony(Event::Emote(EmoteEvent::EmotePackEmotesUpdated(
                            EmotePackEmotesUpdated {
                                pack_id,
                                added_emotes: resp.emotes,
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
        let inner = |h: Option<&EmbedHeading>| {
            if let Some(id) = h.and_then(|h| h.icon.clone()) {
                let _ = post.send(PostProcessEvent::FetchThumbnail(Attachment {
                    kind: "image".into(),
                    ..Attachment::new_unknown(id)
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
