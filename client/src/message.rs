use ahash::AHashMap;
use chrono::NaiveDateTime;
use harmony_rust_sdk::api::{
    chat::{
        get_channel_messages_request::Direction, send_message_request, Attachment, Content, Message as HarmonyMessage,
        Overrides, SendMessageRequest,
    },
    exports::hrpc::exports::http::Uri,
};
use instant::Duration;
use std::{ops::Not, ptr::NonNull};

use crate::{content::MAX_THUMB_SIZE, IndexMap, PostEventSender};

use super::{post_heading, PostProcessEvent};

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

pub trait ReadMessagesView {
    fn get_message(&self, id: &MessageId) -> Option<&Message>;
    fn contains_message(&self, id: &MessageId) -> bool {
        self.get_message(id).is_some()
    }
    fn all_messages(&self) -> Vec<(&MessageId, &Message)>;
    fn get_messages(&self, from: &MessageId, to: &MessageId) -> Vec<(&MessageId, &Message)>;
    fn is_empty(&self) -> bool;
}

pub trait WriteMessagesView {
    fn get_message_mut(&mut self, id: &MessageId) -> Option<&mut Message>;
    fn append_messages(
        &mut self,
        anchor: Option<&MessageId>,
        direction: Direction,
        messages: Vec<(MessageId, Message)>,
    );
    fn insert_message(&mut self, id: MessageId, message: Message);
    fn remove_message(&mut self, id: &MessageId) -> Option<Message>;
    // Acknowledges a message.
    //
    // Returns the old unacknowledged message with it's ID (`unack_id`) if it was acknowledged.
    // Otherwise returns the message with the `ack_id`
    fn ack_message(&mut self, unack_id: MessageId, ack_id: MessageId, message: Message) -> (MessageId, Message);
}

type MessagesMap = IndexMap<MessageId, Message>;

impl ReadMessagesView for MessagesMap {
    #[inline(always)]
    fn get_message(&self, id: &MessageId) -> Option<&Message> {
        self.get(id)
    }

    #[inline(always)]
    fn all_messages(&self) -> Vec<(&MessageId, &Message)> {
        self.iter().collect()
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn get_messages(&self, from: &MessageId, to: &MessageId) -> Vec<(&MessageId, &Message)> {
        (self.contains_key(from) && self.contains_key(to))
            .then(|| {
                let from = *from;
                let to = *to;

                self.iter()
                    .skip_while(|(&id, _)| id.ne(&from))
                    .take_while(|(&id, _)| id.ne(&to))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl WriteMessagesView for MessagesMap {
    #[inline(always)]
    fn get_message_mut(&mut self, id: &MessageId) -> Option<&mut Message> {
        self.get_mut(id)
    }

    fn append_messages(
        &mut self,
        anchor: Option<&MessageId>,
        direction: Direction,
        mut messages: Vec<(MessageId, Message)>,
    ) {
        let append_after_before = |this: &mut MessagesMap, messages: Vec<(MessageId, Message)>, anchor_index: usize| {
            let after_messages = this.drain(anchor_index..).collect::<Vec<_>>();
            this.extend(messages);
            this.extend(after_messages);
        };
        let get_anchor_index = |this: &MessagesMap| anchor.and_then(|id| this.get_index_of(id));

        match direction {
            Direction::BeforeUnspecified => {
                let anchor_index = get_anchor_index(self).unwrap_or(0);
                append_after_before(self, messages, anchor_index);
            }
            Direction::Around => {
                let messages_len = messages.len();
                let after = messages.split_off(messages_len / 2);
                let before = messages;

                let anchor_index = get_anchor_index(self).expect("must have anchor for around");

                let before_len = before.len();
                append_after_before(self, before, anchor_index);
                append_after_before(self, after, anchor_index + before_len);
            }
            Direction::After => {
                let anchor_index = get_anchor_index(self).unwrap_or_else(|| self.len().saturating_sub(1));
                append_after_before(self, messages, anchor_index);
            }
        }
    }

    #[inline(always)]
    fn insert_message(&mut self, id: MessageId, message: Message) {
        self.insert(id, message);
    }

    #[inline(always)]
    fn remove_message(&mut self, id: &MessageId) -> Option<Message> {
        self.remove(id)
    }

    fn ack_message(&mut self, unack_id: MessageId, ack_id: MessageId, message: Message) -> (MessageId, Message) {
        self.insert(ack_id, message);
        self.swap_remove(&unack_id).map_or_else(
            || self.pop().expect("must be inserted msg"),
            |message| (unack_id, message),
        )
    }
}

struct CombinedMessagesView {
    msgs: NonNull<MessagesMap>,
    reply_views: NonNull<AHashMap<MessageId, MessagesMap>>,
}

impl ReadMessagesView for CombinedMessagesView {
    fn is_empty(&self) -> bool {
        unsafe { self.msgs.as_ref().is_empty() && self.reply_views.as_ref().values().all(IndexMap::is_empty) }
    }

    fn get_message(&self, id: &MessageId) -> Option<&Message> {
        // safety: should be guaranteed by `view_internal` usage
        unsafe {
            self.msgs.as_ref().get_message(id).or_else(|| {
                self.reply_views
                    .as_ref()
                    .values()
                    .find_map(|chunk| chunk.get_message(id))
            })
        }
    }

    fn all_messages(&self) -> Vec<(&MessageId, &Message)> {
        unsafe {
            let mut messages = Vec::new();
            for view in self.reply_views.as_ref().values() {
                messages.append(&mut view.all_messages());
            }
            messages.append(&mut self.msgs.as_ref().all_messages());
            messages
        }
    }

    fn get_messages(&self, from: &MessageId, to: &MessageId) -> Vec<(&MessageId, &Message)> {
        unsafe {
            let messages = self.msgs.as_ref().get_messages(from, to);
            messages
                .is_empty()
                .then(|| {
                    for view in self.reply_views.as_ref().values() {
                        let messages = view.get_messages(from, to);
                        if messages.is_empty().not() {
                            return messages;
                        }
                    }
                    Vec::new()
                })
                .unwrap_or(messages)
        }
    }
}

impl WriteMessagesView for CombinedMessagesView {
    fn get_message_mut(&mut self, id: &MessageId) -> Option<&mut Message> {
        unsafe {
            self.msgs.as_mut().get_message_mut(id).or_else(|| {
                self.reply_views
                    .as_mut()
                    .values_mut()
                    .find_map(|chunk| chunk.get_message_mut(id))
            })
        }
    }

    #[inline(always)]
    fn append_messages(
        &mut self,
        anchor: Option<&MessageId>,
        direction: Direction,
        messages: Vec<(MessageId, Message)>,
    ) {
        unsafe { self.msgs.as_mut().append_messages(anchor, direction, messages) }
    }

    #[inline(always)]
    fn insert_message(&mut self, id: MessageId, message: Message) {
        unsafe { self.msgs.as_mut().insert_message(id, message) }
    }

    fn remove_message(&mut self, id: &MessageId) -> Option<Message> {
        unsafe {
            self.msgs.as_mut().remove_message(id).or_else(|| {
                self.reply_views
                    .as_mut()
                    .values_mut()
                    .find_map(|chunk| chunk.remove_message(id))
            })
        }
    }

    fn ack_message(&mut self, unack_id: MessageId, ack_id: MessageId, message: Message) -> (MessageId, Message) {
        unsafe {
            let (id, msg) = self.msgs.as_mut().ack_message(unack_id, ack_id, message);
            if id.is_ack() {
                let mut id_msg = Some((id, msg));
                for view in self.reply_views.as_mut().values_mut() {
                    let (_, msg) = id_msg.take().expect("must have something");
                    let (id, msg) = view.ack_message(unack_id, ack_id, msg);

                    if id.is_ack().not() {
                        break;
                    }

                    id_msg.replace((id, msg));
                }
                id_msg.take().expect("must have something")
            } else {
                (id, msg)
            }
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Messages {
    msgs: Box<MessagesMap>,
    reply_views: Box<AHashMap<MessageId, MessagesMap>>,
}

impl Messages {
    // safety: guarantee that the returned view will only live as long as self
    // safety: make sure that returned view will only be used mutably only
    // when self is borrowed mutably
    unsafe fn view_internal(&self) -> impl WriteMessagesView + ReadMessagesView + '_ {
        CombinedMessagesView {
            msgs: NonNull::new_unchecked((self.msgs.as_ref() as *const MessagesMap) as *mut MessagesMap),
            reply_views: NonNull::new_unchecked(
                (self.reply_views.as_ref() as *const AHashMap<MessageId, MessagesMap>)
                    as *mut AHashMap<MessageId, MessagesMap>,
            ),
        }
    }

    #[inline(always)]
    pub fn view(&self) -> impl ReadMessagesView + '_ {
        unsafe { self.view_internal() }
    }

    #[inline(always)]
    pub fn view_mut(&mut self) -> impl WriteMessagesView + ReadMessagesView + '_ {
        unsafe { self.view_internal() }
    }

    #[inline(always)]
    pub fn continuous_view(&self) -> &impl ReadMessagesView {
        self.msgs.as_ref()
    }

    #[inline(always)]
    /// Panics if view doesn't exist.
    pub fn reply_view(&self, anchor_id: &MessageId) -> &impl ReadMessagesView {
        self.reply_views.get(anchor_id).expect("view for anchor doesn't exist")
    }

    #[inline(always)]
    pub fn continuous_view_mut(&mut self) -> &mut (impl WriteMessagesView + ReadMessagesView) {
        self.msgs.as_mut()
    }

    /// Panics if view doesn't exist.
    #[inline(always)]
    pub fn reply_view_mut(&mut self, anchor_id: &MessageId) -> &mut (impl WriteMessagesView + ReadMessagesView) {
        self.reply_views
            .get_mut(anchor_id)
            .expect("view for anchor doesn't exist")
    }

    /// Creates a new reply view and returns a view to it.
    ///
    /// Does not create a new view if the anchor already exists.
    pub fn create_reply_view(&mut self, anchor_id: MessageId) -> &mut (impl WriteMessagesView + ReadMessagesView) {
        self.reply_views.entry(anchor_id).or_default()
    }
}

pub trait AttachmentExt {
    fn is_thumbnail(&self) -> bool;
    fn is_raster_image(&self) -> bool;
}

impl AttachmentExt for Attachment {
    fn is_thumbnail(&self) -> bool {
        self.is_raster_image() && (self.size as u64) < MAX_THUMB_SIZE
    }

    #[inline(always)]
    fn is_raster_image(&self) -> bool {
        is_raster_image(&self.mimetype)
    }
}

pub fn is_raster_image(mimetype: &str) -> bool {
    mimetype.starts_with("image") && mimetype.ends_with("svg+xml").not()
}

#[derive(Debug, Clone)]
pub struct Message {
    pub content: Content,
    pub sender: u64,
    pub timestamp: NaiveDateTime,
    pub overrides: Option<Overrides>,
    pub reply_to: Option<u64>,
    pub failed_to_send: bool,
}

impl Message {
    pub fn from_request(user_id: u64, request: SendMessageRequest) -> Self {
        Message {
            overrides: request.overrides,
            sender: user_id,
            reply_to: request.in_reply_to,
            content: request.content.map(send_content_to_content).unwrap_or_default(),
            ..Default::default()
        }
    }

    pub fn post_process(&self, post: &PostEventSender, urls: &mut Vec<Uri>, guild_id: u64, channel_id: u64) {
        if let Some(message_id) = self.reply_to.filter(|id| id != &0) {
            let _ = post.send(PostProcessEvent::FetchMessage {
                guild_id,
                channel_id,
                message_id,
            });
        }
        if let Some(id) = self.overrides.as_ref().and_then(|ov| ov.avatar.clone()) {
            let _ = post.send(PostProcessEvent::FetchThumbnail(Attachment {
                mimetype: "image".into(),
                name: "avatar".into(),
                id,
                ..Default::default()
            }));
        }
        for attachment in &self.content.attachments {
            if attachment.is_thumbnail() {
                let _ = post.send(PostProcessEvent::FetchThumbnail(attachment.clone()));
            }
        }
        post_heading(post, &self.content.embeds);
        let urlss = self
            .content
            .text
            .split_whitespace()
            .flat_map(|a| a.trim_end_matches('>').trim_start_matches('<').parse::<Uri>())
            .filter(|url| matches!(url.scheme_str(), Some("http" | "https")));
        urls.extend(urlss);
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            content: Default::default(),
            sender: Default::default(),
            timestamp: {
                let timestamp = Duration::from_millis(instant::now() as u64);
                NaiveDateTime::from_timestamp(timestamp.as_secs() as i64, timestamp.subsec_nanos())
            },
            overrides: None,
            reply_to: None,
            failed_to_send: false,
        }
    }
}

impl From<HarmonyMessage> for Message {
    fn from(message: HarmonyMessage) -> Self {
        Message {
            reply_to: message.in_reply_to,
            content: message.content.unwrap_or_default(),
            sender: message.author_id,
            timestamp: {
                NaiveDateTime::from_timestamp(
                    (message.created_at / 1000) as i64,
                    ((message.created_at % 1000) as u32) * 1000000,
                )
            },
            overrides: message.overrides,
            failed_to_send: false,
        }
    }
}

fn send_content_to_content(from: send_message_request::Content) -> Content {
    Content {
        text: from.text,
        text_formats: from.text_formats,
        embeds: from.embeds,
        attachments: from
            .attachments
            .into_iter()
            .map(send_attachment_to_attachment)
            .collect(),
        extra: None,
    }
}

fn send_attachment_to_attachment(from: send_message_request::Attachment) -> Attachment {
    Attachment {
        id: from.id,
        name: from.name,
        mimetype: "application/octet-stream".to_string(),
        size: 0,
        // TODO: also convert this
        info: None,
    }
}
