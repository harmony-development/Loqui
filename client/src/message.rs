use chrono::NaiveDateTime;
use harmony_rust_sdk::{
    api::{
        chat::{
            self, color, content, embed, overrides::Reason, FormattedText, Message as HarmonyMessage, Minithumbnail,
        },
        exports::hrpc::exports::http::Uri,
        Hmc,
    },
    client::api::rest::FileId,
};
use linemd::{parser::Token, Parser};
use rand::Rng;
use smol_str::SmolStr;
use std::{str::FromStr, time::UNIX_EPOCH};

use crate::{HarmonyToken, IndexMap};

use super::{content::MAX_THUMB_SIZE, post_heading, PostProcessEvent};

pub type Messages = IndexMap<MessageId, Message>;

#[derive(Debug, Clone)]
pub struct EmbedField {
    pub title: String,
    pub subtitle: Option<String>,
    pub body: Option<String>,
}

impl From<EmbedField> for embed::EmbedField {
    fn from(f: EmbedField) -> embed::EmbedField {
        embed::EmbedField {
            title: f.title,
            subtitle: f.subtitle,
            body: f.body.map(|text| FormattedText::default().with_text(text)),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmbedHeading {
    pub url: Option<SmolStr>,
    pub icon: Option<FileId>,
    pub text: String,
    pub subtext: Option<String>,
}

impl From<EmbedHeading> for embed::EmbedHeading {
    fn from(h: EmbedHeading) -> embed::EmbedHeading {
        embed::EmbedHeading {
            icon: h.icon.map(|id| id.to_string()),
            subtext: h.subtext,
            text: h.text,
            url: h.url.map(Into::into),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Embed {
    pub title: String,
    pub body: Option<String>,
    pub color: Option<[u8; 3]>,
    pub footer: Option<EmbedHeading>,
    pub header: Option<EmbedHeading>,
    pub fields: Vec<EmbedField>,
}

impl From<Embed> for chat::Embed {
    fn from(e: Embed) -> chat::Embed {
        chat::Embed {
            body: e.body.map(|text| FormattedText::default().with_text(text)),
            color: e.color.map(color::encode_rgb),
            fields: e.fields.into_iter().map(Into::into).collect(),
            title: e.title,
            footer: e.footer.map(Into::into),
            header: e.header.map(Into::into),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub kind: String,
    pub name: String,
    pub id: FileId,
    pub size: u32,
    pub resolution: Option<(u32, u32)>,
    pub minithumbnail: Option<Minithumbnail>,
}

impl From<Attachment> for chat::Attachment {
    fn from(a: Attachment) -> chat::Attachment {
        chat::Attachment {
            id: a.id.to_string(),
            name: a.name,
            size: a.size,
            mimetype: a.kind,
            caption: Default::default(),
        }
    }
}

impl Attachment {
    pub fn new_unknown(id: FileId) -> Self {
        Self {
            id,
            kind: "application/octet-stream".into(),
            name: "unknown".to_string(),
            size: 0,
            resolution: None,
            minithumbnail: None,
        }
    }

    pub fn is_thumbnail(&self) -> bool {
        matches!(self.kind.split('/').next(), Some("image")) && (self.size as u64) < MAX_THUMB_SIZE
    }

    pub fn from_harmony_attachment(attachment: chat::Attachment) -> Option<Self> {
        Some(Attachment {
            id: FileId::from_str(&attachment.id).ok()?,
            kind: attachment.mimetype,
            name: attachment.name,
            size: attachment.size as u32,
            resolution: None,
            minithumbnail: None,
        })
    }

    pub fn from_harmony_photo(photo: chat::Photo) -> Option<Self> {
        Some(Attachment {
            id: FileId::Hmc(Hmc::from_str(&photo.hmc).ok()?),
            kind: "image/jpeg".into(),
            name: photo.name,
            size: photo.file_size,
            resolution: Some((photo.width, photo.height)),
            minithumbnail: photo.minithumbnail,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct Override {
    pub name: Option<String>,
    pub avatar_url: Option<FileId>,
    pub reason: Option<Reason>,
}

impl From<Override> for chat::Overrides {
    fn from(o: Override) -> Self {
        Self {
            avatar: o.avatar_url.map(|id| id.to_string()),
            username: o.name,
            reason: o.reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageId {
    Ack(u64),
    Unack(u64),
}

impl MessageId {
    pub fn is_ack(&self) -> bool {
        matches!(self, MessageId::Ack(_))
    }

    pub fn transaction_id(&self) -> Option<u64> {
        match self {
            MessageId::Unack(transaction) => Some(*transaction),
            _ => None,
        }
    }

    pub fn id(&self) -> Option<u64> {
        match self {
            MessageId::Ack(id) => Some(*id),
            _ => None,
        }
    }
}

impl Default for MessageId {
    fn default() -> Self {
        MessageId::Unack(rand::thread_rng().gen())
    }
}

#[derive(Debug, Clone)]
pub enum Content {
    Text(String),
    Files(Vec<Attachment>),
    Embeds(Vec<Embed>),
}

impl From<Content> for content::Content {
    fn from(c: Content) -> content::Content {
        match c {
            Content::Text(content) => content::Content::TextMessage(content::TextContent {
                content: Some(FormattedText::default().with_text(content)),
            }),
            Content::Embeds(embeds) => content::Content::EmbedMessage(content::EmbedContent {
                embeds: embeds.into_iter().map(Into::into).collect(),
            }),
            Content::Files(attachments) => content::Content::AttachmentMessage(content::AttachmentContent {
                files: attachments.into_iter().map(Into::into).collect(),
            }),
        }
    }
}

impl From<content::Content> for Content {
    fn from(content: content::Content) -> Self {
        match content {
            content::Content::TextMessage(text) => Self::Text(text.content.map_or_else(String::new, |f| f.text)),
            content::Content::AttachmentMessage(files) => Self::Files(
                files
                    .files
                    .into_iter()
                    .flat_map(Attachment::from_harmony_attachment)
                    .collect(),
            ),
            content::Content::EmbedMessage(embeds) => Self::Embeds(embeds.embeds.into_iter().map(Into::into).collect()),
            content::Content::PhotoMessage(photos) => Self::Files(
                photos
                    .photos
                    .into_iter()
                    .flat_map(Attachment::from_harmony_photo)
                    .collect(),
            ),
            content::Content::InviteRejected(_) => todo!(),
            content::Content::InviteAccepted(_) => todo!(),
            content::Content::RoomUpgradedToGuild(_) => todo!(),
        }
    }
}

impl Default for Content {
    fn default() -> Self {
        Content::Text(Default::default())
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub content: Content,
    pub sender: u64,
    pub timestamp: NaiveDateTime,
    pub overrides: Option<Override>,
    pub being_edited: Option<String>,
    pub reply_to: Option<u64>,
}

impl Message {
    pub fn post_process(&self, post: &mut Vec<PostProcessEvent>, guild_id: u64, channel_id: u64) {
        if let Some(message_id) = self.reply_to.filter(|id| id != &0) {
            post.push(PostProcessEvent::FetchMessage {
                guild_id,
                channel_id,
                message_id,
            });
        }
        if let Some(id) = self.overrides.as_ref().and_then(|ov| ov.avatar_url.clone()) {
            post.push(PostProcessEvent::FetchThumbnail(Attachment {
                kind: "image".into(),
                name: "avatar".into(),
                ..Attachment::new_unknown(id)
            }));
        }
        match &self.content {
            Content::Files(attachments) => {
                for attachment in attachments {
                    if attachment.is_thumbnail() {
                        post.push(PostProcessEvent::FetchThumbnail(attachment.clone()));
                    }
                }
            }
            Content::Embeds(embeds) => {
                post_heading(post, embeds);
            }
            Content::Text(text) => {
                post.extend(
                    text.split_whitespace()
                        .map(|a| a.trim_end_matches('>').trim_start_matches('<').parse::<Uri>())
                        .flatten()
                        .filter(|url| matches!(url.scheme_str(), Some("http" | "https")))
                        .map(PostProcessEvent::FetchLinkMetadata),
                );
                post.extend(
                    text.as_str()
                        .parse_md_custom(HarmonyToken::parse)
                        .into_iter()
                        .flat_map(|tok| {
                            if let Token::Custom(HarmonyToken::Emote(id)) = tok {
                                Some(PostProcessEvent::FetchThumbnail(Attachment {
                                    kind: "image".into(),
                                    name: "emote".into(),
                                    ..Attachment::new_unknown(FileId::Id(id.to_string()))
                                }))
                            } else {
                                None
                            }
                        }),
                );
            }
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            content: Default::default(),
            sender: Default::default(),
            timestamp: {
                let timestamp = UNIX_EPOCH.elapsed().unwrap();
                NaiveDateTime::from_timestamp(timestamp.as_secs() as i64, timestamp.subsec_nanos())
            },
            overrides: None,
            being_edited: None,
            reply_to: None,
        }
    }
}

impl From<chat::Overrides> for Override {
    fn from(overrides: chat::Overrides) -> Self {
        Override {
            name: overrides.username,
            avatar_url: overrides.avatar.map(|a| FileId::from_str(&a).ok()).flatten(),
            reason: overrides.reason,
        }
    }
}

impl From<embed::EmbedHeading> for EmbedHeading {
    fn from(h: embed::EmbedHeading) -> Self {
        EmbedHeading {
            text: h.text,
            subtext: h.subtext,
            url: h.url.map(Into::into),
            icon: h.icon.map(|i| FileId::from_str(&i).ok()).flatten(),
        }
    }
}

impl From<chat::Embed> for Embed {
    fn from(e: chat::Embed) -> Self {
        Embed {
            title: e.title,
            body: e.body.map(|f| f.text),
            footer: e.footer.map(From::from),
            header: e.header.map(From::from),
            fields: e
                .fields
                .into_iter()
                .map(|f| EmbedField {
                    title: f.title,
                    subtitle: f.subtitle,
                    body: f.body.map(|f| f.text),
                })
                .collect(),
            color: e.color.map(color::decode_rgb),
        }
    }
}

impl From<HarmonyMessage> for Message {
    fn from(message: HarmonyMessage) -> Self {
        Message {
            reply_to: message.in_reply_to,
            content: message
                .content
                .and_then(|c| c.content)
                .map(|c| c.into())
                .unwrap_or_default(),
            sender: message.author_id,
            timestamp: { NaiveDateTime::from_timestamp(message.created_at as i64, 0) },
            overrides: message.overrides.map(From::from),
            being_edited: None,
        }
    }
}
