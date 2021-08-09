use std::ops::Not;

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
        truncate_string,
    },
    space,
    style::{
        Theme, ALT_COLOR, AVATAR_WIDTH, DATE_SEPERATOR_SIZE, DEF_SIZE, ERROR_COLOR, MESSAGE_SENDER_SIZE, MESSAGE_SIZE,
        MESSAGE_TIMESTAMP_SIZE, PADDING, SPACING,
    },
    IOSEVKA,
};
use chrono::{Datelike, TimeZone, Timelike};
use client::{
    bool_ext::BoolExt,
    guild::Guild,
    harmony_rust_sdk::api::harmonytypes::{r#override::Reason, UserStatus},
    message::MessageId,
    render_text,
    smol_str::SmolStr,
    OptionExt,
};
use iced::{rule::FillMode, Font, Tooltip};

pub const SHOWN_MSGS_LIMIT: usize = 32;
pub type EventHistoryButsState = [(
    Vec<button::State>,
    button::State,
    button::State,
    button::State,
    button::State,
    button::State,
    button::State,
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
            edit_but_state,
            avatar_but_state,
            reply_but_state,
            goto_reply_state,
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
        let name_to_use = member.map_or_else(SmolStr::default, |member| member.username.clone());
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
        let sender_display_name = message
            .overrides
            .as_ref()
            .map_or(name_to_use, |ov| ov.name.as_str().into());
        let sender_avatar_url = message.overrides.as_ref().map_or_else(
            || member.and_then(|m| m.avatar_url.as_ref()),
            |ov| ov.avatar_url.as_ref(),
        );
        let sender_body_creator = |sender_display_name: &str, avatar_but_state: &'a mut button::State| {
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
                .map_or(Color::WHITE, |(_, role)| {
                    Color::from_rgb8(role.color.0, role.color.1, role.color.2)
                });
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
            last_sender_id.as_ref() != Some(&id_to_use) || last_sender_name.as_ref() != Some(&sender_display_name);

        if is_sender_different {
            if message_group.is_empty().not() {
                event_history = event_history.push(push_to_msg_group(&mut message_group));
            }
            message_group.push(sender_body_creator(&sender_display_name, avatar_but_state));
        } else if message_timestamp.day() != last_timestamp.day() {
            let date_time_seperator = fill_container(
                label!(message_timestamp.format("[%d %B %Y]").to_string())
                    .size(DATE_SEPERATOR_SIZE)
                    .color(color!(153, 153, 153)),
            )
            .height(length!(-));

            event_history = event_history.push(push_to_msg_group(&mut message_group));
            event_history = event_history.push(date_time_seperator);
            message_group.push(sender_body_creator(&sender_display_name, avatar_but_state));
        } else if message_group.is_empty().not()
            && last_timestamp.signed_duration_since(message_timestamp) > chrono::Duration::minutes(5)
        {
            event_history = event_history.push(push_to_msg_group(&mut message_group));
            message_group.push(sender_body_creator(&sender_display_name, avatar_but_state));
        }

        let mut message_body_widgets = Vec::with_capacity(2);

        let msg_text = message.being_edited.as_deref().or_else(|| {
            if let IcyContent::Text(text) = &message.content {
                Some(text)
            } else {
                None
            }
        });

        msg_text.and_do(|textt| {
            let text = client::render_text(textt, members);
            #[cfg(feature = "markdown")]
            let message_text = super::markdown::markdown_svg(&text);
            #[cfg(not(feature = "markdown"))]
            let mut message_text = label!(text).size(MESSAGE_SIZE);

            #[cfg(not(feature = "markdown"))]
            if !message.id.is_ack() || message.being_edited.is_some() {
                message_text = message_text.color(color!(200, 200, 200));
            } else if mode == message.id.id().map_or(Mode::Normal, Mode::EditingMessage) {
                message_text = message_text.color(ERROR_COLOR);
            }

            message_body_widgets.push(message_text.into());
        });

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
                    .style(theme.round())
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
                .style(theme.round().secondary().border_color(Color::from_rgb8(
                    embeds.color.0,
                    embeds.color.1,
                    embeds.color.2,
                )))
                .into(),
            );
        }

        if let IcyContent::Files(attachments) = &message.content {
            media_open_button_states.resize_with(attachments.len(), Default::default);
            for (attachment, media_open_button_state) in attachments.iter().zip(media_open_button_states.iter_mut()) {
                let is_thumbnail = matches!(attachment.kind.split('/').next(), Some("image"));
                let does_content_exist = content_store.content_exists(&attachment.id);

                let content: Element<Message> = thumbnail_cache.thumbnails.get(&attachment.id).map_or_else(
                    || {
                        let text = does_content_exist.some("Open").unwrap_or("Download");
                        label!("{} {}", text, attachment.name).into()
                    },
                    |handle| {
                        // TODO: Don't hardcode this length, calculate it using the size of the window
                        let image = Image::new(handle.clone()).width(length!(= 320));
                        let text = does_content_exist
                            .map_or_else(|| label!("Download {}", attachment.name), || label!(&attachment.name));

                        Column::with_children(vec![text.size(DEF_SIZE - 4).into(), image.into()])
                            .spacing(SPACING)
                            .into()
                    },
                );
                message_body_widgets.push(
                    Button::new(media_open_button_state, content)
                        .on_press(Message::OpenContent {
                            attachment: attachment.clone(),
                            is_thumbnail,
                        })
                        .style(theme.secondary())
                        .into(),
                );
            }
        }

        let msg_body = Column::with_children(message_body_widgets)
            .align_items(Align::Start)
            .spacing(MSG_LR_PADDING);
        let mut message_row = Vec::with_capacity(5);

        let maybe_reply_message = message
            .reply_to
            .and_then(|id| channel.messages.get(&MessageId::Ack(id)));

        let maybe_timestamp = (maybe_reply_message.is_some()
            || is_sender_different
            || last_timestamp.minute() != message_timestamp.minute())
        .map_or_else(
            || space!(w = TIMESTAMP_WIDTH).into(),
            || {
                let message_timestamp = message_timestamp.format("%H:%M").to_string();

                Container::new(
                    label!(message_timestamp)
                        .size(MESSAGE_TIMESTAMP_SIZE)
                        .color(color!(160, 160, 160))
                        .font(IOSEVKA)
                        .width(length!(+)),
                )
                .padding([PADDING / 8, RIGHT_TIMESTAMP_PADDING, 0, LEFT_TIMESTAMP_PADDING])
                .width(length!(= TIMESTAMP_WIDTH))
                .center_x()
                .center_y()
                .into()
            },
        );
        message_row.push(maybe_timestamp);
        message_row.push(msg_body.into());
        message_row.push(space!(w+).into());

        if let Some(id) = message.id.id() {
            let but = Tooltip::new(
                Button::new(reply_but_state, icon(Icon::Reply).size(MESSAGE_SIZE - 10))
                    .on_press(Message::ReplyToMessage(id))
                    .style(theme.secondary()),
                "Reply to message",
                iced::tooltip::Position::Top,
            )
            .size(MESSAGE_SIZE - 2)
            .style(theme);
            message_row.push(but.into());
            if msg_text.is_some() && current_user_id == message.sender {
                let but = Tooltip::new(
                    Button::new(edit_but_state, icon(Icon::Pencil).size(MESSAGE_SIZE - 10))
                        .on_press(Message::ChangeMode(Mode::EditingMessage(id)))
                        .style(theme.secondary()),
                    "Edit message",
                    iced::tooltip::Position::Top,
                )
                .size(MESSAGE_SIZE - 2)
                .style(theme);
                message_row.push(but.into());
            }
        }

        let mut message_col = Vec::with_capacity(2);

        if let Some(reply_message) = maybe_reply_message {
            let name_to_use = members
                .get(&reply_message.sender)
                .map_or_else(SmolStr::default, |member| member.username.clone());
            let author_name = reply_message
                .overrides
                .as_ref()
                .map_or(name_to_use, |ov| ov.name.as_str().into());
            let color = color!(200, 200, 200);

            let author = label!(format!("@{}", author_name)).color(color).size(MESSAGE_SIZE - 4);
            let content = label!(match &reply_message.content {
                IcyContent::Text(text) =>
                    truncate_string(&render_text(&text.replace('\n', " "), members), 40).to_string(),
                IcyContent::Files(files) => {
                    let file_names = files.iter().map(|f| &f.name).fold(String::new(), |mut names, name| {
                        names.push_str(", ");
                        names.push_str(name);
                        names
                    });
                    format!("sent file(s): {}", file_names)
                }
                IcyContent::Embeds(_) => "sent an embed".to_string(),
            })
            .size(MESSAGE_SIZE - 4)
            .color(color);

            message_col.push(
                Row::with_children(vec![
                    space!(w = TIMESTAMP_WIDTH / 5).into(),
                    Row::with_children(vec![
                        icon(Icon::Reply).size(MESSAGE_SIZE).into(),
                        Button::new(
                            goto_reply_state,
                            Row::with_children(vec![author.into(), content.into()])
                                .align_items(Align::Center)
                                .spacing(SPACING / 2)
                                .padding(PADDING / 5),
                        )
                        .on_press(Message::GotoReply(reply_message.id))
                        .style(theme.round())
                        .into(),
                    ])
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
