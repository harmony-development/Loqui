use chrono::NaiveDateTime;
use harmony_rust_sdk::{
    api::harmonytypes::{r#override::Reason, Message as HarmonyMessage},
    client::api::rest::FileId,
};
use std::{str::FromStr, time::UNIX_EPOCH};
use uuid::Uuid;

use super::content::ContentType;

pub type Messages = Vec<Message>;

#[derive(Debug, Clone)]
pub struct Attachment {
    pub kind: ContentType,
    pub name: String,
    pub id: FileId,
    pub size: u32,
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
        }
    }
}

pub(crate) fn harmony_messages_to_ui_messages(messages: Vec<HarmonyMessage>) -> Vec<Message> {
    messages.into_iter().map(From::from).rev().collect()
}

impl From<HarmonyMessage> for Message {
    fn from(message: HarmonyMessage) -> Self {
        Message {
            content: message.content,
            id: MessageId::Ack(message.message_id),
            sender: message.author_id,
            timestamp: {
                let t = message
                    .created_at
                    .unwrap_or_else(|| std::time::SystemTime::now().into());
                NaiveDateTime::from_timestamp(t.seconds, t.nanos as u32)
            },
            overrides: message.overrides.map(|overrides| Override {
                name: overrides.name,
                avatar_url: FileId::from_str(&overrides.avatar).ok(),
                reason: overrides.reason,
            }),
            attachments: message
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
                .collect(),
        }
    }
}
