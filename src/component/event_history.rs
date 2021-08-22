use std::{fmt::Display, ops::Not};

use crate::{
    client::{
        channel::Channel,
        content::ContentStore,
        member::Members,
        message::{Content as IcyContent, EmbedHeading},
    },
    color,
    component::*,
    label,
    screen::{
        main::{Message, Mode},
        scale_down, truncate_string,
    },
    space,
    style::{
        tuple_to_iced_color, Theme, ALT_COLOR, AVATAR_WIDTH, DATE_SEPERATOR_SIZE, DEF_SIZE, ERROR_COLOR,
        MESSAGE_SENDER_SIZE, MESSAGE_SIZE, MESSAGE_TIMESTAMP_SIZE, PADDING, SPACING,
    },
    IOSEVKA,
};
use chrono::{Datelike, TimeZone};
use client::{
    bool_ext::BoolExt,
    guild::Guild,
    harmony_rust_sdk::api::{
        harmonytypes::{r#override::Reason, UserStatus},
        mediaproxy::fetch_link_metadata_response::Data as FetchLinkData,
    },
    linemd::{
        parser::{Text, Token},
        Parser,
    },
    message::{Attachment, MessageId},
    Client, HarmonyToken, OptionExt, Url,
};
use iced::{rule::FillMode, Font, Tooltip};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageMenuOption {
    Copy(MessageId),
    CopyMessageId(u64),
    Reply(u64),
    Edit(u64),
    Delete(u64),
}

impl Display for MessageMenuOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            MessageMenuOption::CopyMessageId(_) => "copy id",
            MessageMenuOption::Copy(_) => "copy",
            MessageMenuOption::Reply(_) => "reply",
            MessageMenuOption::Edit(_) => "edit",
            MessageMenuOption::Delete(_) => "delete",
        };
        f.write_str(text)
    }
}

pub const SHOWN_MSGS_LIMIT: usize = 32;
pub type EventHistoryButsState = [(
    Vec<button::State>,
    button::State,
    button::State,
    button::State,
    button::State,
    Vec<button::State>,
    Vec<button::State>,
    pick_list::State<MessageMenuOption>,
); SHOWN_MSGS_LIMIT];

const MSG_LR_PADDING: u16 = AVATAR_WIDTH / 4;
const RIGHT_TIMESTAMP_PADDING: u16 = MSG_LR_PADDING;
const LEFT_TIMESTAMP_PADDING: u16 = MSG_LR_PADDING + (MSG_LR_PADDING / 4);
const TIMESTAMP_WIDTH: u16 = DEF_SIZE * 2 + RIGHT_TIMESTAMP_PADDING + LEFT_TIMESTAMP_PADDING;

#[allow(clippy::mutable_key_type)]
#[allow(clippy::too_many_arguments)]
pub fn build_event_history<'a>(
    content_store: &ContentStore,
    thumbnail_cache: &ThumbnailCache,
    client: &Client,
    guild: &Guild,
    channel: &Channel,
    members: &Members,
    current_user_id: u64,
    looking_at_message: usize,
    scrollable_state: &'a mut scrollable::State,
    buts_sate: &'a mut EventHistoryButsState,
    mode: Mode,
    theme: Theme,
) -> Element<'a, Message> {
    let mut event_history = Scrollable::new(scrollable_state)
        .on_scroll(Message::MessageHistoryScrolled)
        .width(length!(+))
        .height(length!(+))
        .style(theme)
        .align_items(Align::Start)
        .spacing(SPACING * 2);

    let timeline_range_end = looking_at_message
        .saturating_add(SHOWN_MSGS_LIMIT)
        .min(channel.messages.len());
    let timeline_range_start = timeline_range_end.saturating_sub(SHOWN_MSGS_LIMIT);
    let timeline_range_start = channel
        .messages
        .get_index_of(&MessageId::Ack(channel.last_known_message_id))
        .map_or(timeline_range_start, |last_message_pos| {
            timeline_range_start.max(last_message_pos)
        });
    let mut displayable_events = channel
        .messages
        .iter()
        .skip(timeline_range_start)
        .take(timeline_range_end - timeline_range_start)
        .map(|(_, m)| m);

    let timezone = chrono::Local::now().timezone();

    let first_message = if let Some(msg) = displayable_events.next() {
        msg
    } else {
        return event_history.into();
    };
    let mut last_timestamp = timezone.from_utc_datetime(&first_message.timestamp);
    let mut last_sender_id = None;
    let mut last_sender_name = None;
    let mut message_group = Vec::with_capacity(SHOWN_MSGS_LIMIT);

    let push_to_msg_group = |msg_group: &mut Vec<Element<'a, Message>>| {
        let mut content = Vec::with_capacity(msg_group.len());
        content.append(msg_group);
        let content = Column::with_children(content)
            .padding(PADDING)
            .spacing(SPACING)
            .align_items(Align::Start);

        Column::with_children(vec![
            content.into(),
            space!(h = PADDING / 4).into(),
            Rule::horizontal(0)
                .style(theme.border_width(2.0).border_radius(0.0).padded(FillMode::Full))
                .into(),
        ])
        .spacing(SPACING)
        .align_items(Align::Start)
    };

    for (
        message,
        (
            media_open_button_states,
            h_embed_but,
            f_embed_but,
            avatar_but_state,
            goto_reply_state,
            message_buts_state,
            external_url_states,
            menu_list_state,
        ),
    ) in (std::iter::once(first_message).chain(displayable_events)).zip(buts_sate.iter_mut())
    {
        let id_to_use = message
            .id
            .is_ack()
            .not()
            .some(current_user_id)
            .unwrap_or(message.sender);

        let message_timestamp = timezone.from_utc_datetime(&message.timestamp);
        let member = members.get(&id_to_use);
        let name_to_use = member.map_or("unknown member", |member| member.username.as_str());
        let sender_status = member.map_or(UserStatus::Offline, |m| m.status);
        let is_sender_bot = member.map_or(false, |m| m.is_bot);
        let override_reason_raw = message
            .overrides
            .as_ref()
            .and_then(|overrides| overrides.reason.as_ref());
        let override_reason = override_reason_raw.map(|reason| match reason {
            Reason::Bridge(_) => {
                format!("bridged by {}", name_to_use)
            }
            Reason::SystemMessage(_) => "system message".to_string(),
            Reason::UserDefined(reason) => reason.to_string(),
            Reason::Webhook(_) => {
                format!("webhook by {}", name_to_use)
            }
            Reason::SystemPlurality(_) => "plurality".to_string(),
        });
        let sender_display_name = message.overrides.as_ref().map_or(name_to_use, |ov| ov.name.as_str());
        let sender_avatar_url = message.overrides.as_ref().map_or_else(
            || member.and_then(|m| m.avatar_url.as_ref()),
            |ov| ov.avatar_url.as_ref(),
        );
        let sender_body_creator = |avatar_but_state: &'a mut button::State| {
            let mut widgets = Vec::with_capacity(7);
            let label_container = |label| {
                Container::new(label)
                    .style(theme.secondary())
                    .padding([PADDING / 2, PADDING / 2])
                    .center_x()
                    .center_y()
                    .into()
            };

            let status_color = theme.status_color(sender_status);
            let pfp: Element<Message> = sender_avatar_url
                .and_then(|u| thumbnail_cache.avatars.get(u))
                .cloned()
                .map_or_else(
                    || label!(sender_display_name.chars().next().unwrap_or('u').to_ascii_uppercase()).into(),
                    |handle| {
                        const LEN: Length = length!(= AVATAR_WIDTH - 4);
                        Image::new(handle).height(LEN).width(LEN).into()
                    },
                );

            {
                const LEN: Length = length!(= AVATAR_WIDTH);
                let theme = theme.border_width(2.5).border_color(status_color);
                widgets.push(fill_container(pfp).width(LEN).height(LEN).style(theme).into());
            }

            widgets.push(space!(w = LEFT_TIMESTAMP_PADDING + SPACING).into());
            let sender_name_color = guild
                .highest_role_for_member(id_to_use)
                .map_or(Color::WHITE, |(_, role)| tuple_to_iced_color(role.color));
            widgets.push(label_container(
                label!(sender_display_name)
                    .size(MESSAGE_SENDER_SIZE)
                    .color(sender_name_color),
            ));

            (!matches!(
                override_reason_raw,
                Some(Reason::Bridge(_) | Reason::SystemPlurality(_))
            ) && is_sender_bot)
                .and_do(|| {
                    widgets.push(space!(w = SPACING * 2).into());
                    widgets.push(label_container(label!("Bot").size(MESSAGE_SENDER_SIZE - 4)));
                });

            override_reason.as_ref().and_do(|reason| {
                widgets.push(space!(w = SPACING * 2).into());
                widgets.push(label_container(
                    label!(reason).color(ALT_COLOR).size(MESSAGE_SIZE).width(length!(-)),
                ));
            });

            let content = Row::with_children(widgets)
                .align_items(Align::Center)
                .max_height(AVATAR_WIDTH as u32);

            Button::new(avatar_but_state, content)
                .on_press(Message::SelectedMember(id_to_use))
                .style(theme.secondary())
                .into()
        };

        let is_sender_different =
            last_sender_id.as_ref() != Some(&id_to_use) || last_sender_name != Some(sender_display_name);

        if is_sender_different {
            if message_group.is_empty().not() {
                event_history = event_history.push(push_to_msg_group(&mut message_group));
            }
            message_group.push(sender_body_creator(avatar_but_state));
        } else if message_timestamp.day() != last_timestamp.day() {
            let date_time_seperator = fill_container(
                label!(message_timestamp.format("[%d %B %Y]").to_string())
                    .size(DATE_SEPERATOR_SIZE)
                    .color(color!(153, 153, 153)),
            )
            .height(length!(-));

            event_history = event_history.push(push_to_msg_group(&mut message_group));
            event_history = event_history.push(date_time_seperator);
            event_history = event_history.push(
                Rule::horizontal(SPACING).style(theme.border_width(2.0).border_radius(0.0).padded(FillMode::Full)),
            );
            message_group.push(sender_body_creator(avatar_but_state));
        } else if message_group.is_empty().not()
            && last_timestamp.signed_duration_since(message_timestamp) > chrono::Duration::minutes(5)
        {
            event_history = event_history.push(push_to_msg_group(&mut message_group));
            message_group.push(sender_body_creator(avatar_but_state));
        }

        let mut message_body_widgets = Vec::with_capacity(2);

        let msg_text = message.being_edited.as_deref().or_else(|| {
            if let IcyContent::Text(text) = &message.content {
                Some(text)
            } else {
                None
            }
        });

        if let Some(textt) = msg_text {
            let tokens = textt.parse_md_custom(HarmonyToken::parse);
            let mut widgets = Vec::with_capacity(tokens.len());
            let color = (Mode::EditingMessage(id_to_use) == mode)
                .then(|| ERROR_COLOR)
                .unwrap_or(theme.colorscheme.text);

            let is_emotes_until_line_break = |at: usize| {
                tokens
                    .iter()
                    .skip(at)
                    .take_while(|tok| !matches!(tok, Token::LineBreak))
                    .all(|tok| matches!(tok, Token::Custom(HarmonyToken::Emote(_))))
            };
            let has_mention = tokens.iter().any(|tok| {
                if let Token::Custom(HarmonyToken::Mention(id)) = tok {
                    current_user_id.eq(id)
                } else {
                    false
                }
            });
            let mut only_emotes = is_emotes_until_line_break(0);
            let mut line_widgets = Vec::with_capacity(5);
            let mk_text_elem = |text: &Text| -> Element<Message> {
                let Text { value, code, .. } = text;
                let mut text = label!(value.trim()).color(color).size(MESSAGE_SIZE);
                if *code {
                    text = text.font(IOSEVKA);
                    let mut bg_color = theme.colorscheme.primary_bg;
                    bg_color.r *= 1.6;
                    bg_color.g *= 1.6;
                    bg_color.b *= 1.6;
                    Container::new(text)
                        .style(theme.border_width(0.0).background_color(bg_color))
                        .into()
                } else {
                    text.into()
                }
            };

            message_buts_state.resize_with(tokens.len(), Default::default);
            for ((at, token), but_state) in tokens.iter().enumerate().zip(message_buts_state.iter_mut()) {
                match token {
                    Token::Custom(tok) => match tok {
                        HarmonyToken::Emote(id) => match thumbnail_cache.emotes.get(&FileId::Id(id.to_string())) {
                            Some(handle) => {
                                let tooltip = |content, state| {
                                    Tooltip::new(
                                        Button::new(state, content)
                                            .style(theme)
                                            .padding(0)
                                            .on_press(Message::CopyToClipboard(id.to_string())),
                                        format!(
                                            "Click to copy ID of {}",
                                            truncate_string(client.get_emote_name(id).unwrap_or(id), 10)
                                        ),
                                        iced::tooltip::Position::Top,
                                    )
                                    .size(MESSAGE_SIZE)
                                    .gap(PADDING / 2)
                                    .style(theme)
                                    .into()
                                };
                                if only_emotes {
                                    line_widgets.push(tooltip(
                                        Image::new(handle.clone()).width(length!(= 48)).height(length!( = 48)),
                                        but_state,
                                    ));
                                } else {
                                    line_widgets.push(tooltip(
                                        Image::new(handle.clone())
                                            .width(length!(= MESSAGE_SIZE + 4))
                                            .height(length!( = MESSAGE_SIZE + 4)),
                                        but_state,
                                    ));
                                    line_widgets.push(label!(" ").into());
                                }
                            }
                            None => {
                                line_widgets
                                    .push(label!(format!("<:{}:> ", id)).size(MESSAGE_SIZE).color(color).into());
                            }
                        },
                        HarmonyToken::Mention(id) => {
                            let member_name = members.get(id).map_or_else(|| "unknown user", |m| m.username.as_str());
                            let role_color = guild
                                .highest_role_for_member(*id)
                                .map_or(theme.colorscheme.text, |(_, role)| tuple_to_iced_color(role.color));

                            line_widgets.push(
                                Button::new(
                                    but_state,
                                    label!(format!("@{}", member_name)).size(MESSAGE_SIZE).color(role_color),
                                )
                                .padding([2, 3])
                                .height(length!(= MESSAGE_SIZE + 4))
                                .style({
                                    let theme = theme.background_color(Color { a: 0.1, ..role_color });
                                    has_mention.then(|| theme.border_width(0.0)).unwrap_or(theme)
                                })
                                .on_press(Message::SelectedMember(*id))
                                .into(),
                            );
                            line_widgets.push(label!(" ").into());
                        }
                    },
                    Token::Text(text) => {
                        line_widgets.push(mk_text_elem(text));
                        line_widgets.push(label!(" ").into());
                    }
                    Token::Url { name, url, .. } => {
                        let url = *url;
                        let color = theme.colorscheme.accent;
                        let label = label!(name.as_ref().map_or(url, |text| text.value))
                            .color(color)
                            .size(MESSAGE_SIZE);
                        line_widgets.push(
                            Tooltip::new(
                                Button::new(but_state, label)
                                    .padding([2, 3])
                                    .style({
                                        let theme = theme.background_color(Color { a: 0.1, ..color });
                                        has_mention.then(|| theme.border_width(0.0)).unwrap_or(theme)
                                    })
                                    .on_press(Message::OpenUrl(url.into()))
                                    .height(length!(= MESSAGE_SIZE + 4)),
                                format!("Go to {}", url),
                                iced::tooltip::Position::Top,
                            )
                            .size(MESSAGE_SIZE)
                            .style(theme)
                            .gap(PADDING / 2)
                            .into(),
                        );
                        line_widgets.push(label!(" ").into());
                    }
                    Token::Header(depth) => {
                        line_widgets.push(
                            label!((0..*depth + 1)
                                .enumerate()
                                .map(|(at, _)| if at == *depth { ' ' } else { '#' })
                                .collect::<String>())
                            .color(color)
                            .size(MESSAGE_SIZE)
                            .into(),
                        );
                    }
                    Token::ListItem(number) => {
                        let prefix = match number {
                            Some(num) => label!(format!("{}. ", num)),
                            None => label!(". "),
                        };
                        line_widgets.push(prefix.size(MESSAGE_SIZE).color(color).into());
                    }
                    Token::CodeFence { code, .. } => {
                        only_emotes = is_emotes_until_line_break(at);
                        widgets.push(
                            Row::with_children(line_widgets.drain(..).collect())
                                .align_items(Align::Center)
                                .into(),
                        );
                        line_widgets.push(mk_text_elem(&Text {
                            value: code,
                            code: true,
                            ..Default::default()
                        }));
                    }
                    Token::LineBreak => {
                        only_emotes = is_emotes_until_line_break(at);
                        widgets.push(
                            Row::with_children(line_widgets.drain(..).collect())
                                .align_items(Align::Center)
                                .into(),
                        );
                    }
                }
            }

            widgets.push(
                Row::with_children(line_widgets.drain(..).collect())
                    .align_items(Align::Center)
                    .into(),
            );

            let (container_theme, container_padding) = has_mention
                .then(|| {
                    (
                        theme
                            .border_width(2.0)
                            .border_radius(4.0)
                            .border_color(color!(225, 225, 0, 180)),
                        2,
                    )
                })
                .unwrap_or((theme, 0));
            message_body_widgets.push(
                Container::new(Column::with_children(widgets).align_items(Align::Start))
                    .center_x()
                    .center_y()
                    .padding(container_padding)
                    .style(container_theme)
                    .into(),
            );

            let urls = textt.split_whitespace().map(Url::parse).flatten().collect::<Vec<_>>();
            external_url_states.resize_with(urls.len(), Default::default);
            for (url, media_open_button_state) in urls.into_iter().zip(external_url_states.iter_mut()) {
                if let Some(data) = client.link_datas.get(&url) {
                    match data {
                        FetchLinkData::IsSite(site) => {
                            let desc_words = site
                                .description
                                .split_whitespace()
                                .fold(
                                    (String::with_capacity(site.description.len() + 6), 0),
                                    |(mut total, mut len), item| {
                                        total.push_str(item);
                                        len += item.len();
                                        total.push(' ');
                                        if len >= 50 {
                                            len = 0;
                                            total.push('\n');
                                        }
                                        (total, len)
                                    },
                                )
                                .0;
                            let mut widgets = Vec::with_capacity(6);
                            widgets.push(
                                Row::with_children(vec![
                                    space!(w = PADDING / 2).into(),
                                    label!(truncate_string(&site.page_title, 50)).size(DEF_SIZE - 2).into(),
                                ])
                                .align_items(Align::Center)
                                .into(),
                            );
                            if !desc_words.is_empty() {
                                widgets.push(
                                    Rule::horizontal(SPACING)
                                        .style(theme.border_radius(0.0).padded(FillMode::Full))
                                        .into(),
                                );
                                widgets.push(
                                    Row::with_children(vec![
                                        space!(w = PADDING / 2).into(),
                                        label!(desc_words).size(DEF_SIZE - 5).into(),
                                    ])
                                    .align_items(Align::Center)
                                    .into(),
                                );
                            }
                            let content_width = (DEF_SIZE - 2) * 24;
                            if let Some(handle) = site
                                .image
                                .parse()
                                .ok()
                                .and_then(|url| thumbnail_cache.thumbnails.get(&FileId::External(url)))
                            {
                                widgets.push(
                                    Rule::horizontal(SPACING)
                                        .style(theme.border_radius(0.0).padded(FillMode::Full))
                                        .into(),
                                );
                                widgets.push(
                                    Row::with_children(vec![
                                        space!(w = PADDING / 2).into(),
                                        Image::new(handle.clone())
                                            .width(length!(= content_width - PADDING))
                                            .height(length!(-))
                                            .into(),
                                    ])
                                    .align_items(Align::Center)
                                    .into(),
                                );
                            }
                            let content = Column::with_children(widgets)
                                .width(length!(= content_width))
                                .spacing(SPACING)
                                .align_items(Align::Start);

                            let url: String = url.into();
                            message_body_widgets.push(
                                Button::new(media_open_button_state, content)
                                    .padding([PADDING / 2, 0])
                                    .on_press(Message::OpenUrl(url.into()))
                                    .style(theme.secondary().border_width(2.0))
                                    .into(),
                            );
                        }
                        FetchLinkData::IsMedia(media) => {
                            let id = FileId::External(url);
                            let is_thumbnail = media.mimetype.starts_with("image");
                            let does_content_exist = content_store.content_exists(&id);

                            let content: Element<Message> = thumbnail_cache.thumbnails.get(&id).map_or_else(
                                || {
                                    let text = does_content_exist.some("Open").unwrap_or("Download");
                                    label!("{} {}", text, media.filename).into()
                                },
                                |handle| {
                                    // TODO: Don't hardcode this length, calculate it using the size of the window
                                    let image = Image::new(handle.clone()).width(length!(= 320));
                                    let text = does_content_exist.map_or_else(
                                        || label!("Download {}", media.filename),
                                        || label!(&media.filename),
                                    );

                                    Column::with_children(vec![text.size(DEF_SIZE - 4).into(), image.into()])
                                        .spacing(SPACING)
                                        .into()
                                },
                            );
                            message_body_widgets.push(
                                Button::new(media_open_button_state, content)
                                    .on_press(Message::OpenContent {
                                        attachment: Attachment {
                                            kind: media.mimetype.clone(),
                                            name: media.filename.clone(),
                                            ..Attachment::new_unknown(id)
                                        },
                                        is_thumbnail,
                                    })
                                    .style(theme.secondary().border_width(2.0))
                                    .into(),
                            );
                        }
                    }
                }
            }
        }

        if let IcyContent::Embeds(embeds) = &message.content {
            let put_heading =
                |embed: &mut Vec<Element<'a, Message>>, h: &EmbedHeading, state: &'a mut button::State| {
                    (h.text.is_empty() && h.subtext.is_empty()).not().and_do(move || {
                        let mut heading = Vec::with_capacity(3);

                        if let Some(img_url) = &h.icon {
                            if let Some(handle) = thumbnail_cache.thumbnails.get(img_url) {
                                heading.push(
                                    Image::new(handle.clone())
                                        .height(length!(=24))
                                        .width(length!(=24))
                                        .into(),
                                );
                            }
                        }

                        heading.push(label!(&h.text).size(DEF_SIZE + 2).into());
                        heading.push(
                            label!(&h.subtext)
                                .size(DEF_SIZE - 6)
                                .color(color!(200, 200, 200))
                                .into(),
                        );

                        let mut but = Button::new(state, row(heading).padding(0).spacing(SPACING)).style(theme.embed());

                        if let Some(url) = h.url.clone() {
                            but = but.on_press(Message::OpenUrl(url));
                        }

                        embed.push(but.into());
                    });
                };

            let mut embed = Vec::with_capacity(5);

            if let Some(h) = &embeds.header {
                put_heading(&mut embed, h, h_embed_but);
            }

            embed.push(label!(&embeds.title).size(DEF_SIZE + 2).into());
            embed.push(
                label!(&embeds.body)
                    .color(color!(220, 220, 220))
                    .size(DEF_SIZE - 2)
                    .into(),
            );

            for f in &embeds.fields {
                // TODO: handle presentation
                let field = vec![
                    label!(&f.title).size(DEF_SIZE - 1).into(),
                    label!(&f.subtitle).size(DEF_SIZE - 3).into(),
                    label!(&f.body).color(color!(220, 220, 220)).size(DEF_SIZE - 3).into(),
                ];

                embed.push(
                    Container::new(
                        column(field)
                            .padding(PADDING / 4)
                            .spacing(SPACING / 4)
                            .align_items(Align::Start),
                    )
                    .style(theme)
                    .into(),
                );
            }

            if let Some(h) = &embeds.footer {
                put_heading(&mut embed, h, f_embed_but);
            }

            message_body_widgets.push(
                Container::new(
                    column(embed)
                        .padding(PADDING / 2)
                        .spacing(SPACING / 2)
                        .align_items(Align::Start),
                )
                .style(theme.secondary().border_color(tuple_to_iced_color(embeds.color)))
                .into(),
            );
        }

        if let IcyContent::Files(attachments) = &message.content {
            media_open_button_states.resize_with(attachments.len(), Default::default);
            for (attachment, media_open_button_state) in attachments.iter().zip(media_open_button_states.iter_mut()) {
                let is_thumbnail = attachment.kind.starts_with("image");
                let does_content_exist = content_store.content_exists(&attachment.id);

                let resolution = attachment
                    .resolution
                    .map(|(w, h)| scale_down(w, h, 400))
                    .map(|(w, h)| (w as u16, h as u16));
                let content: Element<Message> = thumbnail_cache.thumbnails.get(&attachment.id).map_or_else(
                    || {
                        resolution.map_or_else(
                            || space!(= 0, 0).into(),
                            |(w, h)| {
                                thumbnail_cache.minithumbnails.get(&attachment.id).map_or_else(
                                    || space!(= w, h).into(),
                                    |handle| {
                                        Image::new(handle.clone())
                                            .width(length!(= w))
                                            .height(length!(= h))
                                            .into()
                                    },
                                )
                            },
                        )
                    },
                    |handle| {
                        // TODO: Don't hardcode this length, calculate it using the size of the window
                        if let Some((w, h)) = resolution {
                            Image::new(handle.clone())
                                .width(length!(= w))
                                .height(length!(= h))
                                .into()
                        } else {
                            Image::new(handle.clone()).width(length!(= 400)).into()
                        }
                    },
                );
                let text = label!(
                    "{} {}",
                    does_content_exist.some("Open").unwrap_or("Download"),
                    attachment.name
                );
                let content = Column::with_children(vec![text.size(DEF_SIZE - 4).into(), content]).spacing(SPACING);
                message_body_widgets.push(
                    Button::new(media_open_button_state, content)
                        .on_press(Message::OpenContent {
                            attachment: attachment.clone(),
                            is_thumbnail,
                        })
                        .style(theme.secondary().border_width(2.0))
                        .into(),
                );
            }
        }

        let msg_body = Column::with_children(message_body_widgets)
            .align_items(Align::Start)
            .spacing(MSG_LR_PADDING);
        let mut message_row = Vec::with_capacity(3);

        let maybe_reply_message = message.reply_to.map(|id| channel.messages.get(&MessageId::Ack(id)));

        let mut options = Vec::with_capacity(4);
        if matches!(message.content, IcyContent::Text(_)) {
            options.push(MessageMenuOption::Copy(message.id));
        }
        if let Some(id) = message.id.id() {
            options.push(MessageMenuOption::Reply(id));
            options.push(MessageMenuOption::CopyMessageId(id));
            if msg_text.is_some() && current_user_id == message.sender {
                options.push(MessageMenuOption::Edit(id));
                options.push(MessageMenuOption::Delete(id));
            }
        }

        let timestamp_formatted = message_timestamp.format("%H:%M").to_string();
        let timestamp = Container::new(
            PickList::new(menu_list_state, options, None, Message::MessageMenuSelected)
                .placeholder(timestamp_formatted)
                .text_size(MESSAGE_TIMESTAMP_SIZE)
                .menu_text_size(MESSAGE_SIZE)
                .padding(0)
                .style(
                    theme
                        .placeholder_color(color!(160, 160, 160))
                        .border_width(0.0)
                        .icon_size(0.0),
                )
                .font(IOSEVKA)
                .menu_font(Font::Default),
        )
        .padding([PADDING / 8, RIGHT_TIMESTAMP_PADDING, 0, LEFT_TIMESTAMP_PADDING])
        .width(length!(= TIMESTAMP_WIDTH))
        .center_x()
        .center_y();
        message_row.push(timestamp.into());
        message_row.push(msg_body.into());

        let mut message_col = Vec::with_capacity(2);

        if let Some(reply_message) = maybe_reply_message {
            let widget = make_reply_message(reply_message, client, theme, Message::GotoReply, goto_reply_state);

            message_col.push(
                Row::with_children(vec![
                    space!(w = TIMESTAMP_WIDTH / 5).into(),
                    Row::with_children(vec![icon(Icon::Reply).size(MESSAGE_SIZE).into(), widget.into()])
                        .spacing(SPACING)
                        .align_items(Align::Center)
                        .into(),
                ])
                .align_items(Align::Center)
                .into(),
            );
        }

        message_col.push(
            Row::with_children(message_row)
                .align_items(Align::Start)
                .spacing(SPACING)
                .into(),
        );

        message_group.push(Column::with_children(message_col).align_items(Align::Start).into());

        last_sender_id = Some(id_to_use);
        last_sender_name = Some(sender_display_name);
        last_timestamp = message_timestamp;
    }
    if message_group.is_empty().not() {
        event_history = event_history.push(push_to_msg_group(&mut message_group));
    }
    event_history.into()
}
