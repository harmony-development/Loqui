use chrono::NaiveDateTime;
use harmony_rust_sdk::{
    api::harmonytypes::{self, r#override::Reason, FieldPresentation, Message as HarmonyMessage},
    client::api::rest::FileId,
};
use std::{str::FromStr, time::UNIX_EPOCH};
use uuid::Uuid;

use super::content::{ContentType, MAX_THUMB_SIZE};

pub type Messages = Vec<Message>;

#[derive(Debug, Clone)]
pub struct EmbedField {
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub presentation: FieldPresentation,
}

#[derive(Debug, Clone)]
pub struct EmbedHeading {
    pub url: Option<String>,
    pub icon: Option<FileId>,
    pub text: String,
    pub subtext: String,
}

#[derive(Debug, Clone)]
pub struct Embed {
    pub title: String,
    pub body: String,
    pub color: iced::Color,
    pub footer: Option<EmbedHeading>,
    pub header: Option<EmbedHeading>,
    pub fields: Vec<EmbedField>,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub kind: ContentType,
    pub name: String,
    pub id: FileId,
    pub size: u32,
}

impl Attachment {
    pub fn new_unknown(id: FileId) -> Self {
        Self {
            id,
            kind: ContentType::Other,
            name: "unknown".to_string(),
            size: 0,
        }
    }

    pub fn is_thumbnail(&self) -> bool {
        matches!(self.kind, ContentType::Image) && (self.size as u64) < MAX_THUMB_SIZE
    }

    pub fn from_harmony_attachment(attachment: harmonytypes::Attachment) -> Option<Self> {
        Some(Attachment {
            id: FileId::from_str(&attachment.id).ok()?,
            kind: ContentType::new(&attachment.r#type),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        let transaction = Uuid::new_v4().as_u128() as u64;
        MessageId::Unack(transaction)
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: MessageId,
    pub content: String,
    pub sender: u64,
    pub timestamp: NaiveDateTime,
    pub attachments: Vec<Attachment>,
    pub overrides: Option<Override>,
    pub embeds: Vec<Embed>,
    pub being_edited: Option<String>,
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
            attachments: Default::default(),
            overrides: None,
            embeds: Default::default(),
            being_edited: None,
        }
    }
}

pub(crate) fn harmony_messages_to_ui_messages(messages: Vec<HarmonyMessage>) -> Vec<Message> {
    messages.into_iter().map(From::from).rev().collect()
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
            url: {
                if h.url.is_empty() {
                    None
                } else {
                    Some(h.url)
                }
            },
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
                    presentation: f.presentation(),
                    title: f.title,
                    subtitle: f.subtitle,
                    body: f.body,
                })
                .collect(),
            color: iced::Color::from_rgb8(
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
            embeds: message.embeds.into_iter().map(From::from).collect(),
            content: message.content,
            id: MessageId::Ack(message.message_id),
            sender: message.author_id,
            timestamp: {
                let t = message
                    .created_at
                    .unwrap_or_else(|| std::time::SystemTime::now().into());
                NaiveDateTime::from_timestamp(t.seconds, t.nanos as u32)
            },
            overrides: message.overrides.map(From::from),
            attachments: message
                .attachments
                .into_iter()
                .flat_map(Attachment::from_harmony_attachment)
                .collect(),
            being_edited: None,
        }
    }
}
