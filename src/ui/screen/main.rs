use std::{cmp::Ordering, time::Duration};

use crate::{
    client::{
        content::{self, ImageHandle, ThumbnailCache},
        error::ClientError,
        message::{Attachment, Message as IcyMessage},
        Client,
    },
    ui::{
        component::{event_history::SHOWN_MSGS_LIMIT, *},
        style::{Theme, MESSAGE_SIZE, PADDING, SPACING},
    },
};
use channel::{get_channel_messages, GetChannelMessages};
use chat::Typing;
use content::ContentType;
use harmony_rust_sdk::{
    api::{
        chat::event::{ChannelCreated, Event, MemberJoined, MessageSent},
        harmonytypes::UserStatus,
    },
    client::api::{
        chat::{
            self,
            channel::{self, get_guild_channels, GetChannelMessagesSelfBuilder},
            guild::get_guild_members,
            GuildId,
        },
        rest::{download, upload_extract_id, FileId},
    },
};
use room_list::build_guild_list;

#[derive(Debug, Clone)]
pub enum Message {
    /// Sent when the user wants to send a message.
    SendMessageComposer {
        guild_id: u64,
        channel_id: u64,
    },
    /// Sent when the user wants to send a file.
    SendFiles {
        guild_id: u64,
        channel_id: u64,
    },
    /// Sent when user makes a change to the message they are composing.
    ComposerMessageChanged(String),
    ScrollToBottom(u64),
    OpenContent {
        content_url: FileId,
        is_thumbnail: bool,
    },
    /// Sent when the user selects a different guild.
    GuildChanged(u64),
    /// Sent twhen the user selects a different channel.
    ChannelChanged(u64),
    /// Sent when the user scrolls the message history.
    MessageHistoryScrolled {
        prev_scroll_perc: f32,
        scroll_perc: f32,
    },
    /// Sent when the user selects an option from the bottom menu.
    SelectedMenuOption(String),
    SelectedMember(u64),
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
    guilds_list_state: scrollable::State,
    guilds_buts_state: Vec<button::State>,
    channels_list_state: scrollable::State,
    channels_buts_state: Vec<button::State>,
    members_buts_state: Vec<button::State>,
    members_list_state: scrollable::State,

    // Join room screen state
    /// `None` if the user didn't select a room, `Some(room_id)` otherwise.
    current_guild_id: Option<u64>,
    current_channel_id: Option<u64>,
    /// The message the user is currently typing.
    message: String,
}

impl MainScreen {
    pub fn view(
        &mut self,
        theme: Theme,
        client: &Client,
        thumbnail_cache: &ThumbnailCache,
    ) -> Element<Message> {
        let guilds = &client.guilds;

        // Resize and (if extended) initialize new button states for new rooms
        self.guilds_buts_state
            .resize_with(guilds.len(), Default::default);

        let guilds_list = if guilds.is_empty() {
            fill_container(label("No guilds found")).style(theme).into()
        } else {
            build_guild_list(
                guilds,
                thumbnail_cache,
                self.current_guild_id,
                &mut self.guilds_list_state,
                &mut self.guilds_buts_state,
                Message::GuildChanged,
                theme,
            )
        };

        let mut screen_widgets = vec![Container::new(guilds_list)
            .width(Length::Units(64))
            .height(Length::Fill)
            .style(theme)
            .into()];

        if let Some((guild, guild_id)) = self
            .current_guild_id
            .as_ref()
            .map(|id| Some((guilds.get(id)?, *id)))
            .flatten()
        {
            let current_user_id = client.user_id.unwrap();
            let current_username = client
                .members
                .get(&current_user_id)
                .map_or_else(|| String::from("unknown"), |member| member.username.clone());

            // TODO: show user avatar next to name
            let menu = PickList::new(
                &mut self.menu_state,
                vec![
                    current_username.clone(),
                    "Join Room".to_string(),
                    "Logout".to_string(),
                ],
                Some(current_username),
                Message::SelectedMenuOption,
            )
            .width(Length::Fill)
            .style(theme);

            self.members_buts_state
                .resize_with(guild.members.len(), Default::default);

            let mut members_list = Scrollable::new(&mut self.members_list_state)
                .spacing(SPACING * 2)
                .padding(PADDING);

            let mut sorted_members = guild
                .members
                .iter()
                .flat_map(|id| Some((id, client.members.get(id)?)))
                .collect::<Vec<_>>();
            sorted_members.sort_by_key(|(_, member)| member.username.as_str());
            sorted_members.sort_by(|(_, member), (_, other_member)| {
                let offline = matches!(member.status, UserStatus::Offline);
                let other_offline = matches!(other_member.status, UserStatus::Offline);

                if !offline && other_offline {
                    Ordering::Less
                } else if offline && !other_offline {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            });

            for (state, (user_id, member)) in self
                .members_buts_state
                .iter_mut()
                .zip(sorted_members.iter())
            {
                let mut username = label(&member.username);
                if matches!(member.status, UserStatus::Offline) {
                    username = username.color(color!(128, 128, 128));
                }
                let mut content: Vec<Element<Message>> =
                    vec![username.into(), Space::with_width(Length::Fill).into()];
                if let Some(handle) = member
                    .avatar_url
                    .as_ref()
                    .map(|hmc| thumbnail_cache.get_thumbnail(hmc))
                    .flatten()
                {
                    content.push(
                        fill_container(Image::new(handle.clone()).width(Length::Fill))
                            .width(Length::Units(32))
                            .height(Length::Units(32))
                            .style(theme.round())
                            .into(),
                    );
                } else {
                    content.push(
                        fill_container(label(
                            member
                                .username
                                .chars()
                                .next()
                                .unwrap_or('u')
                                .to_ascii_uppercase(),
                        ))
                        .width(Length::Units(32))
                        .height(Length::Units(32))
                        .style(theme.round())
                        .into(),
                    );
                }

                members_list = members_list.push(
                    Button::new(
                        state,
                        Row::with_children(content).align_items(Align::Center),
                    )
                    .style(theme.secondary())
                    .on_press(Message::SelectedMember(**user_id))
                    .width(Length::Fill),
                );
            }

            self.channels_buts_state
                .resize_with(guild.channels.len(), Default::default);

            // Build the room list
            let channels_list = if guild.channels.is_empty() {
                // if first_room_id is None, then that means no room found (either cause of filter, or the user aren't in any room)
                // reusing the room_list variable here
                fill_container(label("No room found")).style(theme).into()
            } else {
                build_channel_list(
                    &guild.channels,
                    self.current_channel_id,
                    &mut self.channels_list_state,
                    &mut self.channels_buts_state,
                    Message::ChannelChanged,
                    theme,
                )
            };

            screen_widgets.push(
                Container::new(channels_list)
                    .width(Length::Units(200))
                    .height(Length::Fill)
                    .style(theme)
                    .into(),
            );

            if let Some((channel, channel_id)) = self
                .current_channel_id
                .as_ref()
                .map(|id| Some((guild.channels.get(id)?, *id)))
                .flatten()
            {
                let message_composer = TextInput::new(
                    &mut self.composer_state,
                    "Enter your message here...",
                    self.message.as_str(),
                    Message::ComposerMessageChanged,
                )
                .padding((PADDING / 4) * 3)
                .size(MESSAGE_SIZE)
                .style(theme.secondary())
                .on_submit(Message::SendMessageComposer {
                    guild_id,
                    channel_id,
                });

                let message_count = channel.messages.len();
                let message_history_list = build_event_history(
                    client.content_store(),
                    thumbnail_cache,
                    channel,
                    &client.members,
                    current_user_id,
                    channel.looking_at_message,
                    &mut self.event_history_state,
                    &mut self.content_open_buts_state,
                    theme,
                );

                let mut typing_users_combined = String::new();
                let typing_names = sorted_members
                    .iter()
                    .flat_map(|(id, member)| {
                        // Remove own user id from the list (if its there)
                        if **id == current_user_id {
                            return None;
                        }

                        if member.typing_in_channel == Some(channel_id) {
                            Some(member.username.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();

                let typing_members_count = typing_names.len();

                for (index, member_name) in typing_names.iter().enumerate() {
                    if index > 2 {
                        typing_users_combined += " and others are typing...";
                        break;
                    }

                    typing_users_combined += member_name;

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
                .on_press(Message::SendFiles {
                    guild_id,
                    channel_id,
                });

                let mut bottom_area_widgets = vec![
                    send_file_button.into(),
                    message_composer.width(Length::Fill).into(),
                ];

                if channel.looking_at_message < message_count.saturating_sub(SHOWN_MSGS_LIMIT) {
                    bottom_area_widgets.push(
                        Button::new(
                            &mut self.scroll_to_bottom_but_state,
                            label("↡").size((PADDING / 4) * 3 + MESSAGE_SIZE),
                        )
                        .style(theme.secondary())
                        .on_press(Message::ScrollToBottom(channel_id))
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
            } else {
                let no_selected_channel_warning = fill_container(
                    label("Select a channel")
                        .size(35)
                        .color(color!(128, 128, 128)),
                )
                .style(theme.secondary());

                screen_widgets.push(no_selected_channel_warning.into());
            }
            screen_widgets.push(
                Container::new(
                    Column::with_children(vec![menu.into(), members_list.into()])
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .width(Length::Units(200))
                .height(Length::Fill)
                .style(theme)
                .into(),
            );
        } else {
            let no_selected_guild_warning = fill_container(
                label("Select / join a guild")
                    .size(35)
                    .color(color!(128, 128, 128)),
            )
            .style(theme.secondary());

            screen_widgets.push(no_selected_guild_warning.into());
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
        fn scroll_to_bottom(client: &mut Client, guild_id: u64, channel_id: u64) {
            if let Some((disp, looking_at_message)) = client
                .guilds
                .get_mut(&guild_id)
                .map(|guild| guild.channels.get_mut(&channel_id))
                .flatten()
                .map(|channel| (channel.messages.len(), &mut channel.looking_at_message))
            {
                *looking_at_message = disp.saturating_sub(1);
            }
        }

        match msg {
            Message::MessageHistoryScrolled {
                prev_scroll_perc,
                scroll_perc,
            } => {
                if let (Some(guild_id), Some(channel_id)) =
                    (self.current_guild_id, self.current_channel_id)
                {
                    if scroll_perc < 0.01 && scroll_perc <= prev_scroll_perc {
                        if let Some((
                            oldest_msg_id,
                            disp,
                            loading_messages_history,
                            looking_at_message,
                        )) = client
                            .get_channel(guild_id, channel_id)
                            .map(|channel| {
                                Some((
                                    channel.messages.first().map(|m| m.id.id()).flatten(),
                                    channel.messages.len(),
                                    &mut channel.loading_messages_history,
                                    &mut channel.looking_at_message,
                                ))
                            })
                            .flatten()
                        {
                            if *looking_at_message == disp.saturating_sub(1) {
                                *looking_at_message = disp.saturating_sub(SHOWN_MSGS_LIMIT + 1);
                            } else {
                                *looking_at_message = looking_at_message.saturating_sub(1);
                            }

                            if *looking_at_message < 2 && !*loading_messages_history {
                                *loading_messages_history = true;
                                let inner = client.inner().clone();
                                return Command::perform(
                                    async move {
                                        channel::get_channel_messages(
                                            &inner,
                                            GetChannelMessages::new(guild_id, channel_id)
                                                .before_message(oldest_msg_id.unwrap_or_default()),
                                        )
                                        .await
                                    },
                                    move |result| match result {
                                        Ok(response) => {
                                            super::Message::GetEventsBackwardsResponse {
                                                messages: response.messages,
                                                reached_top: response.reached_top,
                                                guild_id,
                                                channel_id,
                                            }
                                        }

                                        Err(err) => super::Message::Error(Box::new(err.into())),
                                    },
                                );
                            }
                        }
                    } else if scroll_perc > 0.99 && scroll_perc >= prev_scroll_perc {
                        if let Some((disp, looking_at_message)) =
                            client.get_channel(guild_id, channel_id).map(|channel| {
                                (channel.messages.len(), &mut channel.looking_at_message)
                            })
                        {
                            if *looking_at_message > disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                                *looking_at_message = disp.saturating_sub(1);
                            } else {
                                *looking_at_message =
                                    looking_at_message.saturating_add(1).min(disp);
                            }
                        }
                    }
                }
            }
            Message::SelectedMember(user_id) => {
                log::trace!("member: {}", user_id);
            }
            Message::SelectedMenuOption(option) => {
                return match option.as_str() {
                    "Logout" => Command::perform(async {}, |_| {
                        super::Message::PushScreen(Box::new(super::Screen::Logout(
                            super::LogoutScreen::default(),
                        )))
                    }),
                    "Join Room" => Command::perform(async {}, |_| {
                        super::Message::PushScreen(Box::new(super::Screen::GuildDiscovery(
                            super::GuildDiscovery::default(),
                        )))
                    }),
                    _ => Command::none(),
                }
            }
            Message::ComposerMessageChanged(new_msg) => {
                self.message = new_msg;

                if let (Some(guild_id), Some(channel_id)) =
                    (self.current_guild_id, self.current_channel_id)
                {
                    let inner = client.inner().clone();
                    return Command::perform(
                        async move { chat::typing(&inner, Typing::new(guild_id, channel_id)).await },
                        |result| match result {
                            Ok(_) => super::Message::Nothing,
                            Err(err) => super::Message::Error(Box::new(err.into())),
                        },
                    );
                }
            }
            Message::ScrollToBottom(sent_channel_id) => {
                if let (Some(guild_id), Some(channel_id)) =
                    (self.current_guild_id, self.current_channel_id)
                {
                    if sent_channel_id == channel_id {
                        scroll_to_bottom(client, guild_id, channel_id);
                        self.event_history_state.scroll_to_bottom();
                    }
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
                    let inner = client.inner().clone();
                    Command::perform(
                        async move {
                            use harmony_rust_sdk::client::error::ClientError as InnerClientError;
                            let download_task = download(&inner, content_url.clone());

                            let raw_data = download_task
                                .await?
                                .bytes()
                                .await
                                .map_err(InnerClientError::Reqwest)?;
                            tokio::fs::write(&content_path, &raw_data).await?;
                            Ok((
                                content_path,
                                if is_thumbnail && !thumbnail_exists {
                                    Some((content_url, raw_data))
                                } else {
                                    None
                                },
                            ))
                        },
                        |result| match result {
                            Ok((content_path, thumbnail)) => {
                                open::that_in_background(content_path);
                                if let Some((content_url, raw_data)) = thumbnail {
                                    super::Message::DownloadedThumbnail {
                                        thumbnail_url: content_url,
                                        thumbnail: ImageHandle::from_memory(raw_data.to_vec()),
                                    }
                                } else {
                                    super::Message::Nothing
                                }
                            }
                            Err(err) => super::Message::Error(Box::new(err)),
                        },
                    )
                };
            }
            Message::SendMessageComposer {
                guild_id,
                channel_id,
            } => {
                if !self.message.is_empty() {
                    let message = IcyMessage {
                        content: self.message.drain(..).collect::<String>(),
                        sender: client.user_id.unwrap(),
                        ..Default::default()
                    };
                    scroll_to_bottom(client, guild_id, channel_id);
                    self.event_history_state.scroll_to_bottom();
                    return Command::perform(async move { message }, move |message| {
                        super::Message::SendMessage {
                            message,
                            retry_after: Duration::from_secs(0),
                            guild_id,
                            channel_id,
                        }
                    });
                }
            }
            Message::SendFiles {
                guild_id,
                channel_id,
            } => {
                let inner = client.inner().clone();
                let content_store = client.content_store_arc();
                let sender = client.user_id.unwrap();

                return Command::perform(
                    async move {
                        let handles =
                            rfd::AsyncFileDialog::new()
                                .pick_files()
                                .await
                                .ok_or_else(|| {
                                    ClientError::Custom("File selection error".to_string())
                                })?;
                        let mut ids = Vec::with_capacity(handles.len());

                        for handle in handles {
                            match tokio::fs::read(handle.path()).await {
                                Ok(data) => {
                                    let file_mimetype = content::infer_type_from_bytes(&data);
                                    let filename = content::get_filename(handle.path()).to_string();
                                    let filesize = data.len();

                                    let send_result = upload_extract_id(
                                        &inner,
                                        filename.clone(),
                                        file_mimetype.clone(),
                                        data,
                                    )
                                    .await;

                                    match send_result.map(|id| FileId::Hmc(inner.make_hmc(id))) {
                                        Ok(id) => {
                                            if let Err(err) = tokio::fs::hard_link(
                                                handle.path(),
                                                content_store.content_path(&id),
                                            )
                                            .await
                                            {
                                                log::warn!("An IO error occured while hard linking a file you tried to upload (this may result in a duplication of the file): {}", err);
                                            }
                                            ids.push((id, file_mimetype, filename, filesize));
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
                        Ok(ids)
                    },
                    move |result| match result {
                        Ok(hmcs) => super::Message::SendMessage {
                            message: IcyMessage {
                                attachments: hmcs
                                    .into_iter()
                                    .map(|(id, kind, name, size)| Attachment {
                                        id,
                                        kind: ContentType::new(&kind),
                                        name,
                                        size: size as u32,
                                    })
                                    .collect(),
                                sender,
                                ..Default::default()
                            },
                            retry_after: Duration::from_secs(0),
                            guild_id,
                            channel_id,
                        },
                        Err(err) => super::Message::Error(Box::new(err)),
                    },
                );
            }
            Message::GuildChanged(guild_id) => {
                self.current_guild_id = Some(guild_id);
                if client
                    .get_guild(guild_id)
                    .map_or(false, |guild| guild.channels.is_empty())
                {
                    let inner = client.inner().clone();

                    return Command::perform(
                        async move {
                            let guildid = GuildId::new(guild_id);
                            let channels_list = get_guild_channels(&inner, guildid).await?.channels;
                            let mut events = Vec::with_capacity(channels_list.len());
                            for channel in channels_list {
                                events.push(Event::CreatedChannel(ChannelCreated {
                                    guild_id,
                                    channel_id: channel.channel_id,
                                    is_category: channel.is_category,
                                    name: channel.channel_name,
                                    metadata: channel.metadata,
                                    ..Default::default()
                                }));
                            }

                            let members = get_guild_members(&inner, guildid).await?.members;
                            events.reserve(members.len());
                            for member_id in members {
                                events.push(Event::JoinedMember(MemberJoined {
                                    guild_id,
                                    member_id,
                                }));
                            }

                            Ok(events)
                        },
                        |result| match result {
                            Ok(events) => super::Message::EventsReceived(events),
                            Err(err) => super::Message::Error(Box::new(err)),
                        },
                    );
                }
            }
            Message::ChannelChanged(channel_id) => {
                self.current_channel_id = Some(channel_id);
                if let Some((disp, disp_at)) = self
                    .current_guild_id
                    .map(|guild_id| client.get_channel(guild_id, channel_id))
                    .flatten()
                    .map(|channel| (channel.messages.len(), &mut channel.looking_at_message))
                {
                    if *disp_at >= disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                        *disp_at = disp.saturating_sub(1);
                        self.event_history_state.scroll_to_bottom();
                    }
                    if disp == 0 {
                        let inner = client.inner().clone();
                        let guild_id = self.current_guild_id.unwrap();
                        return Command::perform(
                            async move {
                                let messages = get_channel_messages(
                                    &inner,
                                    GetChannelMessages::new(guild_id, channel_id),
                                )
                                .await?
                                .messages;
                                let events = messages
                                    .into_iter()
                                    .map(|msg| {
                                        Event::SentMessage(Box::new(MessageSent {
                                            message: Some(msg),
                                            ..Default::default()
                                        }))
                                    })
                                    .rev()
                                    .collect();
                                Ok(events)
                            },
                            |result| match result {
                                Ok(events) => super::Message::EventsReceived(events),
                                Err(err) => super::Message::Error(Box::new(err)),
                            },
                        );
                    }
                }
            }
        }

        Command::none()
    }
}
