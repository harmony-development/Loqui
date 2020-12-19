use crate::{
    client::{
        content::{self, ContentType, ImageHandle, ThumbnailCache},
        error::ClientError,
        Client,
    },
    ui::{
        component::{event_history::SHOWN_MSGS_LIMIT, *},
        style::{Theme, MESSAGE_SIZE, PADDING, SPACING},
    },
};
use http::Uri;
use image::GenericImageView;
use ruma::{
    events::room::{
        message::{
            AudioInfo, AudioMessageEventContent, FileInfo, FileMessageEventContent,
            ImageMessageEventContent, MessageEventContent, VideoInfo, VideoMessageEventContent,
        },
        ImageInfo, ThumbnailInfo,
    },
    RoomId,
};

#[derive(Debug, Clone)]
pub enum Message {
    /// Sent when the user wants to send a message.
    SendMessageComposer(RoomId),
    /// Sent when the user wants to send a file.
    SendFile(RoomId),
    /// Sent when user makes a change to the message they are composing.
    MessageChanged(String),
    ScrollToBottom,
    OpenContent {
        content_url: Uri,
        is_thumbnail: bool,
    },
    /// Sent when the user selects a different room.
    RoomChanged(RoomId),
    /// Sent when the user makes a change to the room search box.
    RoomSearchTextChanged(String),
    /// Sent when the user scrolls the message history.
    MessageHistoryScrolled {
        prev_scroll_perc: f32,
        scroll_perc: f32,
    },
    /// Sent when the user selects an option from the bottom menu.
    SelectedMenuOption(String),
}

#[derive(Debug, Default)]
pub struct MainScreen {
    // Event history area state
    event_history_state: scrollable::State,
    content_open_buts_state: [button::State; SHOWN_MSGS_LIMIT],
    send_file_but_state: button::State,
    composer_state: text_input::State,
    scroll_to_bottom_but_state: button::State,

    // Room area state
    menu_state: pick_list::State<String>,
    rooms_list_state: scrollable::State,
    rooms_buts_state: Vec<button::State>,
    room_search_box_state: text_input::State,

    // Join room screen state
    /// `None` if the user didn't select a room, `Some(room_id)` otherwise.
    current_room_id: Option<RoomId>,
    /// The message the user is currently typing.
    message: String,
    /// Text used to filter rooms.
    room_search_text: String,
}

impl MainScreen {
    pub fn view(
        &mut self,
        theme: Theme,
        client: &Client,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<Message> {
        let rooms = &client.rooms;

        let username = client.current_user_id().localpart().to_string();
        // Build the top menu
        // TODO: show user avatar next to name
        let menu = PickList::new(
            &mut self.menu_state,
            vec![
                username.clone(),
                "Join Room".to_string(),
                "Logout".to_string(),
            ],
            Some(username),
            Message::SelectedMenuOption,
        )
        .width(Length::Fill)
        .style(theme);

        // Resize and (if extended) initialize new button states for new rooms
        self.rooms_buts_state
            .resize_with(rooms.len(), Default::default);

        // Build the room list
        let (mut room_list, first_room_id) = build_room_list(
            rooms,
            thumbnail_cache,
            self.current_room_id.as_ref(),
            self.room_search_text.as_str(),
            &mut self.rooms_list_state,
            self.rooms_buts_state.as_mut_slice(),
            Message::RoomChanged,
            theme,
        );

        let mut room_search = TextInput::new(
            &mut self.room_search_box_state,
            "Search rooms...",
            &self.room_search_text,
            Message::RoomSearchTextChanged,
        )
        .padding(PADDING / 4)
        .size(18)
        .width(Length::Fill)
        .style(theme);

        if let Some(room_id) = first_room_id {
            room_search = room_search.on_submit(Message::RoomChanged(room_id));
        } else {
            // if first_room_id is None, then that means no room found (either cause of filter, or the user aren't in any room)
            // reusing the room_list variable here
            room_list = fill_container(label("No room found")).style(theme).into();
        }

        let rooms_area = Column::with_children(vec![
            menu.into(),
            room_list,
            Container::new(room_search)
                .width(Length::Fill)
                .padding(PADDING / 2)
                .into(),
        ]);

        let mut screen_widgets = vec![Container::new(rooms_area)
            .width(Length::Units(250))
            .height(Length::Fill)
            .style(theme)
            .into()];

        if let Some((room, room_id)) = self
            .current_room_id
            .as_ref()
            .map(|id| Some((rooms.get(id)?, id)))
            .flatten()
        {
            let message_composer = TextInput::new(
                &mut self.composer_state,
                "Enter your message here...",
                self.message.as_str(),
                Message::MessageChanged,
            )
            .padding((PADDING / 4) * 3)
            .size(MESSAGE_SIZE)
            .style(theme.secondary())
            .on_submit(Message::SendMessageComposer(room_id.clone()));

            let current_user_id = client.current_user_id();
            let displayable_event_count = room.displayable_events().count();

            let message_history_list = build_event_history(
                client.content_store(),
                thumbnail_cache,
                room,
                &current_user_id,
                room.looking_at_event,
                &mut self.event_history_state,
                &mut self.content_open_buts_state,
                theme,
            );

            let members = room.members();
            let mut typing_users_combined = String::new();
            let mut typing_members = members.typing_members();
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

                typing_users_combined += members.get_user_display_name(member_id).as_str();

                typing_users_combined += match typing_members_count {
                    x if x > index + 1 => ", ",
                    1 => " is typing...",
                    _ => " are typing...",
                };
            }

            let typing_users = Column::with_children(vec![
                awspace(6).into(),
                Row::with_children(vec![
                    awspace(9).into(),
                    label(typing_users_combined).size(14).into(),
                ])
                .into(),
            ])
            .height(Length::Units(14));

            let send_file_button = Button::new(
                &mut self.send_file_but_state,
                label("↑").size((PADDING / 4) * 3 + MESSAGE_SIZE),
            )
            .style(theme.secondary())
            .on_press(Message::SendFile(room_id.clone()));

            let mut bottom_area_widgets = vec![
                send_file_button.into(),
                message_composer.width(Length::Fill).into(),
            ];

            if room.looking_at_event < displayable_event_count.saturating_sub(SHOWN_MSGS_LIMIT) {
                bottom_area_widgets.push(
                    Button::new(
                        &mut self.scroll_to_bottom_but_state,
                        label("↡").size((PADDING / 4) * 3 + MESSAGE_SIZE),
                    )
                    .style(theme.secondary())
                    .on_press(Message::ScrollToBottom)
                    .into(),
                );
            }

            let message_area = Column::with_children(vec![
                message_history_list,
                typing_users.into(),
                Container::new(
                    Row::with_children(bottom_area_widgets)
                        .spacing(SPACING * 2)
                        .width(Length::Fill),
                )
                .width(Length::Fill)
                .padding(PADDING / 2)
                .into(),
            ]);

            screen_widgets.push(fill_container(message_area).style(theme.secondary()).into());
        }

        // We know that there will be only one widget if the user isn't looking at a room currently
        if screen_widgets.len() < 2 {
            let in_no_room_warning = fill_container(
                label("Select / join a room to start chatting!")
                    .size(35)
                    .color(color!(128, 128, 128)),
            )
            .style(theme.secondary());

            screen_widgets.push(in_no_room_warning.into());
        }

        Row::with_children(screen_widgets)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    pub fn update(
        &mut self,
        msg: Message,
        client: &mut Client,
        thumbnail_cache: &ThumbnailCache,
    ) -> Command<super::Message> {
        fn scroll_to_bottom(client: &mut Client, room_id: RoomId) {
            if let Some((disp, looking_at_event)) = client.rooms.get_mut(&room_id).map(|room| {
                (
                    room.displayable_events().count(),
                    &mut room.looking_at_event,
                )
            }) {
                *looking_at_event = disp.saturating_sub(1);
            }
        }

        match msg {
            Message::MessageHistoryScrolled {
                prev_scroll_perc,
                scroll_perc,
            } => {
                if let Some(current_room_id) = self.current_room_id.clone() {
                    if scroll_perc < 0.01 && scroll_perc <= prev_scroll_perc {
                        if let Some((disp, loading_events_backward, looking_at_event, prev_batch)) =
                            client.rooms.get_mut(&current_room_id).map(|room| {
                                (
                                    room.displayable_events().count(),
                                    &mut room.loading_events_backward,
                                    &mut room.looking_at_event,
                                    room.prev_batch.clone(),
                                )
                            })
                        {
                            if *looking_at_event == disp.saturating_sub(1) {
                                *looking_at_event = disp.saturating_sub(SHOWN_MSGS_LIMIT + 1);
                            } else {
                                *looking_at_event = looking_at_event.saturating_sub(1);
                            }

                            if *looking_at_event < 2 && !*loading_events_backward {
                                if let Some(prev_batch) = prev_batch {
                                    *loading_events_backward = true;
                                    return Command::perform(
                                        Client::get_events_backwards(
                                            client.inner(),
                                            current_room_id,
                                            prev_batch.clone(),
                                        ),
                                        |result| match result {
                                            Ok(response) => {
                                                super::Message::GetEventsBackwardsResponse(
                                                    Box::new(response),
                                                )
                                            }

                                            Err(err) => super::Message::MatrixError(Box::new(err)),
                                        },
                                    );
                                }
                            }
                        }
                    } else if scroll_perc > 0.99 && scroll_perc >= prev_scroll_perc {
                        if let Some((disp, looking_at_event)) =
                            client.rooms.get_mut(&current_room_id).map(|room| {
                                (
                                    room.displayable_events().count(),
                                    &mut room.looking_at_event,
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
                }
            }
            Message::SelectedMenuOption(option) => match option.as_str() {
                "Logout" => {
                    return Command::perform(async {}, |_| {
                        super::Message::PushScreen(Box::new(super::Screen::Logout(
                            super::LogoutScreen::default(),
                        )))
                    })
                }
                "Join Room" => {
                    return Command::perform(async {}, |_| {
                        super::Message::PushScreen(Box::new(super::Screen::RoomDiscovery(
                            super::RoomDiscoveryScreen::default(),
                        )))
                    })
                }
                u if u == client.current_user_id().localpart() => {}
                _ => unreachable!(),
            },
            Message::MessageChanged(new_msg) => {
                self.message = new_msg;

                if let Some(room_id) = self.current_room_id.clone() {
                    return Command::perform(
                        Client::send_typing(client.inner(), room_id, client.current_user_id()),
                        |result| match result {
                            Ok(_) => super::Message::Nothing,
                            Err(err) => super::Message::MatrixError(Box::new(err)),
                        },
                    );
                }
            }
            Message::ScrollToBottom => {
                if let Some(room_id) = self.current_room_id.clone() {
                    scroll_to_bottom(client, room_id);
                    self.event_history_state.scroll_to_bottom();
                }
            }
            Message::OpenContent {
                content_url,
                is_thumbnail,
            } => {
                let thumbnail_exists = thumbnail_cache.has_thumbnail(&content_url);
                let content_path = client.content_store().content_path(&content_url);
                return if content_path.exists() {
                    Command::perform(
                        async move {
                            let thumbnail = if is_thumbnail && !thumbnail_exists {
                                tokio::fs::read(&content_path)
                                    .await
                                    .map_or(None, |data| Some((data, content_url)))
                            } else {
                                None
                            };

                            (content_path, thumbnail)
                        },
                        |(content_path, thumbnail)| {
                            open::that_in_background(content_path);
                            if let Some((data, thumbnail_url)) = thumbnail {
                                super::Message::DownloadedThumbnail {
                                    thumbnail_url,
                                    thumbnail: ImageHandle::from_memory(data),
                                }
                            } else {
                                super::Message::Nothing
                            }
                        },
                    )
                } else {
                    let download_task =
                        Client::download_content(client.inner(), content_url.clone());
                    Command::perform(
                        async move {
                            match download_task.await {
                                Ok(raw_data) => {
                                    tokio::fs::write(&content_path, raw_data.as_slice()).await?;
                                    Ok((
                                        content_path,
                                        if is_thumbnail && !thumbnail_exists {
                                            Some((content_url, raw_data))
                                        } else {
                                            None
                                        },
                                    ))
                                }
                                Err(err) => Err(err),
                            }
                        },
                        |result| match result {
                            Ok((content_path, thumbnail)) => {
                                open::that_in_background(content_path);
                                if let Some((content_url, raw_data)) = thumbnail {
                                    super::Message::DownloadedThumbnail {
                                        thumbnail_url: content_url,
                                        thumbnail: ImageHandle::from_memory(raw_data),
                                    }
                                } else {
                                    super::Message::Nothing
                                }
                            }
                            Err(err) => super::Message::MatrixError(Box::new(err)),
                        },
                    )
                };
            }
            Message::SendMessageComposer(room_id) => {
                if !self.message.is_empty() {
                    let content =
                        MessageEventContent::text_plain(self.message.drain(..).collect::<String>());
                    scroll_to_bottom(client, room_id.clone());
                    self.event_history_state.scroll_to_bottom();
                    return Command::perform(
                        async { (vec![content], room_id) },
                        |(content, room_id)| super::Message::SendMessage { content, room_id },
                    );
                }
            }
            Message::SendFile(room_id) => {
                let file_select_task = tokio::task::spawn_blocking(
                    || -> Result<Vec<std::path::PathBuf>, ClientError> {
                        Ok(rfd::pick_files(None).unwrap_or_else(Vec::new))
                    },
                );

                let inner = client.inner();
                let content_store = client.content_store_arc();

                return Command::perform(
                    async move {
                        let paths = file_select_task
                            .await
                            .map_err(|e| ClientError::Custom(e.to_string()))??;
                        let mut content_urls_to_send = Vec::with_capacity(paths.len());

                        for path in paths {
                            match tokio::fs::read(&path).await {
                                Ok(data) => {
                                    let file_mimetype = content::infer_type_from_bytes(&data);
                                    let filesize = data.len();
                                    let filename = content::get_filename(&path).to_string();

                                    // TODO: implement video thumbnailing
                                    let (thumbnail, image_info) = if let ContentType::Image =
                                        ContentType::new(&file_mimetype)
                                    {
                                        if let Ok(image) = image::load_from_memory(&data) {
                                            let image_dimensions = image.dimensions(); // (w, h)
                                            let thumbnail_scale = ((1000 * 1000) / filesize) as u32;

                                            if thumbnail_scale <= 1 {
                                                let new_width =
                                                    image_dimensions.0 * thumbnail_scale;
                                                let new_height =
                                                    image_dimensions.1 * thumbnail_scale;

                                                let thumbnail =
                                                    image.thumbnail(new_width, new_height);
                                                let thumbnail_raw = thumbnail.to_bytes();
                                                let thumbnail_size = thumbnail_raw.len();

                                                let send_result = Client::send_content(
                                                    inner.clone(),
                                                    thumbnail_raw,
                                                    Some(file_mimetype.clone()),
                                                    Some(format!("thumbnail_{}", filename)),
                                                )
                                                .await;

                                                match send_result {
                                                    Ok(thumbnail_url) => (
                                                        Some((
                                                            thumbnail_url,
                                                            thumbnail_size,
                                                            thumbnail.height(),
                                                            thumbnail.width(),
                                                        )),
                                                        Some(image_dimensions),
                                                    ),
                                                    Err(err) => {
                                                        log::error!("An error occured while uploading a thumbnail: {}", err);
                                                        (None, Some(image_dimensions))
                                                    }
                                                }
                                            } else {
                                                (None, Some(image_dimensions))
                                            }
                                        } else {
                                            (None, None)
                                        }
                                    } else {
                                        (None, None)
                                    };

                                    let send_result = Client::send_content(
                                        inner.clone(),
                                        data,
                                        Some(file_mimetype.clone()),
                                        Some(filename.clone()),
                                    )
                                    .await;

                                    match send_result {
                                        Ok(content_url) => {
                                            if let Err(err) = tokio::fs::hard_link(
                                                &path,
                                                content_store.content_path(&content_url),
                                            )
                                            .await
                                            {
                                                log::warn!("An IO error occured while hard linking a file you tried to upload (this may result in a duplication of the file): {}", err);
                                            }
                                            content_urls_to_send.push((
                                                content_url,
                                                filename,
                                                file_mimetype,
                                                filesize,
                                                thumbnail,
                                                image_info,
                                            ));
                                        }
                                        Err(err) => {
                                            log::error!(
                                                "An error occured while trying to upload a file: {}",
                                                err
                                            );
                                        }
                                    }
                                }
                                Err(err) => {
                                    log::error!(
                                        "An IO error occured while trying to upload a file: {}",
                                        err
                                    );
                                }
                            }
                        }
                        Ok((content_urls_to_send, room_id))
                    },
                    |result| match result {
                        Ok((content_urls_to_send, room_id)) => super::Message::SendMessage {
                            content: content_urls_to_send
                                .into_iter()
                                .map(
                                    |(
                                        url,
                                        filename,
                                        file_mimetype,
                                        filesize,
                                        thumbnail,
                                        image_dimensions,
                                    )| {
                                        use ruma::UInt;

                                        let (thumbnail_url, thumbnail_info) =
                                            if let Some((url, size, h, w)) = thumbnail {
                                                (
                                                    Some(url.to_string()),
                                                    Some(Box::new(ThumbnailInfo {
                                                        height: Some(UInt::from(h)),
                                                        width: Some(UInt::from(w)),
                                                        mimetype: Some(file_mimetype.clone()),
                                                        size: UInt::new(size as u64),
                                                    })),
                                                )
                                            } else {
                                                (None, None)
                                            };

                                        let body = filename;
                                        let mimetype = Some(file_mimetype.clone());
                                        let url = Some(url.to_string());

                                        match ContentType::new(&file_mimetype) {
                                            ContentType::Image => MessageEventContent::Image(
                                                ImageMessageEventContent {
                                                    body,
                                                    info: Some(Box::new(ImageInfo {
                                                        mimetype,
                                                        height: image_dimensions
                                                            .map(|(_, h)| UInt::from(h)),
                                                        width: image_dimensions
                                                            .map(|(w, _)| UInt::from(w)),
                                                        size: UInt::new(filesize as u64),
                                                        thumbnail_info,
                                                        thumbnail_url,
                                                        thumbnail_file: None,
                                                    })),
                                                    url,
                                                    file: None,
                                                },
                                            ),
                                            ContentType::Audio => MessageEventContent::Audio(
                                                AudioMessageEventContent {
                                                    body,
                                                    info: Some(Box::new(AudioInfo {
                                                        duration: None,
                                                        mimetype,
                                                        size: UInt::new(filesize as u64),
                                                    })),
                                                    url,
                                                    file: None,
                                                },
                                            ),
                                            ContentType::Video => MessageEventContent::Video(
                                                VideoMessageEventContent {
                                                    body,
                                                    info: Some(Box::new(VideoInfo {
                                                        mimetype,
                                                        height: None,
                                                        width: None,
                                                        duration: None,
                                                        size: UInt::new(filesize as u64),
                                                        thumbnail_info,
                                                        thumbnail_url,
                                                        thumbnail_file: None,
                                                    })),
                                                    url,
                                                    file: None,
                                                },
                                            ),
                                            ContentType::Other => {
                                                MessageEventContent::File(FileMessageEventContent {
                                                    body: body.clone(),
                                                    filename: Some(body),
                                                    info: Some(Box::new(FileInfo {
                                                        mimetype,
                                                        size: UInt::new(filesize as u64),
                                                        thumbnail_info,
                                                        thumbnail_url,
                                                        thumbnail_file: None,
                                                    })),
                                                    url,
                                                    file: None,
                                                })
                                            }
                                        }
                                    },
                                )
                                .collect(),
                            room_id,
                        },
                        Err(err) => super::Message::MatrixError(Box::new(err)),
                    },
                );
            }
            Message::RoomChanged(new_room_id) => {
                if let Some((disp, disp_at)) = client.rooms.get_mut(&new_room_id).map(|room| {
                    (
                        room.displayable_events().count(),
                        &mut room.looking_at_event,
                    )
                }) {
                    if *disp_at >= disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                        *disp_at = disp.saturating_sub(1);
                        self.event_history_state.scroll_to_bottom();
                    }
                }
                self.current_room_id = Some(new_room_id);
            }
            Message::RoomSearchTextChanged(new_room_search_text) => {
                self.room_search_text = new_room_search_text;
            }
        }

        Command::none()
    }
}
