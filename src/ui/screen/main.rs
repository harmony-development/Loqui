use crate::{
    client::{
        media::ThumbnailStore,
        media::{make_content_folder, make_content_path, ImageHandle},
        Client, ClientError, TimelineEvent,
    },
    ui::{
        component::{build_event_history, build_room_list, event_history::SHOWN_MSGS_LIMIT},
        style::{BrightContainer, DarkButton, DarkTextInput, Theme},
    },
};
use iced::{
    button, scrollable, text_input, Align, Button, Color, Column, Command, Container, Element,
    Length, Row, Space, Subscription, Text, TextInput,
};
use iced_futures::BoxStream;
use ruma::{
    api::{
        client::r0::{context::get_context, message::send_message_event, sync::sync_events},
        exports::http::Uri,
    },
    events::{room::message::MessageEventContent, AnyMessageEventContent},
    presence::PresenceState,
    EventId, RoomId,
};
use std::{collections::HashMap, hash::Hash, hash::Hasher, path::PathBuf, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Message {
    /// Sent when the user wants to send a message.
    SendMessage,
    SendFile,
    /// Sent when user makes a change to the message they are composing.
    MessageChanged(String),
    ScrollToBottom,
    OpenContent(Uri, bool),
    DownloadedThumbnail {
        thumbnail_url: Uri,
        thumbnail: ImageHandle,
    },
    /// Sent when the user selects a different room.
    RoomChanged(RoomId),
    /// Sent when the user scrolls the message history.
    MessageHistoryScrolled(f32),
    /// Sent when the user clicks the logout button.
    LogoutInitiated,
    LogoutConfirmation(bool),
    /// Sent when a `main::Message::SendMessage` message returns a `LimitExceeded` error.
    /// This is used to retry sending a message. The duration is
    /// the "retry after" time. The UUID is the transaction ID of
    /// this message, used to identify which message to re-send.
    /// The room ID is the ID of the room in which the message is
    /// stored.
    RetrySendMessage {
        retry_after: Duration,
        transaction_id: Uuid,
        room_id: RoomId,
    },
    /// Sent when a sync response is received from the server.
    MatrixSyncResponse(Box<sync_events::Response>),
    /// Sent when a "get context" (get events around an event) is received from the server.
    MatrixGetEventsAroundResponse(Box<get_context::Response>),
}

pub struct MainScreen {
    composer_state: text_input::State,
    scroll_to_bottom_but_state: button::State,
    send_file_but_state: button::State,
    event_history_state: scrollable::State,
    rooms_list_state: scrollable::State,
    rooms_buts_state: Vec<button::State>,
    content_open_buts_state: Vec<button::State>,
    logout_but_state: button::State,
    logout_approve_but_state: button::State,
    logout_cancel_but_state: button::State,

    /// `Some(confirmation)` if there is an ongoing logout request, `None` otherwise.
    /// `confirmation` is `true` if the user approves the logout, `false` otherwise.
    logging_out: Option<bool>,
    /// `None` if the user didn't select a room, `Some(room_id)` otherwise.
    current_room_id: Option<RoomId>,
    looking_at_event: HashMap<RoomId, usize>,
    /// The previous scrolled percentage of the message history list.
    /// Used to check if it's fine to request older / newer events from the server.
    prev_scroll_perc: f32,
    // TODO: move client to `ScreenManager` as an `Option` so we can keep the client between screens
    client: Client,
    /// The message the user is currently typing.
    message: String,
    thumbnail_store: ThumbnailStore,
}

impl MainScreen {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            composer_state: Default::default(),
            scroll_to_bottom_but_state: Default::default(),
            send_file_but_state: Default::default(),
            event_history_state: Default::default(),
            rooms_list_state: Default::default(),
            rooms_buts_state: Default::default(),
            content_open_buts_state: vec![Default::default(); SHOWN_MSGS_LIMIT],
            logout_but_state: Default::default(),
            logout_approve_but_state: Default::default(),
            logout_cancel_but_state: Default::default(),
            logging_out: None,
            current_room_id: None,
            looking_at_event: Default::default(),
            prev_scroll_perc: 0.0,
            message: Default::default(),
            thumbnail_store: ThumbnailStore::new(),
        }
    }

    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if let Some(confirmation) = self.logging_out {
            return if confirmation {
                Container::new(Text::new("Logging out...").size(30))
                    .center_y()
                    .center_x()
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme)
                    .into()
            } else {
                let logout_confirm_panel = Column::with_children(
                    vec![
                        Text::new("Do you want to logout?").into(),
                        Text::new("This will delete your current session and you will need to login with your password.")
                            .color(Color::from_rgb(1.0, 0.0, 0.0))
                            .into(),
                        Row::with_children(
                            vec![
                                Space::with_width(Length::FillPortion(2)).into(),
                                Button::new(&mut self.logout_approve_but_state, Container::new(Text::new("Yes")).width(Length::Fill).center_x())
                                    .width(Length::FillPortion(1))
                                    .on_press(Message::LogoutConfirmation(true))
                                    .style(theme)
                                    .into(),
                                Space::with_width(Length::FillPortion(1)).into(),
                                Button::new(&mut self.logout_cancel_but_state, Container::new(Text::new("No")).width(Length::Fill).center_x())
                                    .width(Length::FillPortion(1))
                                    .on_press(Message::LogoutConfirmation(false))
                                    .style(theme)
                                    .into(),
                                Space::with_width(Length::FillPortion(2)).into(),
                        ])
                        .width(Length::Fill)
                        .align_items(Align::Center)
                        .into(),
                    ])
                    .align_items(Align::Center)
                    .spacing(12);

                let padded_panel = Row::with_children(vec![
                    Space::with_width(Length::FillPortion(3)).into(),
                    logout_confirm_panel.width(Length::FillPortion(4)).into(),
                    Space::with_width(Length::FillPortion(3)).into(),
                ])
                .height(Length::Fill)
                .align_items(Align::Center);

                Container::new(padded_panel)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme)
                    .into()
            };
        }

        for (room_id, index) in self.looking_at_event.drain().collect::<Vec<_>>() {
            if self
                .client
                .rooms()
                .keys()
                .any(|other_room_id| other_room_id == &room_id)
            {
                self.looking_at_event.insert(room_id, index);
            }
        }
        for (room_id, room) in self.client.rooms() {
            if !self.looking_at_event.keys().any(|id| id == room_id) {
                self.looking_at_event.insert(
                    room_id.clone(),
                    room.displayable_events().len().saturating_sub(1),
                );
            }
        }

        let rooms = self.client.rooms();

        if rooms.len() != self.rooms_buts_state.len() {
            self.rooms_buts_state = vec![Default::default(); rooms.len()];
        }

        let room_list = build_room_list(
            rooms,
            self.current_room_id.as_ref(),
            &mut self.rooms_list_state,
            self.rooms_buts_state.as_mut_slice(),
            Message::RoomChanged,
            theme,
        );

        let logout = Button::new(&mut self.logout_but_state, Text::new("Logout").size(16))
            .width(Length::Fill)
            .on_press(Message::LogoutInitiated)
            .style(DarkButton);

        let user_name = Text::new(self.client.current_user_id().localpart())
            .size(16)
            .width(Length::Fill);
        let user_area =
            Row::with_children(vec![logout.into(), user_name.into()]).align_items(Align::Center);

        let rooms_area =
            Column::with_children(vec![room_list, user_area.width(Length::Fill).into()]);

        let mut screen_widgets = vec![Container::new(rooms_area)
            .width(Length::Units(250))
            .height(Length::Fill)
            .style(theme)
            .into()];

        if let Some(room_id) = self.current_room_id.as_ref() {
            if let Some(room) = rooms.get(room_id) {
                let message_composer = TextInput::new(
                    &mut self.composer_state,
                    "Enter your message here...",
                    self.message.as_str(),
                    Message::MessageChanged,
                )
                .padding(12)
                .size(16)
                .style(DarkTextInput)
                .on_submit(Message::SendMessage);

                let current_user_id = self.client.current_user_id();
                let room_disp_len = room.displayable_events().len();

                let message_history_list = build_event_history(
                    &self.thumbnail_store,
                    room,
                    &current_user_id,
                    self.looking_at_event
                        .get(room_id)
                        .copied()
                        .unwrap_or_else(|| room_disp_len.saturating_sub(1)),
                    &mut self.event_history_state,
                    &mut self.content_open_buts_state,
                    theme,
                );

                let mut typing_users_combined = String::new();
                let mut typing_members = room.typing_members();
                // Remove own user id from the list (if its there)
                if let Some(index) = typing_members.iter().position(|id| *id == &current_user_id) {
                    typing_members.remove(index);
                }
                let typing_members_count = typing_members.len();

                for (index, member_id) in typing_members.iter().enumerate() {
                    if index > 2 {
                        typing_users_combined += " and others are typing...";
                        break;
                    }

                    typing_users_combined += room.get_user_display_name(member_id).as_str();

                    typing_users_combined += match typing_members_count {
                        x if x > index + 1 => ", ",
                        1 => " is typing...",
                        _ => " are typing...",
                    };
                }

                let typing_users = Column::with_children(vec![
                    Space::with_width(Length::Units(6)).into(),
                    Row::with_children(vec![
                        Space::with_width(Length::Units(9)).into(),
                        Text::new(typing_users_combined).size(14).into(),
                    ])
                    .into(),
                ]);

                let send_file_button =
                    Button::new(&mut self.send_file_but_state, Text::new("↑").size(28))
                        .style(DarkButton)
                        .on_press(Message::SendFile);

                let mut bottom_area_widgets = vec![
                    send_file_button.into(),
                    message_composer.width(Length::Fill).into(),
                ];

                // This unwrap is safe since we add the room to the map before this
                if *self.looking_at_event.get(room_id).unwrap()
                    < room_disp_len.saturating_sub(SHOWN_MSGS_LIMIT)
                {
                    bottom_area_widgets.push(
                        Button::new(
                            &mut self.scroll_to_bottom_but_state,
                            Text::new("↡").size(28),
                        )
                        .style(DarkButton)
                        .on_press(Message::ScrollToBottom)
                        .into(),
                    );
                }

                let message_area = Column::with_children(vec![
                    message_history_list,
                    typing_users.into(),
                    Container::new(
                        Row::with_children(bottom_area_widgets)
                            .spacing(8)
                            .width(Length::Fill),
                    )
                    .width(Length::Fill)
                    .padding(8)
                    .into(),
                ]);

                screen_widgets.push(
                    Container::new(message_area)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(BrightContainer)
                        .into(),
                );
            }
        }

        // We know that there will be only one widget if the user isn't looking at a room currently
        if screen_widgets.len() < 2 {
            let in_no_room_warning = Container::new(
                Text::new("Select / join a room to start chatting!")
                    .size(35)
                    .color(Color::from_rgb(0.5, 0.5, 0.5)),
            )
            .center_x()
            .center_y()
            .style(BrightContainer);

            screen_widgets.push(
                in_no_room_warning
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
            );
        }

        Row::with_children(screen_widgets)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    pub fn update(&mut self, msg: Message) -> Command<super::Message> {
        fn process_send_message_result(
            result: Result<send_message_event::Response, ClientError>,
            transaction_id: Uuid,
            room_id: RoomId,
        ) -> super::Message {
            use ruma::{api::client::error::ErrorKind as ClientAPIErrorKind, api::error::*};
            use ruma_client::Error as InnerClientError;

            match result {
                Ok(_) => super::Message::Nothing,
                Err(err) => {
                    if let ClientError::Internal(InnerClientError::FromHttpResponse(
                        FromHttpResponseError::Http(ServerError::Known(err)),
                    )) = &err
                    {
                        if let ClientAPIErrorKind::LimitExceeded {
                            retry_after_ms: Some(retry_after),
                        } = err.kind
                        {
                            return super::Message::MainScreen(Message::RetrySendMessage {
                                retry_after,
                                transaction_id,
                                room_id,
                            });
                        }
                    }
                    super::Message::MatrixError(Box::new(err))
                }
            }
        }

        fn make_get_events_around_command(
            inner: ruma_client::Client,
            room_id: RoomId,
            event_id: EventId,
        ) -> Command<super::Message> {
            Command::perform(
                Client::get_events_around(inner, room_id, event_id),
                |result| match result {
                    Ok(response) => super::Message::MainScreen(
                        Message::MatrixGetEventsAroundResponse(Box::new(response)),
                    ),
                    Err(err) => super::Message::MatrixError(Box::new(err)),
                },
            )
        }

        fn make_download_content_com(
            inner: ruma_client::Client,
            content_url: Uri,
        ) -> Command<super::Message> {
            Command::perform(
                async move {
                    let download_result =
                        Client::download_content(inner, content_url.clone()).await;

                    match download_result {
                        Ok(raw_data) => {
                            let path = make_content_path(&content_url);
                            let server_media_dir = make_content_folder(&content_url);
                            tokio::fs::create_dir_all(server_media_dir).await?;
                            tokio::fs::write(path, raw_data.as_slice())
                                .await
                                .map(|_| (content_url, raw_data))
                                .map_err(|e| e.into())
                        }
                        Err(err) => Err(err),
                    }
                },
                |result| match result {
                    Ok((content_url, raw_data)) => {
                        super::Message::MainScreen(Message::DownloadedThumbnail {
                            thumbnail_url: content_url,
                            thumbnail: ImageHandle::from_memory(raw_data),
                        })
                    }
                    Err(err) => super::Message::MatrixError(Box::new(err)),
                },
            )
        }

        fn make_read_thumbnail_com(thumbnail_url: Uri) -> Command<super::Message> {
            Command::perform(
                async move {
                    (
                        async {
                            Ok(ImageHandle::from_memory(
                                tokio::fs::read(make_content_path(&thumbnail_url)).await?,
                            ))
                        }
                        .await,
                        thumbnail_url,
                    )
                },
                |(result, thumbnail_url)| match result {
                    Ok(thumbnail) => super::Message::MainScreen(Message::DownloadedThumbnail {
                        thumbnail,
                        thumbnail_url,
                    }),
                    Err(err) => super::Message::MatrixError(Box::new(err)),
                },
            )
        }

        fn scroll_to_bottom(screen: &mut MainScreen, room_id: RoomId) {
            if let Some(disp) = screen
                .client
                .get_room(&room_id)
                .map(|room| room.displayable_events().len())
            {
                screen
                    .looking_at_event
                    .entry(room_id)
                    .and_modify(|d| *d = disp.saturating_sub(1))
                    .or_insert_with(|| disp.saturating_sub(1));
            }
        }

        match msg {
            Message::MessageHistoryScrolled(scroll_perc) => {
                if scroll_perc < 0.01 && scroll_perc <= self.prev_scroll_perc {
                    if let Some((Some(disp), Some(looking_at_event))) =
                        self.current_room_id.clone().map(|id| {
                            (
                                self.client
                                    .get_room(&id)
                                    .map(|room| room.displayable_events().len()),
                                self.looking_at_event.get_mut(&id),
                            )
                        })
                    {
                        if *looking_at_event == disp.saturating_sub(1) {
                            *looking_at_event = disp.saturating_sub(SHOWN_MSGS_LIMIT + 1);
                        } else {
                            *looking_at_event = looking_at_event.saturating_sub(1);
                        }

                        if *looking_at_event < 2 {
                            if let Some(Some((Some(event), room_id))) =
                                self.current_room_id.as_ref().map(|id| {
                                    self.client
                                        .get_room(id)
                                        .map(|room| (room.oldest_event(), id))
                                })
                            {
                                let inner = self.client.inner();
                                let room_id = room_id.clone();
                                let event_id = event.id().clone();
                                self.prev_scroll_perc = scroll_perc;
                                return make_get_events_around_command(inner, room_id, event_id);
                            }
                        }
                    }
                } else if scroll_perc > 0.99 && scroll_perc >= self.prev_scroll_perc {
                    if let Some((Some(disp), Some(looking_at_event))) =
                        self.current_room_id.clone().map(|id| {
                            (
                                self.client
                                    .get_room(&id)
                                    .map(|room| room.displayable_events().len()),
                                self.looking_at_event.get_mut(&id),
                            )
                        })
                    {
                        if *looking_at_event > disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                            *looking_at_event = disp.saturating_sub(1);
                        } else {
                            *looking_at_event = looking_at_event.saturating_add(1).min(disp);
                        }
                    }
                }

                self.prev_scroll_perc = scroll_perc;
            }
            Message::LogoutInitiated => {
                self.logging_out = Some(false);
            }
            Message::LogoutConfirmation(confirmation) => {
                if confirmation {
                    self.logging_out = Some(true);
                    let inner = self.client.inner();
                    return Command::perform(Client::logout(inner), |result| match result {
                        Ok(_) => super::Message::LogoutComplete,
                        Err(err) => super::Message::MatrixError(Box::new(err)),
                    });
                } else {
                    self.logging_out = None;
                }
            }
            Message::MessageChanged(new_msg) => {
                self.message = new_msg;

                if let Some(room_id) = self.current_room_id.as_ref() {
                    let inner = self.client.inner();
                    return Command::perform(
                        Client::send_typing(inner, room_id.clone(), self.client.current_user_id()),
                        |result| match result {
                            Ok(_) => super::Message::Nothing,
                            Err(err) => super::Message::MatrixError(Box::new(err)),
                        },
                    );
                }
            }
            Message::ScrollToBottom => {
                if let Some(room_id) = self.current_room_id.clone() {
                    scroll_to_bottom(self, room_id);
                    self.prev_scroll_perc = 1.0;
                    self.event_history_state.scroll_to_bottom();
                }
            }
            Message::DownloadedThumbnail {
                thumbnail_url,
                thumbnail,
            } => {
                self.thumbnail_store.put_thumbnail(thumbnail_url, thumbnail);
            }
            Message::OpenContent(content_url, is_thumbnail) => {
                let process_path_result = |result| match result {
                    Ok(path) => {
                        open::that_in_background(path);
                        super::Message::Nothing
                    }
                    Err(err) => super::Message::MatrixError(Box::new(err)),
                };
                let path = make_content_path(&content_url);
                return if path.exists() {
                    Command::perform(async move { Ok(path) }, process_path_result)
                } else {
                    let inner = self.client.inner();
                    Command::perform(
                        async move {
                            let download_result =
                                Client::download_content(inner, content_url.clone()).await;

                            match download_result {
                                Ok(raw_data) => {
                                    let path = make_content_path(&content_url);
                                    let server_media_dir = make_content_folder(&content_url);
                                    tokio::fs::create_dir_all(server_media_dir).await?;
                                    tokio::fs::write(&path, raw_data.as_slice()).await?;
                                    Ok(if is_thumbnail {
                                        Some((path, content_url, raw_data))
                                    } else {
                                        None
                                    })
                                }
                                Err(err) => Err(err),
                            }
                        },
                        |result| match result {
                            Ok(data) => {
                                if let Some((path, content_url, raw_data)) = data {
                                    open::that_in_background(path);
                                    super::Message::MainScreen(Message::DownloadedThumbnail {
                                        thumbnail_url: content_url,
                                        thumbnail: ImageHandle::from_memory(raw_data),
                                    })
                                } else {
                                    super::Message::Nothing
                                }
                            }
                            Err(err) => super::Message::MatrixError(Box::new(err)),
                        },
                    )
                };
            }
            Message::SendFile => {
                /*
                TODO: Investigate implementing a file picker widget for iced
                   (we just put it in as an overlay)
                TODO: actually implement this
                    1. Detect what type of file this is (and create a thumbnail if it's a video / image)
                    2. Upload the file to matrix (and the thumbnail if there is one)
                    3. Hardlink the source to our cache (or copy if FS doesn't support)
                        - this is so that even if the user deletes the file it will be in our cache
                        - (and we won't need to download it again)
                    4. Create `MessageEventContent::Image(ImageMessageEventContent {...});` for each file
                        - set `body` field to whatever is in `self.message`?,
                        - use the MXC URL(s) we got when we uploaded our file(s)
                    5. Send the message(s)!
                */
                let file_select = tokio::task::spawn_blocking(
                    || -> Result<Vec<PathBuf>, nfd2::error::NFDError> {
                        let paths = match nfd2::dialog_multiple().open()? {
                            nfd2::Response::Cancel => vec![],
                            nfd2::Response::Okay(path) => vec![path],
                            nfd2::Response::OkayMultiple(paths) => paths,
                        }
                        .into_iter()
                        // Filter directories out
                        // TODO: implement sending all files in a directory
                        .filter(|path| !path.is_dir())
                        .collect::<Vec<_>>();

                        Ok(paths)
                    },
                );

                // placeholder
                return Command::perform(file_select, |result| {
                    match result {
                        Ok(file_picker_result) => {
                            if let Ok(paths) = file_picker_result {
                                println!("User selected paths: {:?}", paths);
                            }
                        }
                        Err(err) => {
                            log::error!(
                                "Error occured while processing file picker task result: {}",
                                err
                            );
                        }
                    }
                    super::Message::Nothing
                });
            }
            Message::SendMessage => {
                if !self.message.is_empty() {
                    let content =
                        MessageEventContent::text_plain(self.message.drain(..).collect::<String>());
                    if let Some(Some((inner, room_id))) = self.current_room_id.clone().map(|id| {
                        if self.client.has_room(&id) {
                            Some((self.client.inner(), id))
                        } else {
                            None
                        }
                    }) {
                        scroll_to_bottom(self, room_id.clone());
                        self.prev_scroll_perc = 1.0;
                        self.event_history_state.scroll_to_bottom();
                        let transaction_id = Uuid::new_v4();
                        // This unwrap is safe since we check if the room exists beforehand
                        // TODO: check if we actually need to check if a room exists beforehand
                        self.client.get_room_mut(&room_id).unwrap().add_event(
                            TimelineEvent::new_unacked_message(content.clone(), transaction_id),
                        );
                        let content = AnyMessageEventContent::RoomMessage(content);
                        return Command::perform(
                            async move {
                                (
                                    Client::send_message(
                                        inner,
                                        content,
                                        room_id.clone(),
                                        transaction_id,
                                    )
                                    .await,
                                    transaction_id,
                                    room_id,
                                )
                            },
                            |(result, transaction_id, room_id)| {
                                process_send_message_result(result, transaction_id, room_id)
                            },
                        );
                    }
                }
            }
            Message::RetrySendMessage {
                retry_after,
                transaction_id,
                room_id,
            } => {
                let inner = self.client.inner();
                let content = if let Some(Some(Some(content))) =
                    self.client.get_room(&room_id).map(|room| {
                        room.timeline()
                            .iter()
                            .find(|tevent| tevent.transaction_id() == Some(&transaction_id))
                            .map(|tevent| tevent.message_content())
                    }) {
                    content
                } else {
                    return Command::none();
                };

                return Command::perform(
                    async move {
                        tokio::time::delay_for(retry_after).await;
                        (
                            Client::send_message(inner, content, room_id.clone(), transaction_id)
                                .await,
                            transaction_id,
                            room_id,
                        )
                    },
                    |(result, transaction_id, room_id)| {
                        process_send_message_result(result, transaction_id, room_id)
                    },
                );
            }
            Message::MatrixSyncResponse(response) => {
                let (download_urls, read_urls) = self.client.process_sync_response(*response);

                for (room_id, disp) in self
                    .client
                    .rooms()
                    .iter()
                    .map(|(id, room)| (id, room.displayable_events().len()))
                    .filter(|(id, disp)| {
                        self.current_room_id.as_ref() != Some(id)
                            && if let Some(disp_at) = self.looking_at_event.get(id) {
                                *disp_at == disp.saturating_sub(1)
                            } else {
                                false
                            }
                    })
                    .map(|(id, disp)| (id.clone(), disp))
                    .collect::<Vec<(RoomId, usize)>>()
                {
                    *self.looking_at_event.get_mut(&room_id).unwrap() = disp.saturating_sub(1);
                }

                return Command::batch(
                    download_urls
                        .into_iter()
                        .map(|url| make_download_content_com(self.client.inner(), url))
                        .chain(
                            read_urls
                                .into_iter()
                                .map(|url| make_read_thumbnail_com(url)),
                        ),
                );
            }
            Message::MatrixGetEventsAroundResponse(response) => {
                let (download_urls, read_urls) =
                    self.client.process_events_around_response(*response);

                return Command::batch(
                    download_urls
                        .into_iter()
                        .map(|url| make_download_content_com(self.client.inner(), url))
                        .chain(
                            read_urls
                                .into_iter()
                                .map(|url| make_read_thumbnail_com(url)),
                        ),
                );
            }
            Message::RoomChanged(new_room_id) => {
                if let (Some(disp), Some(disp_at)) = (
                    self.client
                        .get_room(&new_room_id)
                        .map(|room| room.displayable_events().len()),
                    self.looking_at_event.get_mut(&new_room_id),
                ) {
                    if *disp_at >= disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                        *disp_at = disp.saturating_sub(1);
                        self.prev_scroll_perc = 1.0;
                        self.event_history_state.scroll_to_bottom();
                    }
                }
                self.current_room_id = Some(new_room_id);
            }
        }
        Command::none()
    }

    pub fn subscription(&self) -> Subscription<super::Message> {
        if let Some(since) = self.client.next_batch() {
            Subscription::from_recipe(SyncRecipe {
                client: self.client.inner(),
                since,
            })
            .map(|result| match result {
                Ok(response) => {
                    super::Message::MainScreen(Message::MatrixSyncResponse(Box::from(response)))
                }
                Err(err) => super::Message::MatrixError(Box::new(err)),
            })
        } else {
            Subscription::none()
        }
    }

    pub fn on_error(&mut self, _error_string: String) {
        self.logging_out = None;
    }
}

pub type SyncResult = Result<sync_events::Response, ClientError>;
pub struct SyncRecipe {
    client: crate::client::InnerClient,
    since: String,
}

impl<H, I> iced_futures::subscription::Recipe<H, I> for SyncRecipe
where
    H: Hasher,
{
    type Output = SyncResult;

    fn hash(&self, state: &mut H) {
        std::any::TypeId::of::<Self>().hash(state);

        self.since.hash(state);
        self.client.session().hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<I>) -> BoxStream<Self::Output> {
        use iced_futures::futures::TryStreamExt;

        Box::pin(
            self.client
                .sync(
                    None,
                    self.since,
                    &PresenceState::Online,
                    Some(Duration::from_secs(20)),
                )
                .map_err(ClientError::Internal),
        )
    }
}
