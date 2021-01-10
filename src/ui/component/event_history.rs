use crate::{
    client::{
        content::{ContentStore, ContentType, ThumbnailCache},
        Room,
    },
    color,
    ui::{
        component::*,
        screen::main::Message,
        style::{
            Theme, AVATAR_WIDTH, DATE_SEPERATOR_SIZE, ERROR_COLOR, MESSAGE_SENDER_SIZE,
            MESSAGE_SIZE, MESSAGE_TIMESTAMP_SIZE, PADDING, SPACING,
        },
    },
};
use chrono::{DateTime, Datelike, Local};
use ruma::{api::exports::http::Uri, UserId};
use std::time::{Duration, Instant};

pub const SHOWN_MSGS_LIMIT: usize = 32;
const MSG_LR_PADDING: u16 = SPACING * 2;

#[allow(clippy::mutable_key_type)]
#[allow(clippy::clippy::too_many_arguments)]
pub fn build_event_history<'a>(
    content_store: &ContentStore,
    thumbnail_cache: &ThumbnailCache,
    room: &Room,
    current_user_id: &UserId,
    looking_at_event: usize,
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
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme)
        .align_items(Align::Start)
        .spacing(SPACING * 2)
        .padding(PADDING);

    let members = room.members();

    let displayable_events = room.displayable_events().collect::<Vec<_>>();
    let timeline_range_end = looking_at_event
        .saturating_add(SHOWN_MSGS_LIMIT)
        .min(displayable_events.len());
    let timeline_range_start = timeline_range_end.saturating_sub(SHOWN_MSGS_LIMIT);
    let displayable_events = &displayable_events[timeline_range_start..timeline_range_end];

    let mut last_timestamp = if let Some(ev) = displayable_events.first() {
        *ev.origin_server_timestamp()
    } else {
        return event_history.into();
    };
    let mut last_sender = None;
    let mut last_minute = last_timestamp
        .elapsed()
        .unwrap_or_else(|_| Instant::now().elapsed())
        .as_secs()
        / 60;
    let mut message_group = vec![];

    for (timeline_event, media_open_button_state) in displayable_events
        .iter()
        .zip(content_open_buttons.iter_mut())
    {
        let cur_timestamp = timeline_event
            .origin_server_timestamp()
            .elapsed()
            .unwrap_or_else(|_| Instant::now().elapsed());
        let id_to_use = if !timeline_event.is_ack() {
            current_user_id
        } else {
            timeline_event.sender()
        };

        let sender_display_name = members.get_user_display_name(id_to_use);
        let sender_avatar_url = members
            .get_member(id_to_use)
            .map(|m| m.avatar_url())
            .flatten();
        let sender_body_creator = |sender_display_name: &str| {
            let mut widgets = Vec::with_capacity(2);

            if let Some(handle) = sender_avatar_url
                .map(|u| thumbnail_cache.get_thumbnail(u))
                .flatten()
                .cloned()
            {
                // TODO: Add `border_radius` styling for `Image` so we can use it here
                widgets.push(Image::new(handle).width(Length::Units(AVATAR_WIDTH)).into());
            }

            widgets.push(
                Container::new(
                    label(format!("[{}]", sender_display_name))
                        .color(theme.calculate_sender_color(sender_display_name.len()))
                        .size(MESSAGE_SENDER_SIZE),
                )
                .align_y(Align::End)
                .into(),
            );

            row(widgets).spacing(MSG_LR_PADDING).padding(0)
        };

        let mut is_sender_different = false;
        if last_sender != Some(id_to_use) {
            is_sender_different = true;
            if !message_group.is_empty() {
                event_history = event_history.push(
                    Container::new(
                        column(message_group.drain(..).collect()).align_items(Align::Start),
                    )
                    .style(theme.round()),
                );
            }
            message_group.push(sender_body_creator(&sender_display_name).into());
        }

        if !is_sender_different {
            let time = last_timestamp
                .elapsed()
                .unwrap_or_else(|_| Instant::now().elapsed());
            if !message_group.is_empty()
                && time.checked_sub(cur_timestamp).unwrap_or_default() > Duration::from_secs(60 * 5)
            {
                event_history = event_history.push(
                    Container::new(
                        column(message_group.drain(..).collect()).align_items(Align::Start),
                    )
                    .style(theme.round()),
                );
                let cur_time_date =
                    DateTime::<Local>::from(*timeline_event.origin_server_timestamp());
                let time_date = DateTime::<Local>::from(last_timestamp);
                if cur_time_date.day() != time_date.day() {
                    let date_time_seperator = fill_container(
                        label(cur_time_date.format("[%d %B %Y]").to_string())
                            .size(DATE_SEPERATOR_SIZE)
                            .color(color!(153, 153, 153)),
                    )
                    .height(Length::Shrink);

                    event_history = event_history.push(date_time_seperator);
                }
                message_group.push(sender_body_creator(&sender_display_name).into());
            }
        }

        let mut message_text = label(timeline_event.formatted(&members)).size(MESSAGE_SIZE);

        if !timeline_event.is_ack() {
            message_text = message_text.color(color!(128, 128, 128));
        } else if timeline_event.is_state() {
            message_text = message_text.color(color!(200, 200, 200));
        } else if timeline_event.is_redacted_message() {
            message_text = message_text.color(ERROR_COLOR);
        }

        let mut message_body_widgets = vec![message_text.into()];

        if let (Some(content_url), Some(content_type)) =
            (timeline_event.content_url(), timeline_event.content_type())
        {
            fn create_button<'a>(
                is_thumbnail: bool,
                content_url: Uri,
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

            let is_thumbnail = matches!(content_type, ContentType::Image);
            let does_content_exist = content_store.content_exists(&content_url);

            if let Some(thumbnail_image) = thumbnail_cache
                .get_thumbnail(&content_url)
                .or_else(|| {
                    timeline_event
                        .thumbnail_url()
                        .map(|url| thumbnail_cache.get_thumbnail(&url))
                        .flatten()
                })
                // FIXME: Don't hardcode this length, calculate it using the size of the window
                .map(|handle| Image::new(handle.clone()).width(Length::Units(320)))
            {
                if does_content_exist {
                    message_body_widgets.push(create_button(
                        is_thumbnail,
                        content_url,
                        thumbnail_image,
                        media_open_button_state,
                        theme,
                    ));
                } else {
                    let button = create_button(
                        is_thumbnail,
                        content_url,
                        Column::with_children(vec![
                            label("Download content").into(),
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
                    content_url,
                    label(text),
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

        // FIXME: doesnt work properly
        let cur_minute = (cur_timestamp.as_secs() / 60) % 60;
        if is_sender_different || last_minute != cur_minute {
            last_minute = cur_minute;
            let message_timestamp =
                DateTime::<Local>::from(*timeline_event.origin_server_timestamp())
                    .format("%H:%M")
                    .to_string();

            let timestamp_label = label(message_timestamp)
                .size(MESSAGE_TIMESTAMP_SIZE)
                .color(color!(160, 160, 160));

            message_row.push(
                Column::with_children(vec![
                    ahspace(PADDING / 8).into(),
                    Row::with_children(vec![timestamp_label.into(), ahspace(PADDING / 4).into()])
                        .into(),
                ])
                .into(),
            );
        }
        message_row.push(msg_body);

        message_group.push(row(message_row).padding(0).into());

        last_sender = Some(id_to_use);
        last_timestamp = *timeline_event.origin_server_timestamp();
    }
    if !message_group.is_empty() {
        event_history = event_history.push(
            Container::new(column(message_group.drain(..).collect()).align_items(Align::Start))
                .style(theme.round()),
        );
    }
    event_history.into()
}
