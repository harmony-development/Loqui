use std::{
    cmp::Ordering,
    path::PathBuf,
    time::{Duration, Instant},
};

use super::{Message as TopLevelMessage, Screen as TopLevelScreen};
use channel::{get_channel_messages, GetChannelMessages};
use chat::Typing;
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
            permissions, GuildId,
        },
        rest::{download_extract_file, upload_extract_id, FileId},
    },
};
use iced_aw::{modal, Modal};
use indexmap::IndexMap;

use chan_guild_list::build_guild_list;
use create_channel::ChannelCreationModal;
use image_viewer::ImageViewerModal;
use logout::LogoutModal;

use crate::{
    client::{
        content::{self, ImageHandle, ThumbnailCache},
        error::ClientError,
        message::{Attachment, Content as IcyContent, Message as IcyMessage},
        Client,
    },
    label, label_button, length, space,
    ui::{
        component::{event_history::SHOWN_MSGS_LIMIT, *},
        style::{Theme, ALT_COLOR, AVATAR_WIDTH, ERROR_COLOR, MESSAGE_SIZE, PADDING, SPACING},
    },
};

use self::quick_switcher::QuickSwitcherModal;

pub mod create_channel;
pub mod image_viewer;
pub mod logout;
pub mod quick_switcher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    EditingMessage(u64),
    EditMessage,
    Normal,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    EditLastMessage,
    QuickSwitch,
    ChangeMode(Mode),
    ClearError,
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
        attachment: Attachment,
        is_thumbnail: bool,
    },
    OpenImageView {
        handle: ImageHandle,
        path: PathBuf,
        name: String,
    },
    OpenUrl(String),
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
    SelectedChannelMenuOption(String),
    SelectedMember(u64),
    LogoutChoice(bool),
    ChannelCreationMessage(create_channel::Message),
    ImageViewMessage(image_viewer::Message),
    QuickSwitchMsg(quick_switcher::Message),
}

#[derive(Debug, Default)]
pub struct MainScreen {
    // Event history area state
    event_history_state: scrollable::State,
    content_open_buts_state: [button::State; SHOWN_MSGS_LIMIT],
    edit_buts_sate: [button::State; SHOWN_MSGS_LIMIT],
    send_file_but_state: button::State,
    composer_state: text_input::State,
    scroll_to_bottom_but_state: button::State,
    embed_buttons_state: [(button::State, button::State); SHOWN_MSGS_LIMIT],

    // Room area state
    channel_menu_state: pick_list::State<String>,
    menu_state: pick_list::State<String>,
    guilds_list_state: scrollable::State,
    guilds_buts_state: Vec<button::State>,
    channels_list_state: scrollable::State,
    channels_buts_state: Vec<button::State>,
    members_buts_state: Vec<button::State>,
    members_list_state: scrollable::State,

    logout_modal: modal::State<LogoutModal>,
    create_channel_modal: modal::State<ChannelCreationModal>,
    pub image_viewer_modal: modal::State<ImageViewerModal>,
    quick_switcher_modal: modal::State<QuickSwitcherModal>,

    // Join room screen state
    /// `None` if the user didn't select a room, `Some(room_id)` otherwise.
    guild_last_channels: IndexMap<u64, u64>,
    current_guild_id: Option<u64>,
    current_channel_id: Option<u64>,
    /// The message the user is currently typing.
    message: String,
    error_text: String,
    error_close_but_state: button::State,
    mode: Mode,
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

        // Create individual widgets

        let guilds_list = if guilds.is_empty() {
            fill_container(label!("No guilds found"))
                .style(theme)
                .into()
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
            .width(length!(= 64))
            .height(length!(+))
            .style(theme)
            .into()];

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
                "Join / Create a Guild".to_string(),
                "Logout".to_string(),
            ],
            Some(current_username),
            Message::SelectedMenuOption,
        )
        .width(length!(+))
        .style(theme);

        if let Some((guild, guild_id)) = self
            .current_guild_id
            .as_ref()
            .map(|id| Some((guilds.get(id)?, *id)))
            .flatten()
        {
            self.members_buts_state
                .resize_with(guild.members.len(), Default::default);

            let mut members_list = Scrollable::new(&mut self.members_list_state)
                .spacing(SPACING)
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

            // Fill sorted_members with content
            for (state, (user_id, member)) in self
                .members_buts_state
                .iter_mut()
                .zip(sorted_members.iter())
            {
                let mut username = label!(&member.username);
                if matches!(member.status, UserStatus::Offline) {
                    username = username.color(ALT_COLOR);
                }
                let mut content: Vec<Element<Message>> = vec![username.into(), space!(w+).into()];
                if let Some(handle) = member
                    .avatar_url
                    .as_ref()
                    .map(|hmc| thumbnail_cache.get_thumbnail(hmc))
                    .flatten()
                {
                    content.push(
                        fill_container(Image::new(handle.clone()).width(length!(+)))
                            .width(length!(= AVATAR_WIDTH))
                            .height(length!(= AVATAR_WIDTH))
                            .style(theme.round())
                            .into(),
                    );
                } else {
                    content.push(
                        fill_container(label!(member
                            .username
                            .chars()
                            .next()
                            .unwrap_or('u')
                            .to_ascii_uppercase()))
                        .width(length!(= AVATAR_WIDTH))
                        .height(length!(= AVATAR_WIDTH))
                        .style(theme.round())
                        .into(),
                    );
                }

                members_list = members_list.push(
                    Button::new(state, Row::with_children(content).align_items(align!(|)))
                        .style(theme.secondary())
                        .on_press(Message::SelectedMember(**user_id))
                        .width(length!(+)),
                );
            }

            // TODO: show user avatar next to name
            let channel_menu = PickList::new(
                &mut self.channel_menu_state,
                vec![
                    guild.name.clone(),
                    "New Channel".to_string(),
                    "Edit Guild".to_string(),
                ],
                Some(guild.name.clone()),
                Message::SelectedChannelMenuOption,
            )
            .width(length!(+))
            .style(theme);

            self.channels_buts_state
                .resize_with(guild.channels.len(), Default::default);

            // Build the room list
            let mut channels_list = if guild.channels.is_empty() {
                // if first_room_id is None, then that means no room found (either cause of filter, or the user aren't in any room)
                // reusing the room_list variable here
                fill_container(label!("No room found")).style(theme).into()
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

            channels_list = Column::with_children(vec![channel_menu.into(), channels_list]).into();

            screen_widgets.push(
                Container::new(channels_list)
                    .width(length!(= 200))
                    .height(length!(+))
                    .style(theme)
                    .into(),
            );

            if let Some((channel, channel_id)) = self
                .current_channel_id
                .as_ref()
                .map(|id| Some((guild.channels.get(id)?, *id)))
                .flatten()
            {
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
                    &mut self.embed_buttons_state,
                    &mut self.edit_buts_sate,
                    self.mode,
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

                        if member.typing_in_channel.map(|(g, c, _)| (g, c))
                            == Some((guild_id, channel_id))
                        {
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
                    space!(w = 6).into(),
                    Row::with_children(vec![
                        space!(w = 9).into(),
                        label!(typing_users_combined).size(14).into(),
                    ])
                    .into(),
                ])
                .height(length!(= 14));

                let send_file_button = Button::new(
                    &mut self.send_file_but_state,
                    label!(iced_aw::Icon::Upload)
                        .font(iced_aw::ICON_FONT)
                        .size((PADDING / 4) * 3 + MESSAGE_SIZE),
                )
                .style(theme.secondary())
                .on_press(Message::SendFiles {
                    guild_id,
                    channel_id,
                });

                let message_composer = match self.mode {
                    Mode::Normal | Mode::EditingMessage(_) => TextInput::new(
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
                    })
                    .width(length!(+))
                    .into(),
                    Mode::EditMessage => fill_container(label!("Select a message to edit..."))
                        .padding((PADDING / 4) * 3)
                        .height(length!(-))
                        .style(theme.secondary())
                        .into(),
                };

                let mut bottom_area_widgets = vec![send_file_button.into(), message_composer];

                if channel.looking_at_message < message_count.saturating_sub(SHOWN_MSGS_LIMIT) {
                    bottom_area_widgets.push(
                        Button::new(
                            &mut self.scroll_to_bottom_but_state,
                            label!(iced_aw::Icon::ArrowDown)
                                .font(iced_aw::ICON_FONT)
                                .size((PADDING / 4) * 3 + MESSAGE_SIZE),
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
                            .width(length!(+)),
                    )
                    .width(length!(+))
                    .padding(PADDING / 2)
                    .into(),
                ]);

                screen_widgets.push(fill_container(message_area).style(theme.secondary()).into());
            } else {
                let no_selected_channel_warning =
                    fill_container(label!("Select a channel").size(35).color(ALT_COLOR))
                        .style(theme.secondary());

                screen_widgets.push(no_selected_channel_warning.into());
            }
            screen_widgets.push(
                Container::new(
                    Column::with_children(vec![menu.into(), members_list.into()])
                        .width(length!(+))
                        .height(length!(+)),
                )
                .width(length!(= 200))
                .height(length!(+))
                .style(theme)
                .into(),
            );
        } else {
            let no_selected_guild_warning =
                fill_container(label!("Select / join a guild").size(35).color(ALT_COLOR))
                    .style(theme.secondary());

            screen_widgets.push(no_selected_guild_warning.into());

            screen_widgets.push(
                Container::new(
                    Column::with_children(vec![menu.into()])
                        .width(length!(+))
                        .height(length!(+)),
                )
                .width(length!(= 200))
                .height(length!(+))
                .style(theme)
                .into(),
            );
        }

        // Layouting

        // Show screen widgets from left to right
        let content = Row::with_children(screen_widgets)
            .height(length!(+))
            .width(length!(+));

        // Show error handling if needed
        let content: Element<Message> = if self.error_text.is_empty() {
            content.into()
        } else {
            Column::with_children(vec![
                fill_container(
                    Row::with_children(vec![
                        label!(&self.error_text)
                            .color(ERROR_COLOR)
                            .width(length!(+))
                            .into(),
                        space!(w+).into(),
                        label_button!(&mut self.error_close_but_state, "Close")
                            .on_press(Message::ClearError)
                            .style(theme.secondary())
                            .into(),
                    ])
                    .padding(PADDING / 4),
                )
                .style(theme)
                .height(length!(-))
                .into(),
                content.into(),
            ])
            .width(length!(+))
            .height(length!(+))
            .align_items(Align::Center)
            .into()
        };

        // Show QuickSwitcherModal
        let content = Modal::new(&mut self.quick_switcher_modal, content, move |state| {
            state.view(theme).map(Message::QuickSwitchMsg)
        })
        .style(theme)
        .backdrop(Message::QuickSwitch)
        .on_esc(Message::QuickSwitch);

        // Show LogoutModal
        let content = Modal::new(&mut self.logout_modal, content, move |state| {
            state.view(theme).map(Message::LogoutChoice)
        })
        .style(theme)
        .backdrop(Message::LogoutChoice(false))
        .on_esc(Message::LogoutChoice(false));

        let content = if self.current_guild_id.is_some() {
            // Show CreateChannelModal, if a guild is selected
            let content = Modal::new(&mut self.create_channel_modal, content, move |state| {
                state.view(theme).map(Message::ChannelCreationMessage)
            })
            .style(theme)
            .backdrop(Message::ChannelCreationMessage(
                create_channel::Message::GoBack,
            ))
            .on_esc(Message::ChannelCreationMessage(
                create_channel::Message::GoBack,
            ));
            if self.current_channel_id.is_some() {
                // Show Image view, if a guild and a channel are selected
                Modal::new(&mut self.image_viewer_modal, content, move |state| {
                    state.view(theme).map(Message::ImageViewMessage)
                })
                .style(theme)
                .backdrop(Message::ImageViewMessage(image_viewer::Message::Close))
                .on_esc(Message::ImageViewMessage(image_viewer::Message::Close))
                .into()
            } else {
                content.into()
            }
        } else {
            content.into()
        };

        content
    }

    pub fn update(
        &mut self,
        msg: Message,
        client: &mut Client,
        thumbnail_cache: &ThumbnailCache,
    ) -> Command<TopLevelMessage> {
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
            Message::QuickSwitch => {
                self.quick_switcher_modal
                    .show(!self.quick_switcher_modal.is_shown());
                let cmd = self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
                let cmd2 = self.update(
                    Message::QuickSwitchMsg(quick_switcher::Message::SearchTermChanged(
                        self.quick_switcher_modal.inner().search_value.clone(),
                    )),
                    client,
                    thumbnail_cache,
                );
                return Command::batch(vec![cmd, cmd2]);
            }
            Message::QuickSwitchMsg(msg) => match msg {
                quick_switcher::Message::SwitchToChannel {
                    guild_id,
                    channel_id,
                } => {
                    let cmd = self.update(Message::GuildChanged(guild_id), client, thumbnail_cache);
                    let cmd2 =
                        self.update(Message::ChannelChanged(channel_id), client, thumbnail_cache);
                    self.quick_switcher_modal.show(false);
                    self.quick_switcher_modal.inner_mut().search_value.clear();
                    return Command::batch(vec![cmd, cmd2]);
                }
                quick_switcher::Message::SwitchToGuild(guild_id) => {
                    let cmd = self.update(Message::GuildChanged(guild_id), client, thumbnail_cache);
                    self.quick_switcher_modal.show(false);
                    self.quick_switcher_modal.inner_mut().search_value.clear();
                    return cmd;
                }
                quick_switcher::Message::SearchTermChanged(new_term) => {
                    let guild = |pattern: &str| {
                        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
                        let mut guilds = client
                            .guilds
                            .iter()
                            .map(|(id, g)| (*id, g.name.as_str()))
                            .flat_map(|(id, name)| {
                                Some((matcher.fuzzy(name, pattern, false)?.0, id, name))
                            })
                            .collect::<Vec<_>>();
                        guilds.sort_unstable_by_key(|(score, _, _)| *score);
                        guilds
                            .into_iter()
                            .rev()
                            .map(|(_, id, name)| quick_switcher::SearchResult::Guild {
                                id,
                                name: name.to_string(),
                            })
                            .collect::<Vec<_>>()
                    };

                    let channel = |pattern: &str| {
                        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
                        let mut channels = client
                            .guilds
                            .iter()
                            .flat_map(|(gid, g)| {
                                g.channels
                                    .iter()
                                    .map(move |(cid, c)| (*gid, *cid, c.name.as_str()))
                                    .flat_map(|(gid, cid, name)| {
                                        Some((
                                            matcher.fuzzy(name, pattern, false)?.0,
                                            gid,
                                            cid,
                                            name,
                                        ))
                                    })
                            })
                            .collect::<Vec<_>>();
                        channels.sort_unstable_by_key(|(score, _, _, _)| *score);
                        channels
                            .into_iter()
                            .rev()
                            .map(
                                |(_, gid, cid, name)| quick_switcher::SearchResult::Channel {
                                    guild_id: gid,
                                    id: cid,
                                    name: name.to_string(),
                                },
                            )
                            .collect()
                    };

                    let term_trimmed = new_term.trim();
                    if term_trimmed.is_empty() {
                        self.quick_switcher_modal.inner_mut().results = self
                            .guild_last_channels
                            .iter()
                            .map(|(gid, cid)| quick_switcher::SearchResult::Channel {
                                guild_id: *gid,
                                id: *cid,
                                name: client
                                    .get_channel(*gid, *cid)
                                    .map(|c| c.name.clone())
                                    .unwrap_or_else(|| "unknown".to_string()),
                            })
                            .collect();
                    } else if let Some(pattern) = new_term.strip_prefix("*").map(str::trim) {
                        self.quick_switcher_modal.inner_mut().results = guild(pattern);
                    } else if let Some(pattern) = new_term.strip_prefix("#").map(str::trim) {
                        self.quick_switcher_modal.inner_mut().results = channel(pattern);
                    } else {
                        self.quick_switcher_modal.inner_mut().results = guild(term_trimmed);
                        let mut channels = channel(term_trimmed);
                        self.quick_switcher_modal
                            .inner_mut()
                            .results
                            .append(&mut channels);
                    }
                    self.quick_switcher_modal.inner_mut().search_value = new_term;
                }
            },
            Message::EditLastMessage => {
                let current_user_id = client.user_id.expect("literally how?");
                if let (Some(guild_id), Some(channel_id)) =
                    (self.current_guild_id, self.current_channel_id)
                {
                    if let Some(mid) = client
                        .get_channel(guild_id, channel_id)
                        .map(|c| {
                            c.messages.iter().rev().find_map(|m| {
                                if m.sender == current_user_id && m.id.id().is_some() {
                                    m.id.id()
                                } else {
                                    None
                                }
                            })
                        })
                        .flatten()
                    {
                        self.mode = Mode::EditMessage;
                        return self.update(
                            Message::ChangeMode(Mode::EditingMessage(mid)),
                            client,
                            thumbnail_cache,
                        );
                    }
                }
            }
            Message::ChangeMode(mode) => {
                if let (Mode::EditMessage, Mode::EditingMessage(mid)) = (self.mode, mode) {
                    if let (Some(gid), Some(cid)) = (self.current_guild_id, self.current_channel_id)
                    {
                        self.composer_state.focus();
                        if let Some(msg) = client
                            .get_channel(gid, cid)
                            .map(|c| c.messages.iter_mut().rev().find(|m| m.id.id() == Some(mid)))
                            .flatten()
                        {
                            if let IcyContent::Text(text) = &msg.content {
                                self.message = text.clone();
                            }
                        }
                    } else {
                        self.composer_state.unfocus();
                        self.message.clear();
                    }
                }
                if let (Mode::EditingMessage(_), Mode::Normal) = (self.mode, mode) {
                    self.composer_state.unfocus();
                    self.message.clear();
                }
                self.mode = mode;
            }
            Message::ClearError => {
                self.error_text.clear();
            }
            Message::OpenUrl(url) => {
                open::that_in_background(url);
            }
            Message::OpenImageView { handle, path, name } => {
                self.image_viewer_modal.show(true);
                self.image_viewer_modal.inner_mut().image_handle = Some((handle, (path, name)));
                return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
            }
            Message::ImageViewMessage(msg) => {
                let (cmd, go_back) = self.image_viewer_modal.inner_mut().update(msg);

                if go_back {
                    self.image_viewer_modal.show(false);
                }

                return cmd;
            }
            Message::ChannelCreationMessage(msg) => {
                let (cmd, go_back) = self.create_channel_modal.inner_mut().update(
                    msg,
                    self.current_guild_id.unwrap(),
                    &client,
                );

                if go_back {
                    self.create_channel_modal.show(false);
                }

                return cmd;
            }
            Message::LogoutChoice(confirm) => {
                self.logout_modal.show(false);
                return self.logout_modal.inner_mut().update(confirm, client);
            }
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
                            reached_top,
                            loading_messages_history,
                            looking_at_message,
                        )) = client
                            .get_channel(guild_id, channel_id)
                            .map(|channel| {
                                Some((
                                    channel.messages.first().map(|m| m.id.id()).flatten(),
                                    channel.messages.len(),
                                    channel.reached_top,
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

                            if !reached_top && *looking_at_message < 2 && !*loading_messages_history
                            {
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
                                        .map_or_else(
                                            |err| TopLevelMessage::Error(Box::new(err.into())),
                                            |response| {
                                                TopLevelMessage::GetEventsBackwardsResponse {
                                                    messages: response.messages,
                                                    reached_top: response.reached_top,
                                                    guild_id,
                                                    channel_id,
                                                }
                                            },
                                        )
                                    },
                                    |result| result,
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
                tracing::trace!("member: {}", user_id);
            }
            Message::SelectedChannelMenuOption(option) => match option.as_str() {
                "New Channel" => {
                    self.create_channel_modal.show(true);
                    self.error_text.clear();
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
                }
                "Edit Guild" => {
                    let guild_id = self.current_guild_id.unwrap();
                    let client_inner = client.inner().clone();
                    return Command::perform(
                        async move {
                            return permissions::query_has_permission(
                                &client_inner,
                                permissions::QueryPermissions::new(
                                    guild_id,
                                    "guild.manage.change-information".to_string(),
                                ),
                            )
                            .await;
                        },
                        move |result| match result {
                            Ok(x) => {
                                if x.ok {
                                    TopLevelMessage::PushScreen(Box::new(
                                        TopLevelScreen::GuildSettings(super::GuildSettings::new(
                                            guild_id,
                                        )),
                                    ))
                                } else {
                                    TopLevelMessage::Error(Box::new(ClientError::Custom(
                                        "Not permitted to edit guild information".to_string(),
                                    )))
                                }
                            }
                            Err(x) => TopLevelMessage::Error(Box::new(x.into())),
                        },
                    );
                }
                _ => {}
            },
            Message::SelectedMenuOption(option) => match option.as_str() {
                "Logout" => {
                    self.logout_modal.show(true);
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
                }
                "Join / Create a Guild" => {
                    return TopLevelScreen::push_screen_cmd(TopLevelScreen::GuildDiscovery(
                        super::GuildDiscovery::default(),
                    ));
                }
                _ => {}
            },
            Message::ComposerMessageChanged(new_msg) => {
                self.message = new_msg;

                if let (Some(guild_id), Some(channel_id), Some(typing)) = (
                    self.current_guild_id,
                    self.current_channel_id,
                    client
                        .user_id
                        .map(|id| client.get_member(id))
                        .flatten()
                        .map(|member| &mut member.typing_in_channel),
                ) {
                    if Some((guild_id, channel_id)) != typing.map(|(g, c, _)| (g, c))
                        || typing.map_or(false, |(_, _, since)| since.elapsed().as_secs() >= 5)
                    {
                        *typing = Some((guild_id, channel_id, Instant::now()));
                        let inner = client.inner().clone();
                        return Command::perform(
                            async move { chat::typing(&inner, Typing::new(guild_id, channel_id)).await },
                            |result| {
                                result.map_or_else(
                                    |err| TopLevelMessage::Error(Box::new(err.into())),
                                    |_| TopLevelMessage::Nothing,
                                )
                            },
                        );
                    }
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
                attachment,
                is_thumbnail,
            } => {
                let maybe_thumb = thumbnail_cache.get_thumbnail(&attachment.id).cloned();
                let content_path = client.content_store().content_path(&attachment.id);
                return if content_path.exists() {
                    Command::perform(
                        async move {
                            Ok(if is_thumbnail && maybe_thumb.is_none() {
                                let data = tokio::fs::read(&content_path).await?;
                                let bgra = image::load_from_memory(&data).unwrap().into_bgra8();

                                TopLevelMessage::DownloadedThumbnail {
                                    data: attachment,
                                    thumbnail: ImageHandle::from_pixels(
                                        bgra.width(),
                                        bgra.height(),
                                        bgra.into_vec(),
                                    ),
                                    open: true,
                                }
                            } else if is_thumbnail {
                                TopLevelMessage::MainScreen(Message::OpenImageView {
                                    handle: maybe_thumb.unwrap(),
                                    path: content_path,
                                    name: attachment.name,
                                })
                            } else {
                                open::that_in_background(content_path);
                                TopLevelMessage::Nothing
                            })
                        },
                        |result| result.unwrap_or_else(|err| TopLevelMessage::Error(Box::new(err))),
                    )
                } else {
                    let inner = client.inner().clone();
                    Command::perform(
                        async move {
                            let downloaded_file =
                                download_extract_file(&inner, attachment.id.clone()).await?;
                            tokio::fs::write(&content_path, downloaded_file.data()).await?;
                            let bgra = image::load_from_memory(downloaded_file.data())
                                .unwrap()
                                .into_bgra8();

                            Ok(if is_thumbnail && maybe_thumb.is_none() {
                                TopLevelMessage::DownloadedThumbnail {
                                    data: attachment,
                                    thumbnail: ImageHandle::from_pixels(
                                        bgra.width(),
                                        bgra.height(),
                                        bgra.into_vec(),
                                    ),
                                    open: true,
                                }
                            } else if is_thumbnail {
                                TopLevelMessage::MainScreen(Message::OpenImageView {
                                    handle: maybe_thumb.unwrap(),
                                    path: content_path,
                                    name: attachment.name,
                                })
                            } else {
                                open::that_in_background(content_path);
                                TopLevelMessage::Nothing
                            })
                        },
                        |result| result.unwrap_or_else(|err| TopLevelMessage::Error(Box::new(err))),
                    )
                };
            }
            Message::SendMessageComposer {
                guild_id,
                channel_id,
            } => {
                if !self.message.trim().is_empty() {
                    if let Mode::EditingMessage(message_id) = self.mode {
                        let new_content: String =
                            self.message.drain(..).collect::<String>().trim().into();
                        if let Some(msg) = client
                            .get_channel(guild_id, channel_id)
                            .map(|c| {
                                c.messages
                                    .iter_mut()
                                    .find(|m| m.id.id() == Some(message_id))
                            })
                            .flatten()
                        {
                            msg.being_edited = Some(new_content.clone());
                        }
                        self.mode = Mode::Normal;
                        return client.edit_msg_cmd(guild_id, channel_id, message_id, new_content);
                    } else if let Mode::Normal = self.mode {
                        let message = IcyMessage {
                            content: IcyContent::Text(
                                self.message.drain(..).collect::<String>().trim().into(),
                            ),
                            sender: client.user_id.unwrap(),
                            ..Default::default()
                        };
                        if let Some(cmd) = client.send_msg_cmd(
                            guild_id,
                            channel_id,
                            Duration::from_secs(0),
                            message,
                        ) {
                            scroll_to_bottom(client, guild_id, channel_id);
                            self.event_history_state.scroll_to_bottom();
                            return cmd;
                        }
                    }
                } else if let Mode::EditingMessage(mid) = self.mode {
                    self.mode = Mode::Normal;
                    return client.delete_msg_cmd(guild_id, channel_id, mid);
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

                                    match send_result.map(FileId::Id) {
                                        Ok(id) => {
                                            if let Err(err) = tokio::fs::hard_link(
                                                handle.path(),
                                                content_store.content_path(&id),
                                            )
                                            .await
                                            {
                                                tracing::warn!("An IO error occured while hard linking a file you tried to upload (this may result in a duplication of the file): {}", err);
                                            }
                                            ids.push((id, file_mimetype, filename, filesize));
                                        }
                                        Err(err) => {
                                            tracing::error!(
                                                "An error occured while trying to upload a file: {}",
                                                err
                                            );
                                        }
                                    }
                                }
                                Err(err) => {
                                    tracing::error!(
                                        "An IO error occured while trying to upload a file: {}",
                                        err
                                    );
                                }
                            }
                        }
                        Ok(TopLevelMessage::SendMessage {
                            message: IcyMessage {
                                content: IcyContent::Files(
                                    ids.into_iter()
                                        .map(|(id, kind, name, size)| Attachment {
                                            id,
                                            kind,
                                            name,
                                            size: size as u32,
                                        })
                                        .collect(),
                                ),
                                sender,
                                ..Default::default()
                            },
                            retry_after: Duration::from_secs(0),
                            guild_id,
                            channel_id,
                        })
                    },
                    |result| {
                        result.unwrap_or_else(|err| {
                            if matches!(err, ClientError::Custom(_)) {
                                tracing::error!("{}", err);
                                TopLevelMessage::Nothing
                            } else {
                                TopLevelMessage::Error(Box::new(err))
                            }
                        })
                    },
                );
            }
            Message::GuildChanged(guild_id) => {
                self.mode = Mode::Normal;
                self.message.clear();
                self.current_guild_id = Some(guild_id);
                if let Some(guild) = client.get_guild(guild_id) {
                    if guild.channels.is_empty() {
                        let inner = client.inner().clone();

                        return Command::perform(
                            async move {
                                let guildid = GuildId::new(guild_id);
                                let channels_list =
                                    get_guild_channels(&inner, guildid).await?.channels;
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
                                        member_id,
                                        guild_id,
                                    }));
                                }

                                Ok(events)
                            },
                            |result| {
                                result.map_or_else(
                                    |err| TopLevelMessage::Error(Box::new(err)),
                                    TopLevelMessage::EventsReceived,
                                )
                            },
                        );
                    } else {
                        self.current_channel_id = self
                            .guild_last_channels
                            .get(&guild_id)
                            .copied()
                            .or_else(|| Some(*guild.channels.first().unwrap().0));
                    }
                }
            }
            Message::ChannelChanged(channel_id) => {
                self.mode = Mode::Normal;
                self.message.clear();
                self.current_channel_id = Some(channel_id);
                self.guild_last_channels
                    .insert(self.current_guild_id.unwrap(), channel_id);
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
                            |result| {
                                result.map_or_else(
                                    |err| TopLevelMessage::Error(Box::new(err)),
                                    TopLevelMessage::EventsReceived,
                                )
                            },
                        );
                    }
                }
            }
        }

        Command::none()
    }

    pub fn subscription(&self) -> Subscription<TopLevelMessage> {
        use iced_native::{
            keyboard::{self, KeyCode},
            Event,
        };

        fn filter_events(
            ev: Event,
            _status: iced_native::event::Status,
        ) -> Option<TopLevelMessage> {
            match ev {
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key_code: KeyCode::Escape,
                    ..
                }) => Some(TopLevelMessage::MainScreen(Message::ChangeMode(
                    Mode::Normal,
                ))),
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key_code: KeyCode::K,
                    modifiers: keyboard::Modifiers { control: true, .. },
                }) => Some(TopLevelMessage::MainScreen(Message::QuickSwitch)),
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key_code: KeyCode::E,
                    modifiers: keyboard::Modifiers { control: true, .. },
                }) => Some(TopLevelMessage::MainScreen(Message::ChangeMode(
                    Mode::EditMessage,
                ))),
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key_code: KeyCode::Up,
                    ..
                }) => Some(TopLevelMessage::MainScreen(Message::EditLastMessage)),
                _ => None,
            }
        }

        iced_native::subscription::events_with(filter_events)
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        self.error_text = error.to_string();
        self.logout_modal.show(false);

        Command::batch(vec![
            self.create_channel_modal.inner_mut().on_error(&error),
            self.logout_modal.inner_mut().on_error(&error),
        ])
    }
}
