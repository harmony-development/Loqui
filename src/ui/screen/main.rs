use crate::{
    client::{
        media::{self, ContentType, ImageHandle, ThumbnailStore},
        Client, ClientError, InnerClient, TimelineEvent,
    },
    ui::{
        component::{build_event_history, build_room_list, event_history::SHOWN_MSGS_LIMIT},
        style::{BrightContainer, DarkButton, DarkTextInput, Theme},
    },
};
use ahash::AHashMap;
use iced::{
    button, pick_list, scrollable, text_input, Align, Button, Color, Column, Command, Container,
    Element, Length, PickList, Row, Space, Subscription, Text, TextInput,
};
use iced_futures::BoxStream;
use image::GenericImageView;
use ruma::{
    api::{
        client::r0::{context::get_context, sync::sync_events},
        exports::http::Uri,
    },
    events::room::message::FileMessageEventContent,
    events::room::message::VideoInfo,
    events::room::message::VideoMessageEventContent,
    events::room::ThumbnailInfo,
    events::{
        room::{
            message::{
                AudioInfo, AudioMessageEventContent, FileInfo, ImageMessageEventContent,
                MessageEventContent,
            },
            ImageInfo,
        },
        AnySyncRoomEvent,
    },
    presence::PresenceState,
    RoomId,
};
use std::{hash::Hash, hash::Hasher, path::PathBuf, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Message {
    SendMessage {
        content: Vec<MessageEventContent>,
        room_id: RoomId,
    },
    SendMessageResult(RetrySendEventResult),
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
    DownloadedThumbnail {
        thumbnail_url: Uri,
        thumbnail: ImageHandle,
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
    LogoutConfirmation(bool),
    /// Sent when a sync response is received from the server.
    MatrixSyncResponse(Box<sync_events::Response>),
    /// Sent when a "get context" (get events around an event) is received from the server.
    MatrixGetEventsAroundResponse(Box<get_context::Response>),
}

pub struct MainScreen {
    // Event history area state
    event_history_state: scrollable::State,
    content_open_buts_state: Vec<button::State>,
    send_file_but_state: button::State,
    composer_state: text_input::State,
    scroll_to_bottom_but_state: button::State,

    // Room area state
    menu_state: pick_list::State<String>,
    rooms_list_state: scrollable::State,
    rooms_buts_state: Vec<button::State>,
    room_search_box_state: text_input::State,

    // Logout screen state
    logout_approve_but_state: button::State,
    logout_cancel_but_state: button::State,

    /// `Some(confirmation)` if there is an ongoing logout request, `None` otherwise.
    /// `confirmation` is `true` if the user approves the logout, `false` otherwise.
    logging_out: Option<bool>,
    /// `None` if the user didn't select a room, `Some(room_id)` otherwise.
    current_room_id: Option<RoomId>,
    looking_at_event: AHashMap<RoomId, usize>,
    /// The message the user is currently typing.
    message: String,
    /// Text used to filter rooms.
    room_search_text: String,
    thumbnail_store: ThumbnailStore,
}

impl MainScreen {
    pub fn new() -> Self {
        Self {
            composer_state: Default::default(),
            scroll_to_bottom_but_state: Default::default(),
            send_file_but_state: Default::default(),
            event_history_state: Default::default(),
            rooms_list_state: Default::default(),
            rooms_buts_state: Default::default(),
            room_search_box_state: Default::default(),
            content_open_buts_state: vec![Default::default(); SHOWN_MSGS_LIMIT],
            menu_state: Default::default(),
            logout_approve_but_state: Default::default(),
            logout_cancel_but_state: Default::default(),
            logging_out: None,
            current_room_id: None,
            looking_at_event: Default::default(),
            message: Default::default(),
            room_search_text: Default::default(),
            thumbnail_store: ThumbnailStore::new(),
        }
    }

    pub fn logout_screen(&mut self, theme: Theme, confirmation: bool) -> Element<Message> {
        if confirmation {
            Container::new(Text::new("Logging out...").size(30))
                .center_y()
                .center_x()
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme)
                .into()
        } else {
            #[inline(always)]
            fn make_button<'a>(
                state: &'a mut button::State,
                confirm: bool,
                theme: Theme,
            ) -> Element<'a, Message> {
                Button::new(
                    state,
                    Container::new(Text::new(if confirm { "Yes" } else { "No" }))
                        .width(Length::Fill)
                        .center_x(),
                )
                .width(Length::FillPortion(1))
                .on_press(Message::LogoutConfirmation(confirm))
                .style(theme)
                .into()
            }

            #[inline(always)]
            fn make_space<'a>(units: u16) -> Element<'a, Message> {
                Space::with_width(Length::FillPortion(units)).into()
            }

            let logout_confirm_panel = Column::with_children(
                    vec![
                        Text::new("Do you want to logout?").into(),
                        Text::new("This will delete your current session and you will need to login with your password.")
                            .color(Color::from_rgb(1.0, 0.0, 0.0))
                            .into(),
                        Row::with_children(
                            vec![
                                make_space(2),
                                make_button(&mut self.logout_approve_but_state, true, theme),
                                make_space(1),
                                make_button(&mut self.logout_cancel_but_state, false, theme),
                                make_space(2),
                        ])
                        .width(Length::Fill)
                        .align_items(Align::Center)
                        .into(),
                    ])
                    .align_items(Align::Center)
                    .spacing(12);

            let padded_panel = Row::with_children(vec![
                make_space(3),
                logout_confirm_panel.width(Length::FillPortion(4)).into(),
                make_space(3),
            ])
            .height(Length::Fill)
            .align_items(Align::Center);

            Container::new(padded_panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme)
                .into()
        }
    }

    pub fn view(&mut self, theme: Theme, client: &Client) -> Element<Message> {
        if let Some(confirmation) = self.logging_out {
            return self.logout_screen(theme, confirmation);
        }

        let rooms = client.rooms();

        // Add missing looking_at_event values for newly added rooms (if there is any)
        for (room_id, room) in rooms {
            if !self.looking_at_event.contains_key(room_id) {
                self.looking_at_event.insert(
                    room_id.clone(),
                    room.displayable_events().count().saturating_sub(1),
                );
            }
        }

        let username = client.current_user_id().localpart().to_string();
        // Build the top menu
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
        .padding(4)
        .size(18)
        .width(Length::Fill)
        .style(theme);

        if let Some(room_id) = first_room_id {
            room_search = room_search.on_submit(Message::RoomChanged(room_id));
        } else {
            // if first_room_id is None, then that means no room found (either cause of filter, or the user aren't in any room)
            // reusing the room_list variable here
            room_list = Container::new(Text::new("No room found"))
                .center_x()
                .center_y()
                .height(Length::Fill)
                .width(Length::Fill)
                .style(theme)
                .into();
        }

        let rooms_area = Column::with_children(vec![
            menu.into(),
            room_list,
            Container::new(room_search)
                .width(Length::Fill)
                .padding(6)
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
            .padding(12)
            .size(16)
            .style(DarkTextInput)
            .on_submit(Message::SendMessageComposer(room_id.clone()));

            let current_user_id = client.current_user_id();
            let displayable_event_count = room.displayable_events().count();

            let message_history_list = build_event_history(
                &self.thumbnail_store,
                room,
                &current_user_id,
                self.looking_at_event
                    .get(room_id)
                    .copied()
                    .unwrap_or_else(|| displayable_event_count.saturating_sub(1)),
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
            ])
            .height(Length::Units(14));

            let send_file_button =
                Button::new(&mut self.send_file_but_state, Text::new("↑").size(28))
                    .style(DarkButton)
                    .on_press(Message::SendFile(room_id.clone()));

            let mut bottom_area_widgets = vec![
                send_file_button.into(),
                message_composer.width(Length::Fill).into(),
            ];

            // This unwrap is safe since we add the room to the map before this
            if *self.looking_at_event.get(room_id).unwrap()
                < displayable_event_count.saturating_sub(SHOWN_MSGS_LIMIT)
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

        // We know that there will be only one widget if the user isn't looking at a room currently
        if screen_widgets.len() < 2 {
            let in_no_room_warning = Container::new(
                Text::new("Select / join a room to start chatting!")
                    .size(35)
                    .color(Color::from_rgb(0.5, 0.5, 0.5)),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .style(BrightContainer);

            screen_widgets.push(in_no_room_warning.into());
        }

        Row::with_children(screen_widgets)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    pub fn update(&mut self, msg: Message, client: &mut Client) -> Command<super::Message> {
        fn make_thumbnail_commands(
            client: &Client,
            thumbnail_urls: Vec<(bool, Uri)>,
        ) -> Command<super::Message> {
            return Command::batch(thumbnail_urls.into_iter().map(
                |(is_in_cache, thumbnail_url)| {
                    if is_in_cache {
                        Command::perform(
                            async move {
                                (
                                    async {
                                        Ok(ImageHandle::from_memory(
                                            tokio::fs::read(
                                                media::make_content_path(&thumbnail_url)
                                                    .map_or_else(
                                                        || {
                                                            Err(ClientError::Custom(String::from(
                                                                "Could not make content path",
                                                            )))
                                                        },
                                                        |p| Ok(p),
                                                    )?,
                                            )
                                            .await?,
                                        ))
                                    }
                                    .await,
                                    thumbnail_url,
                                )
                            },
                            |(result, thumbnail_url)| match result {
                                Ok(thumbnail) => {
                                    super::Message::MainScreen(Message::DownloadedThumbnail {
                                        thumbnail,
                                        thumbnail_url,
                                    })
                                }
                                Err(err) => super::Message::MatrixError(Box::new(err)),
                            },
                        )
                    } else {
                        let inner = client.inner();

                        Command::perform(
                            async move {
                                let download_result =
                                    Client::download_content(inner, thumbnail_url.clone()).await;

                                match download_result {
                                    Ok(raw_data) => {
                                        if let (Some(content_path), Some(server_media_dir)) = (
                                            media::make_content_path(&thumbnail_url),
                                            media::make_content_folder(&thumbnail_url),
                                        ) {
                                            tokio::fs::create_dir_all(server_media_dir).await?;
                                            tokio::fs::write(content_path, raw_data.as_slice())
                                                .await
                                                .map(|_| (thumbnail_url, raw_data))
                                                .map_err(|e| e.into())
                                        } else {
                                            Err(ClientError::Custom(String::from(
                                                "Could not make content path or server media path",
                                            )))
                                        }
                                    }
                                    Err(err) => Err(err),
                                }
                            },
                            |result| match result {
                                Ok((thumbnail_url, raw_data)) => {
                                    super::Message::MainScreen(Message::DownloadedThumbnail {
                                        thumbnail_url,
                                        thumbnail: ImageHandle::from_memory(raw_data),
                                    })
                                }
                                Err(err) => super::Message::MatrixError(Box::new(err)),
                            },
                        )
                    }
                },
            ));
        }

        fn scroll_to_bottom(screen: &mut MainScreen, client: &Client, room_id: RoomId) {
            if let Some(disp) = client
                .get_room(&room_id)
                .map(|room| room.displayable_events().count())
            {
                screen
                    .looking_at_event
                    .entry(room_id)
                    .and_modify(|d| *d = disp.saturating_sub(1))
                    .or_insert_with(|| disp.saturating_sub(1));
            }
        }

        match msg {
            Message::MessageHistoryScrolled {
                prev_scroll_perc,
                scroll_perc,
            } => {
                if scroll_perc < 0.01 && scroll_perc <= prev_scroll_perc {
                    if let Some((disp, looking_at_event)) = self
                        .current_room_id
                        .clone()
                        .map(|id| {
                            Some((
                                client
                                    .get_room(&id)
                                    .map(|room| room.displayable_events().count())?,
                                self.looking_at_event.get_mut(&id)?,
                            ))
                        })
                        .flatten()
                    {
                        if *looking_at_event == disp.saturating_sub(1) {
                            *looking_at_event = disp.saturating_sub(SHOWN_MSGS_LIMIT + 1);
                        } else {
                            *looking_at_event = looking_at_event.saturating_sub(1);
                        }

                        if *looking_at_event < 2 {
                            if let Some((event, room_id)) = self
                                .current_room_id
                                .as_ref()
                                .map(|id| {
                                    client
                                        .get_room(id)
                                        .map(|room| Some((room.displayable_events().next()?, id)))
                                        .flatten()
                                })
                                .flatten()
                            {
                                let inner = client.inner();
                                let room_id = room_id.clone();
                                let event_id = event.id().clone();
                                return Command::perform(
                                    Client::get_events_around(inner, room_id, event_id),
                                    |result| match result {
                                        Ok(response) => super::Message::MainScreen(
                                            Message::MatrixGetEventsAroundResponse(Box::new(
                                                response,
                                            )),
                                        ),
                                        Err(err) => super::Message::MatrixError(Box::new(err)),
                                    },
                                );
                            }
                        }
                    }
                } else if scroll_perc > 0.99 && scroll_perc >= prev_scroll_perc {
                    if let Some((disp, looking_at_event)) = self
                        .current_room_id
                        .clone()
                        .map(|id| {
                            Some((
                                client
                                    .get_room(&id)
                                    .map(|room| room.displayable_events().count())?,
                                self.looking_at_event.get_mut(&id)?,
                            ))
                        })
                        .flatten()
                    {
                        if *looking_at_event > disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                            *looking_at_event = disp.saturating_sub(1);
                        } else {
                            *looking_at_event = looking_at_event.saturating_add(1).min(disp);
                        }
                    }
                }
            }
            Message::SelectedMenuOption(option) => match option.as_str() {
                "Logout" => {
                    self.logging_out = Some(false);
                }
                "Join Room" => println!("aaaaaa"),
                u if u == client.current_user_id().localpart() => println!("bbbbbbbb"),
                _ => unreachable!(),
            },
            Message::LogoutConfirmation(confirmation) => {
                if confirmation {
                    self.logging_out = Some(true);
                    let inner = client.inner();
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
                    let inner = client.inner();
                    return Command::perform(
                        Client::send_typing(inner, room_id.clone(), client.current_user_id()),
                        |result| match result {
                            Ok(_) => super::Message::Nothing,
                            Err(err) => super::Message::MatrixError(Box::new(err)),
                        },
                    );
                }
            }
            Message::ScrollToBottom => {
                if let Some(room_id) = self.current_room_id.clone() {
                    scroll_to_bottom(self, client, room_id);
                    self.event_history_state.scroll_to_bottom();
                }
            }
            Message::DownloadedThumbnail {
                thumbnail_url,
                thumbnail,
            } => {
                self.thumbnail_store.put_thumbnail(thumbnail_url, thumbnail);
            }
            Message::OpenContent {
                content_url,
                is_thumbnail,
            } => {
                let thumbnail_exists = self.thumbnail_store.has_thumbnail(&content_url);
                if let Some(content_path) = media::make_content_path(&content_url) {
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
                                    super::Message::MainScreen(Message::DownloadedThumbnail {
                                        thumbnail_url,
                                        thumbnail: ImageHandle::from_memory(data),
                                    })
                                } else {
                                    super::Message::Nothing
                                }
                            },
                        )
                    } else {
                        let inner = client.inner();
                        Command::perform(
                            async move {
                                match Client::download_content(inner, content_url.clone()).await {
                                    Ok(raw_data) => {
                                        if let Some(server_media_dir) =
                                            media::make_content_folder(&content_url)
                                        {
                                            tokio::fs::create_dir_all(server_media_dir).await?;
                                            tokio::fs::write(&content_path, raw_data.as_slice())
                                                .await?;
                                            Ok((
                                                content_path,
                                                if is_thumbnail && !thumbnail_exists {
                                                    Some((content_url, raw_data))
                                                } else {
                                                    None
                                                },
                                            ))
                                        } else {
                                            Err(ClientError::Custom(String::from("Could not make media path: media doesnt come from any server")))
                                        }
                                    }
                                    Err(err) => Err(err),
                                }
                            },
                            |result| match result {
                                Ok((content_path, thumbnail)) => {
                                    open::that_in_background(content_path);
                                    if let Some((content_url, raw_data)) = thumbnail {
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
            }
            Message::SendMessageComposer(room_id) => {
                if !self.message.is_empty() {
                    let content =
                        MessageEventContent::text_plain(self.message.drain(..).collect::<String>());
                    scroll_to_bottom(self, client, room_id.clone());
                    self.event_history_state.scroll_to_bottom();
                    return Command::perform(
                        async move { (content, room_id) },
                        |(content, room_id)| {
                            super::Message::MainScreen(Message::SendMessage {
                                content: vec![content],
                                room_id,
                            })
                        },
                    );
                }
            }
            Message::SendFile(room_id) => {
                let file_select_task =
                    tokio::task::spawn_blocking(|| -> Result<Vec<PathBuf>, ClientError> {
                        let paths = match nfd2::dialog_multiple()
                            .open()
                            .map_err(|e| ClientError::Custom(e.to_string()))?
                        {
                            nfd2::Response::Cancel => vec![],
                            nfd2::Response::Okay(path) => vec![path],
                            nfd2::Response::OkayMultiple(paths) => paths,
                        }
                        .into_iter()
                        .filter(|path| !path.is_dir())
                        .collect();

                        Ok(paths)
                    });

                let inner = client.inner();

                return Command::perform(
                    async move {
                        let paths = file_select_task
                            .await
                            .map_err(|e| ClientError::Custom(e.to_string()))??;
                        let mut content_urls_to_send = Vec::with_capacity(paths.len());

                        for path in paths {
                            match tokio::fs::read(&path).await {
                                Ok(data) => {
                                    let file_mimetype = media::infer_mimetype_from_bytes(&data);
                                    let filesize = data.len();
                                    let filename = media::get_filename(&path);

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
                                            if let Some(server_media_dir) =
                                                media::make_content_folder(&content_url)
                                            {
                                                if let Err(err) =
                                                    tokio::fs::create_dir_all(server_media_dir)
                                                        .await
                                                {
                                                    log::warn!("An IO error occured while trying to create a folder to hard link a file you tried to upload: {}", err);
                                                }
                                            }
                                            if let Some(content_path) =
                                                media::make_content_path(&content_url)
                                            {
                                                if let Err(err) =
                                                    tokio::fs::hard_link(&path, content_path).await
                                                {
                                                    log::warn!("An IO error occured while hard linking a file you tried to upload (this may result in a duplication of the file): {}", err);
                                                }
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
                        Ok((content_urls_to_send, room_id)) => {
                            super::Message::MainScreen(Message::SendMessage {
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
                                            let (thumbnail_url, thumbnail_info) =
                                                if let Some((url, size, h, w)) = thumbnail {
                                                    (
                                                        Some(url.to_string()),
                                                        Some(Box::new(ThumbnailInfo {
                                                            height: Some(ruma::UInt::from(h)),
                                                            width: Some(ruma::UInt::from(w)),
                                                            mimetype: Some(file_mimetype.clone()),
                                                            size: ruma::UInt::new(size as u64),
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
                                                                .map(|(_, h)| ruma::UInt::from(h)),
                                                            width: image_dimensions
                                                                .map(|(w, _)| ruma::UInt::from(w)),
                                                            size: ruma::UInt::new(filesize as u64),
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
                                                            size: ruma::UInt::new(filesize as u64),
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
                                                            size: ruma::UInt::new(filesize as u64),
                                                            thumbnail_info,
                                                            thumbnail_url,
                                                            thumbnail_file: None,
                                                        })),
                                                        url,
                                                        file: None,
                                                    },
                                                ),
                                                ContentType::Other => MessageEventContent::File(
                                                    FileMessageEventContent {
                                                        body: body.clone(),
                                                        filename: Some(body),
                                                        info: Some(Box::new(FileInfo {
                                                            mimetype,
                                                            size: ruma::UInt::new(filesize as u64),
                                                            thumbnail_info,
                                                            thumbnail_url,
                                                            thumbnail_file: None,
                                                        })),
                                                        url,
                                                        file: None,
                                                    },
                                                ),
                                            }
                                        },
                                    )
                                    .collect(),
                                room_id,
                            })
                        }
                        Err(err) => super::Message::MatrixError(Box::new(err)),
                    },
                );
            }
            Message::SendMessage { content, room_id } => {
                if let Some(room) = client.get_room_mut(&room_id) {
                    for content in content {
                        room.add_event(TimelineEvent::new_unacked_message(content, Uuid::new_v4()));
                    }
                }
            }
            Message::SendMessageResult(errors) => {
                use ruma::{api::client::error::ErrorKind as ClientAPIErrorKind, api::error::*};
                use ruma_client::Error as InnerClientError;

                for (room_id, errors) in errors {
                    for (transaction_id, error) in errors {
                        if let ClientError::Internal(InnerClientError::FromHttpResponse(
                            FromHttpResponseError::Http(ServerError::Known(err)),
                        )) = error
                        {
                            if let ClientAPIErrorKind::LimitExceeded { retry_after_ms } = err.kind {
                                if let Some(retry_after) = retry_after_ms {
                                    if let Some(room) = client.get_room_mut(&room_id) {
                                        room.wait_for_duration(retry_after, transaction_id);
                                    }
                                    log::error!("Send message after: {}", retry_after.as_secs());
                                }
                            }
                        } else {
                            log::error!("Error while sendign message: {}", error);
                        }
                    }
                }
            }
            Message::MatrixSyncResponse(response) => {
                let thumbnail_urls = client.process_sync_response(*response);

                for (room_id, disp) in client
                    .rooms()
                    .iter()
                    .filter_map(|(id, room)| {
                        let disp = room.displayable_events().count();
                        if self.current_room_id.as_ref() != Some(id) {
                            if let Some(disp_at) = self.looking_at_event.get(id) {
                                if *disp_at == disp.saturating_sub(1) {
                                    return Some((id.clone(), disp));
                                }
                            }
                        }
                        None
                    })
                    .collect::<Vec<(RoomId, usize)>>()
                {
                    *self.looking_at_event.get_mut(&room_id).unwrap() = disp.saturating_sub(1);
                }

                return make_thumbnail_commands(&client, thumbnail_urls);
            }
            Message::MatrixGetEventsAroundResponse(response) => {
                let thumbnail_urls = client.process_events_around_response(*response);
                return make_thumbnail_commands(&client, thumbnail_urls);
            }
            Message::RoomChanged(new_room_id) => {
                if let (Some(disp), Some(disp_at)) = (
                    client
                        .get_room(&new_room_id)
                        .map(|room| room.displayable_events().count()),
                    self.looking_at_event.get_mut(&new_room_id),
                ) {
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

    pub fn subscription(&self, client: &Client) -> Subscription<super::Message> {
        let rooms_queued_events = client.rooms_queued_events();
        let mut sub = Subscription::from_recipe(RetrySendEventRecipe {
            client: client.inner(),
            rooms_queued_events,
        })
        .map(|result| super::Message::MainScreen(Message::SendMessageResult(result)));

        if let Some(since) = client.next_batch() {
            sub = Subscription::batch(vec![
                sub,
                Subscription::from_recipe(SyncRecipe {
                    client: client.inner(),
                    since,
                })
                .map(|result| match result {
                    Ok(response) => {
                        super::Message::MainScreen(Message::MatrixSyncResponse(Box::from(response)))
                    }
                    Err(err) => super::Message::MatrixError(Box::new(err)),
                }),
            ]);
        }

        sub
    }

    pub fn on_error(&mut self, _error_string: String) {
        self.logging_out = None;
    }
}

pub type RetrySendEventResult = Vec<(RoomId, Vec<(Uuid, ClientError)>)>;
pub struct RetrySendEventRecipe {
    client: InnerClient,
    rooms_queued_events: Vec<(RoomId, Vec<(Uuid, AnySyncRoomEvent, Option<Duration>)>)>,
}

impl<H, I> iced_futures::subscription::Recipe<H, I> for RetrySendEventRecipe
where
    H: Hasher,
{
    type Output = RetrySendEventResult;

    fn hash(&self, state: &mut H) {
        std::any::TypeId::of::<Self>().hash(state);

        for (id, events) in &self.rooms_queued_events {
            id.hash(state);
            for (transaction_id, _, retry_after) in events {
                transaction_id.hash(state);
                retry_after.hash(state);
            }
        }
    }

    fn stream(self: Box<Self>, _input: BoxStream<I>) -> BoxStream<Self::Output> {
        let future = async move {
            let mut room_errors = Vec::new();

            for (room_id, events) in self.rooms_queued_events {
                let mut transaction_errors = Vec::new();
                for (transaction_id, event, retry_after) in events {
                    if let Some(dur) = retry_after {
                        tokio::time::delay_for(dur).await;
                    }

                    let result = match event {
                        AnySyncRoomEvent::Message(ev) => {
                            Client::send_message(
                                self.client.clone(),
                                ev.content(),
                                room_id.clone(),
                                transaction_id,
                            )
                            .await
                        }
                        _ => unimplemented!(),
                    };

                    if let Err(e) = result {
                        transaction_errors.push((transaction_id, e));
                    }
                }
                room_errors.push((room_id, transaction_errors));
            }

            room_errors
        };

        Box::pin(iced_futures::futures::stream::once(future))
    }
}

pub type SyncResult = Result<sync_events::Response, ClientError>;
pub struct SyncRecipe {
    client: InnerClient,
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
