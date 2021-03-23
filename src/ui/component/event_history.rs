use crate::{
    client::{
        channel::Channel,
        content::{ContentStore, ContentType, ThumbnailCache},
        member::Members,
        message::{Attachment, EmbedHeading},
    },
    color, label, space,
    ui::{
        component::*,
        screen::main::{Message, Mode},
        style::{
            Theme, ALT_COLOR, AVATAR_WIDTH, DATE_SEPERATOR_SIZE, DEF_SIZE, ERROR_COLOR,
            MESSAGE_SENDER_SIZE, MESSAGE_SIZE, MESSAGE_TIMESTAMP_SIZE, PADDING, SPACING,
        },
    },
};
use chrono::{Datelike, Timelike};
use harmony_rust_sdk::api::harmonytypes::r#override::Reason;

pub const SHOWN_MSGS_LIMIT: usize = 32;
const MSG_LR_PADDING: u16 = SPACING * 2;

#[allow(clippy::mutable_key_type)]
#[allow(clippy::clippy::too_many_arguments)]
pub fn build_event_history<'a>(
    content_store: &ContentStore,
    thumbnail_cache: &ThumbnailCache,
    channel: &Channel,
    members: &Members,
    current_user_id: u64,
    looking_at_message: usize,
    scrollable_state: &'a mut scrollable::State,
    content_open_buttons: &'a mut [button::State; SHOWN_MSGS_LIMIT],
    embed_buttons: &'a mut [[(button::State, button::State); SHOWN_MSGS_LIMIT]; SHOWN_MSGS_LIMIT],
    edit_buts_sate: &'a mut [button::State; SHOWN_MSGS_LIMIT],
    mode: Mode,
    theme: Theme,
) -> Element<'a, Message> {
    let mut event_history = Scrollable::new(scrollable_state)
        .on_scroll(
            |scroll_perc, prev_scroll_perc| Message::MessageHistoryScrolled {
                prev_scroll_perc,
                scroll_perc,
            },
        )
        .snap_to_bottom(true)
        .width(length!(+))
        .height(length!(+))
        .style(theme)
        .align_items(align!(|<))
        .spacing(SPACING * 2)
        .padding(PADDING);

    let displayable_events = &channel.messages;
    let timeline_range_end = looking_at_message
        .saturating_add(SHOWN_MSGS_LIMIT)
        .min(displayable_events.len());
    let timeline_range_start = timeline_range_end.saturating_sub(SHOWN_MSGS_LIMIT);
    let displayable_events = &displayable_events[timeline_range_start..timeline_range_end];

    let mut last_timestamp = if let Some(ev) = displayable_events.first() {
        ev.timestamp
    } else {
        return event_history.into();
    };
    let mut last_sender_id = None;
    let mut last_sender_name = None;
    let mut message_group = vec![];

    for (((message, media_open_button_state), embed_buts), edit_but_state) in displayable_events
        .iter()
        .zip(content_open_buttons.iter_mut())
        .zip(embed_buttons.iter_mut())
        .zip(edit_buts_sate.iter_mut())
    {
        let id_to_use = if !message.id.is_ack() {
            current_user_id
        } else {
            message.sender
        };

        let name_to_use = members
            .get(&id_to_use)
            .map_or_else(String::default, |member| member.username.clone());
        let override_reason = message
            .overrides
            .as_ref()
            .map(|overrides| overrides.reason.as_ref())
            .flatten()
            .map(|reason| match reason {
                Reason::Bridge(_) => {
                    format!("bridged by {}", name_to_use)
                }
                Reason::SystemMessage(_) => "system message".to_string(),
                Reason::UserDefined(reason) => reason.to_string(),
                Reason::Webhook(_) => {
                    format!("webhook by {}", name_to_use)
                }
                _ => todo!("plurality"),
            });
        let sender_display_name = if let Some(overrides) = &message.overrides {
            overrides.name.clone()
        } else {
            name_to_use
        };
        let sender_color = theme.calculate_sender_color(sender_display_name.len());
        let sender_avatar_url = if let Some(overrides) = &message.overrides {
            overrides.avatar_url.as_ref()
        } else {
            members
                .get(&id_to_use)
                .map(|m| m.avatar_url.as_ref())
                .flatten()
        };
        let sender_body_creator = |sender_display_name: &str| {
            let mut widgets = Vec::with_capacity(2);

            if let Some(handle) = sender_avatar_url
                .map(|u| thumbnail_cache.get_thumbnail(&u))
                .flatten()
                .cloned()
            {
                // TODO: Add `border_radius` styling for `Image` so we can use it here
                widgets.push(Image::new(handle).width(length!(= AVATAR_WIDTH)).into());
            }

            widgets.push(
                label!("[{}]", sender_display_name)
                    .color(sender_color)
                    .size(MESSAGE_SENDER_SIZE)
                    .into(),
            );

            if let Some(reason) = &override_reason {
                widgets.push(
                    label!(reason)
                        .color(ALT_COLOR)
                        .size(MESSAGE_SIZE)
                        .width(length!(-))
                        .into(),
                );
            }

            row(widgets)
                .max_height(AVATAR_WIDTH as u32)
                .spacing(MSG_LR_PADDING)
                .padding(0)
        };

        let is_sender_different = last_sender_id.as_ref() != Some(&id_to_use)
            || last_sender_name.as_ref() != Some(&sender_display_name);
        if is_sender_different {
            if !message_group.is_empty() {
                event_history = event_history.push(
                    Container::new(
                        column(message_group.drain(..).collect()).align_items(align!(|<)),
                    )
                    .style(theme.round()),
                );
            }
            message_group.push(sender_body_creator(&sender_display_name).into());
        }

        if message.timestamp.day() != last_timestamp.day() {
            let date_time_seperator = fill_container(
                label!(message.timestamp.format("[%d %B %Y]").to_string())
                    .size(DATE_SEPERATOR_SIZE)
                    .color(color!(153, 153, 153)),
            )
            .height(length!(-));

            event_history = event_history.push(date_time_seperator);
        }

        if !is_sender_different
            && !message_group.is_empty()
            && last_timestamp.signed_duration_since(message.timestamp)
                > chrono::Duration::minutes(5)
        {
            event_history = event_history.push(
                Container::new(column(message_group.drain(..).collect()).align_items(align!(|<)))
                    .style(theme.round()),
            );
            message_group.push(sender_body_creator(&sender_display_name).into());
        }

        let mut message_body_widgets = Vec::with_capacity(2);

        let mut message_text = label!(message
            .being_edited
            .as_deref()
            .unwrap_or_else(|| message.content.as_str()))
        .size(MESSAGE_SIZE);

        if !message.id.is_ack() || message.being_edited.is_some() {
            message_text = message_text.color(color!(200, 200, 200));
        } else if mode == message.id.id().map_or(Mode::Normal, Mode::EditingMessage) {
            message_text = message_text.color(ERROR_COLOR);
        }

        message_body_widgets.push(message_text.into());

        for (e, (h_but_state, f_but_state)) in message.embeds.iter().zip(embed_buts.iter_mut()) {
            let put_heading = |embed: &mut Vec<Element<'a, Message>>,
                               h: &EmbedHeading,
                               state: &'a mut button::State| {
                if !(h.text.is_empty() && h.subtext.is_empty()) {
                    let mut heading = Vec::with_capacity(3);

                    if let Some(img_url) = &h.icon {
                        if let Some(handle) = thumbnail_cache.get_thumbnail(img_url) {
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

                    let mut but = Button::new(state, row(heading).padding(0).spacing(SPACING))
                        .style(theme.embed());

                    if let Some(url) = &h.url {
                        but = but.on_press(Message::OpenUrl(url.clone()));
                    }

                    embed.push(but.into());
                }
            };

            let mut embed = Vec::with_capacity(5);

            if let Some(h) = &e.header {
                put_heading(&mut embed, h, h_but_state);
            }

            embed.push(label!(&e.title).size(DEF_SIZE + 2).into());
            embed.push(
                label!(&e.body)
                    .color(color!(220, 220, 220))
                    .size(DEF_SIZE - 2)
                    .into(),
            );

            for f in &e.fields {
                // TODO: handle presentation
                let mut field = Vec::with_capacity(2);

                field.push(label!(&f.title).size(DEF_SIZE - 1).into());
                field.push(label!(&f.subtitle).size(DEF_SIZE - 3).into());
                field.push(
                    label!(&f.body)
                        .color(color!(220, 220, 220))
                        .size(DEF_SIZE - 3)
                        .into(),
                );

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

            if let Some(h) = &e.footer {
                put_heading(&mut embed, h, f_but_state);
            }

            message_body_widgets.push(
                Container::new(
                    column(embed)
                        .padding(PADDING / 2)
                        .spacing(SPACING / 2)
                        .align_items(Align::Start),
                )
                .style(theme.round().secondary().with_border_color(e.color))
                .into(),
            );
        }

        if let Some(attachment) = message.attachments.first() {
            fn create_button<'a>(
                is_thumbnail: bool,
                attachment: Attachment,
                content: impl Into<Element<'a, Message>>,
                button_state: &'a mut button::State,
                theme: Theme,
            ) -> Element<'a, Message> {
                Button::new(button_state, content.into())
                    .on_press(Message::OpenContent {
                        attachment,
                        is_thumbnail,
                    })
                    .style(theme.secondary())
                    .into()
            };

            let is_thumbnail = matches!(attachment.kind, ContentType::Image);
            let does_content_exist = content_store.content_exists(&attachment.id);

            if let Some(thumbnail_image) = thumbnail_cache
                .get_thumbnail(&attachment.id)
                // FIXME: Don't hardcode this length, calculate it using the size of the window
                .map(|handle| Image::new(handle.clone()).width(length!(= 320)))
            {
                if does_content_exist {
                    message_body_widgets.push(create_button(
                        is_thumbnail,
                        attachment.clone(),
                        Column::with_children(vec![
                            label!("{}", attachment.name).size(DEF_SIZE - 4).into(),
                            thumbnail_image.into(),
                        ])
                        .spacing(SPACING),
                        media_open_button_state,
                        theme,
                    ));
                } else {
                    let button = create_button(
                        is_thumbnail,
                        attachment.clone(),
                        Column::with_children(vec![
                            label!("Download {}", attachment.name)
                                .size(DEF_SIZE - 4)
                                .into(),
                            thumbnail_image.into(),
                        ])
                        .spacing(SPACING),
                        media_open_button_state,
                        theme,
                    );

                    message_body_widgets.push(button);
                }
            } else {
                let text = if does_content_exist {
                    "Open"
                } else {
                    "Download"
                };

                message_body_widgets.push(create_button(
                    is_thumbnail,
                    attachment.clone(),
                    label!("{} {}", text, attachment.name),
                    media_open_button_state,
                    theme,
                ));
            }
        }

        let msg_body = column(message_body_widgets)
            .align_items(align!(|<))
            .padding(0)
            .spacing(MSG_LR_PADDING);
        let mut message_row = Vec::with_capacity(2);

        let maybe_timestamp = if is_sender_different
            || last_timestamp.minute() != message.timestamp.minute()
        {
            let message_timestamp = message.timestamp.format("%H:%M").to_string();

            let timestamp_label = label!(message_timestamp)
                .size(MESSAGE_TIMESTAMP_SIZE)
                .color(color!(160, 160, 160));

            Column::with_children(vec![
                space!(h = PADDING / 8).into(),
                Row::with_children(vec![timestamp_label.into(), space!(h = PADDING / 2).into()])
                    .into(),
            ])
            .into()
        } else {
            space!(w = PADDING * 2 - (PADDING / 4 + PADDING / 16)).into()
        };
        message_row.push(maybe_timestamp);
        let mut but = Button::new(edit_but_state, msg_body)
            .padding(1)
            .style(theme.embed());
        if current_user_id == message.sender && Mode::EditMessage == mode {
            if let Some(id) = message.id.id() {
                but = but.on_press(Message::ChangeMode(Mode::EditingMessage(id)));
            }
        }
        message_row.push(but.into());

        message_group.push(row(message_row).align_items(align!(|<)).padding(0).into());

        last_sender_id = Some(id_to_use);
        last_sender_name = Some(sender_display_name);
        last_timestamp = message.timestamp;
    }
    if !message_group.is_empty() {
        event_history = event_history.push(
            Container::new(column(message_group.drain(..).collect()).align_items(align!(|<)))
                .style(theme.round()),
        );
    }
    event_history.into()
}
