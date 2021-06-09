use bool_ext::BoolExt;
use chrono::NaiveDateTime;
use harmony_rust_sdk::{
    api::harmonytypes::{
        self, content, r#override::Reason, ContentEmbed, ContentFiles, ContentText, Message as HarmonyMessage,
    },
    client::api::rest::FileId,
};
use rand::Rng;
use smol_str::SmolStr;
use std::{str::FromStr, time::UNIX_EPOCH};

use crate::IndexMap;

use super::{content::MAX_THUMB_SIZE, post_heading, PostProcessEvent};

pub type Messages = IndexMap<MessageId, Message>;

#[derive(Debug, Clone)]
pub struct EmbedField {
    pub title: String,
    pub subtitle: String,
    pub body: String,
}

impl From<EmbedField> for harmonytypes::EmbedField {
    fn from(f: EmbedField) -> harmonytypes::EmbedField {
        harmonytypes::EmbedField {
            title: f.title,
            subtitle: f.subtitle,
            body: f.body,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmbedHeading {
    pub url: Option<SmolStr>,
    pub icon: Option<FileId>,
    pub text: String,
    pub subtext: String,
}

impl From<EmbedHeading> for harmonytypes::EmbedHeading {
    fn from(h: EmbedHeading) -> harmonytypes::EmbedHeading {
        harmonytypes::EmbedHeading {
            icon: h.icon.map_or_else(String::default, |id| id.to_string()),
            subtext: h.subtext,
            text: h.text,
            url: h.url.map_or_else(String::default, Into::into),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Embed {
    pub title: String,
    pub body: String,
    pub color: (u8, u8, u8),
    pub footer: Option<EmbedHeading>,
    pub header: Option<EmbedHeading>,
    pub fields: Vec<EmbedField>,
}

impl From<Embed> for harmonytypes::Embed {
    fn from(e: Embed) -> harmonytypes::Embed {
        harmonytypes::Embed {
            body: e.body,
            color: {
                let mut c = (e.color.0 * 255) as i64;
                c = (c << 8) + (e.color.1 * 255) as i64;
                c = (c << 8) + (e.color.2 * 255) as i64;
                c as i64
            },
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
}

impl From<Attachment> for harmonytypes::Attachment {
    fn from(a: Attachment) -> harmonytypes::Attachment {
        harmonytypes::Attachment {
            id: a.id.to_string(),
            name: a.name,
            size: a.size as i32,
            r#type: a.kind,
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
        }
    }

    pub fn is_thumbnail(&self) -> bool {
        matches!(self.kind.split('/').next(), Some("image")) && (self.size as u64) < MAX_THUMB_SIZE
    }

    pub fn from_harmony_attachment(attachment: harmonytypes::Attachment) -> Option<Self> {
        Some(Attachment {
            id: FileId::from_str(&attachment.id).ok()?,
            kind: attachment.r#type,
            name: attachment.name,
            size: attachment.size as u32,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct Override {
    pub name: String,
    pub avatar_url: Option<FileId>,
    pub reason: Option<Reason>,
}

impl From<Override> for harmonytypes::Override {
    fn from(o: Override) -> Self {
        Self {
            avatar: o.avatar_url.map_or_else(String::default, |id| id.to_string()),
            name: o.name,
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
    Embeds(Box<Embed>),
}

impl From<Content> for content::Content {
    fn from(c: Content) -> content::Content {
        match c {
            Content::Text(content) => content::Content::TextMessage(ContentText { content }),
            Content::Embeds(embeds) => content::Content::EmbedMessage(ContentEmbed {
                embeds: Some(Box::new((*embeds).into())),
            }),
            Content::Files(attachments) => content::Content::FilesMessage(ContentFiles {
                attachments: attachments.into_iter().map(Into::into).collect(),
            }),
        }
    }
}

impl From<content::Content> for Content {
    fn from(content: content::Content) -> Self {
        match content {
            content::Content::TextMessage(text) => Self::Text(text.content),
            content::Content::FilesMessage(files) => Self::Files(
                files
                    .attachments
                    .into_iter()
                    .flat_map(Attachment::from_harmony_attachment)
                    .collect(),
            ),
            content::Content::EmbedMessage(embeds) => {
                Self::Embeds(Box::new((*embeds.embeds.unwrap_or_default()).into()))
            }
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
    pub id: MessageId,
    pub content: Content,
    pub sender: u64,
    pub timestamp: NaiveDateTime,
    pub overrides: Option<Override>,
    pub being_edited: Option<String>,
}

impl Message {
    pub fn post_process(&self, post: &mut Vec<PostProcessEvent>) {
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
            _ => {}
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            id: Default::default(),
            content: Default::default(),
            sender: Default::default(),
            timestamp: {
                let timestamp = UNIX_EPOCH.elapsed().unwrap();
                NaiveDateTime::from_timestamp(timestamp.as_secs() as i64, timestamp.subsec_nanos())
            },
            overrides: None,
            being_edited: None,
        }
    }
}

pub(crate) fn harmony_messages_to_ui_messages(
    messages: IndexMap<MessageId, HarmonyMessage>,
) -> IndexMap<MessageId, Message> {
    messages.into_iter().map(|(id, msg)| (id, msg.into())).rev().collect()
}

impl From<harmonytypes::Override> for Override {
    fn from(overrides: harmonytypes::Override) -> Self {
        Override {
            name: overrides.name,
            avatar_url: FileId::from_str(&overrides.avatar).ok(),
            reason: overrides.reason,
        }
    }
}

impl From<harmonytypes::EmbedHeading> for EmbedHeading {
    fn from(h: harmonytypes::EmbedHeading) -> Self {
        EmbedHeading {
            text: h.text,
            subtext: h.subtext,
            url: h.url.is_empty().some(h.url.into()),
            icon: FileId::from_str(&h.icon).ok(),
        }
    }
}

impl From<harmonytypes::Embed> for Embed {
    fn from(e: harmonytypes::Embed) -> Self {
        Embed {
            title: e.title,
            body: e.body,
            footer: e.footer.map(From::from),
            header: e.header.map(From::from),
            fields: e
                .fields
                .into_iter()
                .map(|f| EmbedField {
                    title: f.title,
                    subtitle: f.subtitle,
                    body: f.body,
                })
                .collect(),
            color: (
                ((e.color >> 16) & 255) as u8,
                ((e.color >> 8) & 255) as u8,
                (e.color & 255) as u8,
            ),
        }
    }
}

impl From<HarmonyMessage> for Message {
    fn from(message: HarmonyMessage) -> Self {
        Message {
            content: message
                .content
                .map(|c| c.content)
                .flatten()
                .map(|c| c.into())
                .unwrap_or_default(),
            id: MessageId::Ack(message.message_id),
            sender: message.author_id,
            timestamp: {
                let t = message
                    .created_at
                    .unwrap_or_else(|| std::time::SystemTime::now().into());
                NaiveDateTime::from_timestamp(t.seconds, t.nanos as u32)
            },
            overrides: message.overrides.map(From::from),
            being_edited: None,
        }
    }
}
