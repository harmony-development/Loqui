use crate::{
    client::{
        channel::Channel,
        content::{ContentStore, ContentType, ThumbnailCache},
        member::Members,
    },
    color, label, space,
    ui::{
        component::*,
        screen::main::Message,
        style::{
            Theme, ALT_COLOR, AVATAR_WIDTH, DATE_SEPERATOR_SIZE, MESSAGE_SENDER_SIZE, MESSAGE_SIZE,
            MESSAGE_TIMESTAMP_SIZE, PADDING, SPACING,
        },
    },
};
use chrono::{Datelike, Timelike};
use harmony_rust_sdk::{api::harmonytypes::r#override::Reason, client::api::rest::FileId};

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

    for (message, media_open_button_state) in displayable_events
        .iter()
        .zip(content_open_buttons.iter_mut())
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
                Container::new(
                    label!("[{}]", sender_display_name)
                        .color(theme.calculate_sender_color(sender_display_name.len()))
                        .size(MESSAGE_SENDER_SIZE),
                )
                .align_x(align!(|<))
                .align_y(align!(>|))
                .into(),
            );

            if let Some(reason) = &override_reason {
                widgets.push(
                    Container::new(label!(reason).color(ALT_COLOR).size(MESSAGE_SIZE))
                        .align_x(align!(|<))
                        .align_y(align!(>|))
                        .into(),
                );
            }

            row(widgets).spacing(MSG_LR_PADDING).padding(0)
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

        if !is_sender_different
            && !message_group.is_empty()
            && last_timestamp.signed_duration_since(message.timestamp)
                > chrono::Duration::minutes(5)
        {
            event_history = event_history.push(
                Container::new(column(message_group.drain(..).collect()).align_items(align!(|<)))
                    .style(theme.round()),
            );
            if message.timestamp.day() != last_timestamp.day() {
                let date_time_seperator = fill_container(
                    label!(message.timestamp.format("[%d %B %Y]").to_string())
                        .size(DATE_SEPERATOR_SIZE)
                        .color(color!(153, 153, 153)),
                )
                .height(length!(-));

                event_history = event_history.push(date_time_seperator);
            }
            message_group.push(sender_body_creator(&sender_display_name).into());
        }

        let mut message_text = label!(&message.content).size(MESSAGE_SIZE);

        if !message.id.is_ack() {
            message_text = message_text.color(color!(200, 200, 200));
        }

        let mut message_body_widgets = vec![message_text.into()];

        if let Some(attachment) = message.attachments.first() {
            fn create_button<'a>(
                is_thumbnail: bool,
                content_url: FileId,
                content: impl Into<Element<'a, Message>>,
                button_state: &'a mut button::State,
                theme: Theme,
            ) -> Element<'a, Message> {
                Button::new(button_state, content.into())
                    .on_press(Message::OpenContent {
                        content_url,
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
                        attachment.id.clone(),
                        thumbnail_image,
                        media_open_button_state,
                        theme,
                    ));
                } else {
                    let button = create_button(
                        is_thumbnail,
                        attachment.id.clone(),
                        Column::with_children(vec![
                            label!("Download content").into(),
                            thumbnail_image.into(),
                        ]),
                        media_open_button_state,
                        theme,
                    );

                    message_body_widgets.push(button);
                }
            } else {
                let text = if does_content_exist {
                    "Open content"
                } else {
                    "Download content"
                };

                message_body_widgets.push(create_button(
                    is_thumbnail,
                    attachment.id.clone(),
                    label!(text),
                    media_open_button_state,
                    theme,
                ));
            }
        }

        let msg_body = column(message_body_widgets)
            .padding(0)
            .spacing(MSG_LR_PADDING)
            .into();
        let mut message_row = Vec::with_capacity(2);

        if is_sender_different || last_timestamp.minute() != message.timestamp.minute() {
            let message_timestamp = message.timestamp.format("%H:%M").to_string();

            let timestamp_label = label!(message_timestamp)
                .size(MESSAGE_TIMESTAMP_SIZE)
                .color(color!(160, 160, 160));

            message_row.push(
                Column::with_children(vec![
                    space!(h = PADDING / 8).into(),
                    Row::with_children(vec![
                        timestamp_label.into(),
                        space!(h = PADDING / 4).into(),
                    ])
                    .into(),
                ])
                .into(),
            );
        }
        message_row.push(msg_body);

        message_group.push(row(message_row).padding(0).into());

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
