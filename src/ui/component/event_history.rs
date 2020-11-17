use crate::{
    client::{
        media::ContentType,
        media::{content_exists, ThumbnailStore},
        Room,
    },
    ui::{
        screen::main::Message,
        style::{DarkButton, Theme},
    },
};
use chrono::{DateTime, Datelike, Local};
use iced::{
    button, scrollable, Align, Button, Color, Column, Container, Element, Image, Length, Row,
    Scrollable, Space, Text,
};
use ruma::{api::exports::http::Uri, UserId};
use std::time::{Duration, Instant};

pub const SHOWN_MSGS_LIMIT: usize = 32; // for only one half

#[allow(clippy::mutable_key_type)]
pub fn build_event_history<'a>(
    thumbnail_store: &ThumbnailStore,
    room: &Room,
    current_user_id: &UserId,
    looking_at_event: usize,
    scrollable_state: &'a mut scrollable::State,
    content_open_buttons: &'a mut Vec<button::State>,
    theme: Theme,
) -> Element<'a, Message> {
    let mut event_history = Scrollable::new(scrollable_state)
        .on_scroll(Message::MessageHistoryScrolled)
        .snap_to_bottom(true)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme)
        .align_items(Align::Start)
        .spacing(8)
        .padding(16);

    let timeline_range_end = looking_at_event
        .saturating_add(SHOWN_MSGS_LIMIT)
        .min(room.displayable_events().len());
    let timeline_range_start = timeline_range_end.saturating_sub(SHOWN_MSGS_LIMIT);
    let displayable_events = &room.displayable_events()[timeline_range_start..timeline_range_end];

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

        let sender_display_name = room.get_user_display_name(id_to_use);
        let sender_body_creator = |sender_display_name: &str| {
            Text::new(format!("[{}]", sender_display_name))
                .color(theme.calculate_sender_color(id_to_use.localpart().len()))
                .size(19)
        };

        let mut is_sender_different = false;
        if last_sender != Some(id_to_use) {
            is_sender_different = true;
            if !message_group.is_empty() {
                event_history = event_history.push(
                    Container::new(
                        Column::with_children(message_group.drain(..).collect())
                            .align_items(Align::Start)
                            .padding(16)
                            .spacing(4),
                    )
                    .style(crate::ui::style::RoundContainer),
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
                        Column::with_children(message_group.drain(..).collect())
                            .align_items(Align::Start)
                            .padding(16)
                            .spacing(4),
                    )
                    .style(crate::ui::style::RoundContainer),
                );
                let cur_time_date =
                    DateTime::<Local>::from(*timeline_event.origin_server_timestamp());
                let time_date = DateTime::<Local>::from(last_timestamp);
                if cur_time_date.day() != time_date.day() {
                    let date_time_seperator = Container::new(
                        Text::new(cur_time_date.format("[%d %B %Y]").to_string())
                            .size(22)
                            .color(Color::from_rgb(0.6, 0.6, 0.6)),
                    )
                    .center_x()
                    .center_y()
                    .height(Length::Shrink)
                    .width(Length::Fill);

                    event_history = event_history.push(date_time_seperator);
                }
                message_group.push(sender_body_creator(&sender_display_name).into());
            }
        }

        let mut message_text = Text::new(timeline_event.formatted(room)).size(16);

        if !timeline_event.is_ack() {
            message_text = message_text.color(Color::from_rgb(0.5, 0.5, 0.5));
        } else if timeline_event.is_state() {
            message_text = message_text.color(Color::from_rgb8(200, 200, 200));
        } else if timeline_event.is_redacted_message() {
            message_text = message_text.color(Color::from_rgb8(200, 0, 0));
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
            ) -> Element<'a, Message> {
                Button::new(button_state, content.into())
                    .on_press(Message::OpenContent(content_url, is_thumbnail))
                    .style(DarkButton)
                    .into()
            };

            let is_thumbnail = matches!(content_type, ContentType::Image);
            let does_content_exist = content_exists(&content_url);

            if let Some(thumbnail_image) = {
                if is_thumbnail {
                    Some(content_url.clone())
                } else {
                    timeline_event.thumbnail_url()
                }
            }
            .map(|thumbnail_url| {
                thumbnail_store
                    .get_thumbnail(&thumbnail_url)
                    .map(|handle| Image::new(handle.clone()).width(Length::Fill))
            })
            .flatten()
            {
                if does_content_exist {
                    message_body_widgets.push(create_button(
                        is_thumbnail,
                        content_url,
                        thumbnail_image.width(Length::Units(360)),
                        media_open_button_state,
                    ));
                } else {
                    let button = create_button(
                        is_thumbnail,
                        content_url,
                        Column::with_children(vec![
                            Text::new("Download content").into(),
                            thumbnail_image.width(Length::Units(360)).into(),
                        ]),
                        media_open_button_state,
                    );

                    message_body_widgets.push(button);
                }
            } else {
                let button_label = Text::new(if does_content_exist {
                    "Open content"
                } else {
                    "Download content"
                });
                message_body_widgets.push(create_button(
                    is_thumbnail,
                    content_url,
                    button_label,
                    media_open_button_state,
                ));
            }
        }

        let mut message_row = vec![Column::with_children(message_body_widgets)
            .align_items(Align::Start)
            .spacing(4)
            .into()];

        // FIXME: doesnt work properly
        let cur_minute = (cur_timestamp.as_secs() / 60) % 60;
        if is_sender_different || last_minute != cur_minute {
            last_minute = cur_minute;
            let message_timestamp = Text::new(
                DateTime::<Local>::from(*timeline_event.origin_server_timestamp())
                    .format("%H:%M")
                    .to_string(),
            )
            .size(14)
            .color(Color::from_rgb8(160, 160, 160));
            message_row.insert(0, Container::new(message_timestamp).padding(2).into());
        } else {
            message_row.insert(0, Space::with_width(Length::Units(39)).into());
        }

        message_group.push(
            Row::with_children(message_row)
                .align_items(Align::Start)
                .spacing(8)
                .into(),
        );

        last_sender = Some(id_to_use);
        last_timestamp = *timeline_event.origin_server_timestamp();
    }
    if !message_group.is_empty() {
        event_history = event_history.push(
            Container::new(
                Column::with_children(message_group.drain(..).collect())
                    .align_items(Align::Start)
                    .padding(16)
                    .spacing(4),
            )
            .style(crate::ui::style::RoundContainer),
        );
    }
    event_history.into()
}
