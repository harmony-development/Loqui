use std::{
    cmp::Ordering,
    convert::identity,
    fmt::{self, Display, Formatter},
    ops::Not,
    path::PathBuf,
    time::{Duration, Instant},
};

use super::{Message as TopLevelMessage, Screen as TopLevelScreen};
use channel::{get_channel_messages, GetChannelMessages};
use chat::Typing;
use client::{
    bool_ext::BoolExt,
    error::ClientResult,
    harmony_rust_sdk::{
        api::{
            chat::{
                event::{ChannelCreated, Event, MemberJoined, MessageSent, PermissionUpdated},
                GetChannelMessagesResponse,
            },
            harmonytypes::UserStatus,
        },
        client::api::{
            chat::{
                self,
                channel::{self, get_guild_channels, GetChannelMessagesSelfBuilder},
                guild::{self, get_guild_members},
                permissions::{query_has_permission, QueryPermissions, QueryPermissionsSelfBuilder},
                profile::{self, ProfileUpdate},
                GuildId,
            },
            rest::download_extract_file,
        },
    },
    message::MessageId,
    smol_str::SmolStr,
    tracing::error,
    IndexMap, OptionExt,
};
use iced::futures::future::ready;
use iced_aw::{modal, Modal};

use chan_guild_list::build_guild_list;
use create_channel::ChannelCreationModal;
use help::HelpModal;
use image_viewer::ImageViewerModal;
use logout::LogoutModal;
use profile_edit::ProfileEditModal;

use crate::{
    client::{
        error::ClientError,
        message::{Attachment, Content as IcyContent, Message as IcyMessage},
        Client,
    },
    component::{
        event_history::{EventHistoryButsState, SHOWN_MSGS_LIMIT},
        *,
    },
    label, label_button, length,
    screen::{map_send_msg, map_to_nothing, truncate_string, ClientExt, ResultExt},
    space,
    style::{Theme, ALT_COLOR, AVATAR_WIDTH, ERROR_COLOR, MESSAGE_SIZE, PADDING, SPACING},
};

use self::{edit_channel::UpdateChannelModal, quick_switcher::QuickSwitcherModal};

pub mod create_channel;
pub mod edit_channel;
pub mod help;
pub mod image_viewer;
pub mod logout;
pub mod profile_edit;
pub mod quick_switcher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    EditingMessage(u64),
    Normal,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileMenuOption {
    EditProfile,
    Help,
    Logout,
    SwitchAccount,
    Exit,
    Custom(SmolStr),
}

impl Display for ProfileMenuOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let w = match self {
            ProfileMenuOption::EditProfile => "Edit Profile",
            ProfileMenuOption::Help => "Help",
            ProfileMenuOption::Logout => "Logout",
            ProfileMenuOption::SwitchAccount => "Switch Account",
            ProfileMenuOption::Exit => "Exit",
            ProfileMenuOption::Custom(s) => s.as_str(),
        };

        f.write_str(w)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildMenuOption {
    NewChannel,
    EditGuild,
    CopyGuildId,
    LeaveGuild,
    Custom(SmolStr),
}

impl Display for GuildMenuOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let w = match self {
            GuildMenuOption::NewChannel => "New Channel",
            GuildMenuOption::EditGuild => "Edit Guild",
            GuildMenuOption::CopyGuildId => "Copy Guild ID",
            GuildMenuOption::LeaveGuild => "Leave Guild",
            GuildMenuOption::Custom(s) => s.as_str(),
        };

        f.write_str(w)
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
    OpenUrl(SmolStr),
    /// Sent when the user selects a different guild.
    GuildChanged(u64),
    /// Sent when the user selects a different channel.
    ChannelChanged(u64),
    /// Sent when the user scrolls the message history.
    MessageHistoryScrolled(f32),
    /// Sent when the user selects a menu entry from the profile menu (top right of the screen).
    SelectedAppMenuOption(ProfileMenuOption),
    /// Sent when the user selects a menu entry from the guild menu (top left of the screen).
    /// Only sent if we are currently looking at a guild.
    SelectedGuildMenuOption(GuildMenuOption),
    /// Sent when a member is selected, either from the message history or the member sidebar.
    SelectedMember(u64),
    /// Sent when the user selects `Yes` or `No` (also backdrop and escape) in the logout modal.
    LogoutChoice(bool),
    /// Sent when the permission check for viewing a channel is complete
    ChannelViewPerm(u64, u64, bool),
    /// Modal message passing
    ChannelCreationMessage(create_channel::Message),
    ImageViewMessage(image_viewer::Message),
    QuickSwitchMsg(quick_switcher::Message),
    ProfileEditMsg(profile_edit::Message),
    HelpModal(help::Message),
    UpdateChannelMessage(edit_channel::Message),
    /// Sent when the permission check for channel edits are complete.
    ShowUpdateChannelModal(u64, u64),
    /// Sent when the user triggers an ID copy (guild ID, message ID etc.)
    CopyIdToClipboard(u64),
    /// Sent when the user clicks the `+` button (guild discovery)
    OpenCreateJoinGuild,
    /// Sent when the user picks a new status
    ChangeUserStatus(UserStatus),
    ReplyToMessage(u64),
    GotoReply(MessageId),
    NextBeforeGuild(bool),
    NextBeforeChannel(bool),
}

#[derive(Debug, Default, Clone)]
pub struct MainScreen {
    // Event history area state
    event_history_state: scrollable::State,
    history_buts_sate: EventHistoryButsState,
    send_file_but_state: button::State,
    composer_state: text_input::State,
    scroll_to_bottom_but_state: button::State,

    // Room area state
    channel_menu_state: pick_list::State<GuildMenuOption>,
    menu_state: pick_list::State<ProfileMenuOption>,
    guilds_list_state: scrollable::State,
    guilds_buts_state: Vec<button::State>,
    channels_list_state: scrollable::State,
    channels_buts_state: Vec<(button::State, button::State, button::State)>,
    members_buts_state: Vec<button::State>,
    members_list_state: scrollable::State,
    status_list: pick_list::State<UserStatus>,

    // Modal states
    logout_modal: modal::State<LogoutModal>,
    create_channel_modal: modal::State<ChannelCreationModal>,
    pub image_viewer_modal: modal::State<ImageViewerModal>,
    quick_switcher_modal: modal::State<QuickSwitcherModal>,
    profile_edit_modal: modal::State<ProfileEditModal>,
    help_modal: modal::State<HelpModal>,
    update_channel_modal: modal::State<UpdateChannelModal>,

    /// A map of the last channel we have looked in each guild we are in
    guild_last_channels: IndexMap<u64, u64>,
    /// Current guild we are looking at
    pub current_guild_id: Option<u64>,
    /// Current channel we are looking at
    pub current_channel_id: Option<u64>,
    /// The message the user is currently typing.
    message: String,
    reply_to: Option<u64>,
    /// The last error in string form.
    error_text: String,
    /// Error "popup" close button state
    error_close_but_state: button::State,
    /// Current mode
    mode: Mode,
    prev_scroll_perc: f32,
}

impl MainScreen {
    pub fn view<'a>(
        &'a mut self,
        theme: Theme,
        client: &'a Client,
        thumbnail_cache: &'a ThumbnailCache,
    ) -> Element<'a, Message> {
        let guilds = &client.guilds;

        // Resize and (if extended) initialize new button states for new rooms
        // +1 for create / join guild [tag:create_join_guild_but_state]
        self.guilds_buts_state.resize_with(guilds.len() + 1, Default::default);

        // Create individual widgets

        let guild_list = build_guild_list(
            guilds,
            thumbnail_cache,
            self.current_guild_id,
            &mut self.guilds_list_state,
            &mut self.guilds_buts_state,
            Message::GuildChanged,
            theme,
        );

        let guild_list = Container::new(guild_list)
            .width(length!(= 64))
            .height(length!(+))
            .style(theme);

        let mut screen_widgets = Vec::with_capacity(3);
        screen_widgets.push(guild_list.into());

        let current_user_id = client.user_id.unwrap();
        let current_profile = client.members.get(&current_user_id);
        let current_username = current_profile.map_or(SmolStr::new_inline("Loading..."), |member| {
            truncate_string(&member.username, 16).into()
        });

        // TODO: show user avatar next to name
        let menu = PickList::new(
            &mut self.menu_state,
            vec![
                ProfileMenuOption::EditProfile,
                ProfileMenuOption::Help,
                ProfileMenuOption::SwitchAccount,
                ProfileMenuOption::Logout,
                ProfileMenuOption::Exit,
            ],
            Some(ProfileMenuOption::Custom(current_username)),
            Message::SelectedAppMenuOption,
        )
        .width(length!(+))
        .style(theme);

        let status_menu = PickList::new(
            &mut self.status_list,
            vec![
                UserStatus::OnlineUnspecified,
                UserStatus::DoNotDisturb,
                UserStatus::Idle,
                UserStatus::Offline,
                UserStatus::Streaming,
            ],
            Some(current_profile.map_or(UserStatus::OnlineUnspecified, |m| m.status)),
            Message::ChangeUserStatus,
        )
        .width(length!(+))
        .style(theme);

        if let Some((guild, guild_id)) = self
            .current_guild_id
            .as_ref()
            .and_then(|id| Some((guilds.get(id)?, *id)))
        {
            let mut sorted_members = guild
                .members
                .iter()
                .flat_map(|id| client.members.get(id).map(|m| (id, m)))
                .collect::<Vec<_>>();
            sorted_members.sort_unstable_by(|(_, member), (_, other_member)| {
                let name = member.username.as_str().cmp(other_member.username.as_str());
                let offline = matches!(member.status, UserStatus::Offline);
                let other_offline = matches!(other_member.status, UserStatus::Offline);

                match (offline, other_offline) {
                    (false, true) => Ordering::Less,
                    (true, false) => Ordering::Greater,
                    _ => name,
                }
            });

            self.members_buts_state
                .resize_with(guild.members.len(), Default::default);

            // Create the member list
            let member_list = self.members_buts_state.iter_mut().zip(sorted_members.iter()).fold(
                Scrollable::new(&mut self.members_list_state)
                    .spacing(SPACING)
                    .padding(PADDING),
                |mut list, (state, (user_id, member))| {
                    let mut username = label!(truncate_string(&member.username, 10));
                    // Set text color to a more dimmed one if the user is offline
                    if matches!(member.status, UserStatus::Offline) {
                        username = username.color(ALT_COLOR)
                    }
                    let status_color = theme.status_color(member.status);
                    let pfp: Element<Message> = member
                        .avatar_url
                        .as_ref()
                        .and_then(|hmc| thumbnail_cache.avatars.get(hmc))
                        .map_or_else(
                            || label!(member.username.chars().next().unwrap_or('u').to_ascii_uppercase()).into(),
                            |handle| {
                                let len = length!(= AVATAR_WIDTH - 4);
                                Image::new(handle.clone()).width(len).height(len).into()
                            },
                        );
                    let len = length!(= AVATAR_WIDTH);
                    let content: Vec<Element<Message>> = vec![
                        username.into(),
                        space!(w+).into(),
                        fill_container(pfp)
                            .width(len)
                            .height(len)
                            .style(theme.round().border_color(status_color))
                            .into(),
                    ];

                    list = list.push(
                        Button::new(state, Row::with_children(content).align_items(Align::Center))
                            .style(theme.secondary())
                            .on_press(Message::SelectedMember(**user_id))
                            .width(length!(+)),
                    );

                    list
                },
            );

            // [tag:guild_menu_entry]
            let channel_menu_entries = vec![
                GuildMenuOption::NewChannel,
                GuildMenuOption::EditGuild,
                GuildMenuOption::CopyGuildId,
                GuildMenuOption::LeaveGuild,
            ];

            let channel_menu = PickList::new(
                &mut self.channel_menu_state,
                channel_menu_entries,
                Some(GuildMenuOption::Custom(truncate_string(&guild.name, 16).into())),
                Message::SelectedGuildMenuOption,
            )
            .width(length!(+))
            .style(theme);

            self.channels_buts_state
                .resize_with(guild.channels.len(), Default::default);

            // Build the room list
            let channels_list = if guild.channels.is_empty() {
                fill_container(label!("No room found")).style(theme).into()
            } else {
                build_channel_list(
                    &guild.channels,
                    guild_id,
                    self.current_channel_id,
                    &mut self.channels_list_state,
                    &mut self.channels_buts_state,
                    Message::ChannelChanged,
                    theme,
                )
            };

            screen_widgets.push(
                Container::new(Column::with_children(vec![channel_menu.into(), channels_list]))
                    .width(length!(= 200))
                    .height(length!(+))
                    .style(theme)
                    .into(),
            );

            if let Some((channel, channel_id)) = self
                .current_channel_id
                .as_ref()
                .and_then(|id| Some((guild.channels.get(id)?, *id)))
            {
                let message_history_list = build_event_history(
                    client.content_store(),
                    thumbnail_cache,
                    channel,
                    &client.members,
                    current_user_id,
                    channel.looking_at_message,
                    &mut self.event_history_state,
                    &mut self.history_buts_sate,
                    self.mode,
                    theme,
                );

                let typing_names = sorted_members
                    .iter()
                    .flat_map(|(id, member)| {
                        // Remove own user id from the list (if its there)
                        if **id == current_user_id {
                            return None;
                        }

                        member
                            .typing_in_channel
                            .and_then(|(g, c, _)| (g == guild_id && c == channel_id).then(|| member.username.as_str()))
                    })
                    .collect::<Vec<_>>();
                let typing_members_count = typing_names.len();
                let typing_users_combined =
                    typing_names
                        .iter()
                        .enumerate()
                        .fold(String::new(), |mut comb, (index, name)| {
                            comb += name;

                            comb += match typing_members_count {
                                x if x > index + 1 => ", ",
                                1 => " is typing...",
                                _ => " are typing...",
                            };

                            comb
                        });

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
                    icon(Icon::Upload).size((PADDING / 4) * 3 + MESSAGE_SIZE),
                )
                .style(theme.secondary())
                .on_press(Message::SendFiles { guild_id, channel_id });

                let message_composer = if channel.user_perms.send_msg {
                    match self.mode {
                        Mode::Normal | Mode::EditingMessage(_) => TextInput::new(
                            &mut self.composer_state,
                            "Enter your message here...",
                            self.message.as_str(),
                            Message::ComposerMessageChanged,
                        )
                        .padding((PADDING / 4) * 3)
                        .size(MESSAGE_SIZE)
                        .style(theme.secondary())
                        .on_submit(Message::SendMessageComposer { guild_id, channel_id })
                        .width(length!(+))
                        .into(),
                    }
                } else {
                    fill_container(label!("You don't have permission to send a message here"))
                        .padding((PADDING / 4) * 3)
                        .height(length!(-))
                        .style(theme)
                        .into()
                };

                let mut bottom_area_widgets = vec![send_file_button.into(), message_composer];

                if channel.looking_at_message < channel.messages.len().saturating_sub(SHOWN_MSGS_LIMIT) {
                    bottom_area_widgets.push(
                        Button::new(
                            &mut self.scroll_to_bottom_but_state,
                            icon(Icon::ArrowDown).size((PADDING / 4) * 3 + MESSAGE_SIZE),
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

                screen_widgets.push(fill_container(message_area).style(theme).into());
            } else {
                let no_selected_channel_warning =
                    fill_container(label!("Select a channel").size(35).color(ALT_COLOR)).style(theme);

                screen_widgets.push(no_selected_channel_warning.into());
            }
            screen_widgets.push(
                Container::new(
                    Column::with_children(vec![
                        menu.into(),
                        member_list.into(),
                        space!(h+).into(),
                        status_menu.into(),
                    ])
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
                fill_container(label!("Select / join a guild").size(35).color(ALT_COLOR)).style(theme);

            screen_widgets.push(no_selected_guild_warning.into());
            screen_widgets.push(
                Container::new(
                    Column::with_children(vec![menu.into(), space!(h+).into(), status_menu.into()])
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
        let content: Element<Message> = Row::with_children(screen_widgets)
            .height(length!(+))
            .width(length!(+))
            .into();

        // Show error handling if needed
        let content = if self.error_text.is_empty() {
            content
        } else {
            Column::with_children(vec![
                fill_container(
                    Row::with_children(vec![
                        label!(truncate_string(&self.error_text, 128))
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
                content,
            ])
            .width(length!(+))
            .height(length!(+))
            .align_items(Align::Center)
            .into()
        };

        // Show HelpModal
        let content = Modal::new(&mut self.help_modal, content, move |state| {
            state.view(theme).map(Message::HelpModal)
        })
        .style(theme)
        .backdrop(Message::HelpModal(true))
        .on_esc(Message::HelpModal(true));

        // Show ProfileEditModal
        let content = Modal::new(&mut self.profile_edit_modal, content, move |state| {
            state.view(theme, client, thumbnail_cache).map(Message::ProfileEditMsg)
        })
        .style(theme)
        .backdrop(Message::ProfileEditMsg(profile_edit::Message::Back))
        .on_esc(Message::ProfileEditMsg(profile_edit::Message::Back));

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
            .backdrop(Message::ChannelCreationMessage(create_channel::Message::GoBack))
            .on_esc(Message::ChannelCreationMessage(create_channel::Message::GoBack));
            // Show UpdateChannelModal, if a guild is selected
            let content = Modal::new(&mut self.update_channel_modal, content, move |state| {
                state.view(theme).map(Message::UpdateChannelMessage)
            })
            .style(theme)
            .backdrop(Message::UpdateChannelMessage(edit_channel::Message::GoBack))
            .on_esc(Message::UpdateChannelMessage(edit_channel::Message::GoBack));
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
        clip: &mut iced::Clipboard,
    ) -> Command<TopLevelMessage> {
        fn scroll_to_bottom(client: &mut Client, guild_id: u64, channel_id: u64) {
            client
                .guilds
                .get_mut(&guild_id)
                .and_then(|guild| guild.channels.get_mut(&channel_id))
                .and_do(|c| c.looking_at_message = c.messages.len().saturating_sub(1));
        }

        match msg {
            Message::GotoReply(message_id) => {
                let guild_id = self.current_guild_id.unwrap();
                let channel_id = self.current_channel_id.unwrap();

                if let Some(channel) = client.get_channel(guild_id, channel_id) {
                    if let Some(pos) = channel.messages.iter().position(|(id, _)| message_id.eq(id)) {
                        channel.looking_at_message = pos.saturating_sub(SHOWN_MSGS_LIMIT);
                        self.event_history_state.snap_to(1.0);
                    }
                }
            }
            Message::ReplyToMessage(message_id) => {
                self.reply_to = Some(message_id);
            }
            Message::ChangeUserStatus(new_status) => {
                return client.mk_cmd(
                    |inner| async move {
                        profile::profile_update(&inner, ProfileUpdate::default().new_status(new_status)).await
                    },
                    |_| TopLevelMessage::Nothing,
                );
            }
            Message::OpenCreateJoinGuild => {
                return TopLevelScreen::push_screen_cmd(TopLevelScreen::GuildDiscovery(
                    super::GuildDiscovery::default().into(),
                ));
            }
            Message::CopyIdToClipboard(id) => clip.write(id.to_string()),
            Message::ChannelViewPerm(guild_id, channel_id, ok) => {
                client.get_channel(guild_id, channel_id).unwrap().user_perms.send_msg = ok;
            }
            Message::QuickSwitch => {
                self.quick_switcher_modal.show(!self.quick_switcher_modal.is_shown());
                let cmd = self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
                let cmd2 = self.update(
                    Message::QuickSwitchMsg(quick_switcher::Message::SearchTermChanged(
                        self.quick_switcher_modal.inner().search_value.clone(),
                    )),
                    client,
                    thumbnail_cache,
                    clip,
                );
                return Command::batch(vec![cmd, cmd2]);
            }
            Message::QuickSwitchMsg(msg) => match msg {
                quick_switcher::Message::SwitchToChannel { guild_id, channel_id } => {
                    let cmd = self.update(Message::GuildChanged(guild_id), client, thumbnail_cache, clip);
                    let cmd2 = self.update(Message::ChannelChanged(channel_id), client, thumbnail_cache, clip);
                    self.quick_switcher_modal.show(false);
                    self.quick_switcher_modal.inner_mut().search_value.clear();
                    return Command::batch(vec![cmd, cmd2]);
                }
                quick_switcher::Message::SwitchToGuild(guild_id) => {
                    let cmd = self.update(Message::GuildChanged(guild_id), client, thumbnail_cache, clip);
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
                            .flat_map(|(id, name)| Some((matcher.fuzzy(name, pattern, false)?.0, id, name)))
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
                                    .filter_map(move |(cid, c)| {
                                        c.is_category
                                            .not()
                                            .then(|| (*gid, *cid, SmolStr::from(format!("{} | {}", c.name, g.name))))
                                    })
                                    .flat_map(|(gid, cid, name)| {
                                        Some((matcher.fuzzy(&name, pattern, false)?.0, gid, cid, name))
                                    })
                            })
                            .collect::<Vec<_>>();
                        channels.sort_unstable_by_key(|(score, _, _, _)| *score);
                        channels
                            .into_iter()
                            .rev()
                            .map(|(_, gid, cid, name)| quick_switcher::SearchResult::Channel {
                                guild_id: gid,
                                id: cid,
                                name,
                            })
                            .collect()
                    };

                    let term_trimmed = new_term.trim();
                    self.quick_switcher_modal.inner_mut().results = if term_trimmed.is_empty() {
                        self.guild_last_channels
                            .iter()
                            .map(|(gid, cid)| quick_switcher::SearchResult::Channel {
                                guild_id: *gid,
                                id: *cid,
                                name: client
                                    .guilds
                                    .get(gid)
                                    .and_then(|g| {
                                        g.channels
                                            .get(cid)
                                            .map(|c| SmolStr::from(format!("{} | {}", c.name, g.name)))
                                    })
                                    .unwrap_or_else(|| SmolStr::new_inline("unknown")),
                            })
                            .collect()
                    } else if let Some(pattern) = new_term.strip_prefix('*').map(str::trim) {
                        guild(pattern)
                    } else if let Some(pattern) = new_term.strip_prefix('#').map(str::trim) {
                        channel(pattern)
                    } else {
                        [guild(term_trimmed), channel(term_trimmed)].concat()
                    };
                    self.quick_switcher_modal.inner_mut().search_value = new_term;
                }
            },
            Message::EditLastMessage => {
                let current_user_id = client.user_id.expect("literally how?");
                if let (Some(guild_id), Some(channel_id)) = (self.current_guild_id, self.current_channel_id) {
                    if let Some(mid) = client.get_channel(guild_id, channel_id).and_then(|c| {
                        c.messages
                            .iter()
                            .rev()
                            .find_map(|(id, m)| id.id().and_then(|id| (m.sender == current_user_id).some(id)))
                    }) {
                        return self.update(
                            Message::ChangeMode(Mode::EditingMessage(mid)),
                            client,
                            thumbnail_cache,
                            clip,
                        );
                    }
                }
            }
            Message::ChangeMode(mode) => {
                if let Mode::EditingMessage(mid) = mode {
                    if let (Some(gid), Some(cid)) = (self.current_guild_id, self.current_channel_id) {
                        if let Some(msg) = client
                            .get_channel(gid, cid)
                            .and_then(|c| c.messages.get(&MessageId::Ack(mid)))
                        {
                            self.composer_state.focus();
                            if let IcyContent::Text(text) = &msg.content {
                                client::tracing::debug!("editing message: {} / \"{}\"", mid, text);
                                self.message.clear();
                                self.message.push_str(text);
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
                open::that_in_background(url.as_str());
            }
            Message::OpenImageView { handle, path, name } => {
                self.image_viewer_modal.show(true);
                self.image_viewer_modal.inner_mut().image_handle = Some((handle, (path, name)));
                return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
            }
            Message::ProfileEditMsg(msg) => {
                let (cmd, go_back) = self.profile_edit_modal.inner_mut().update(msg, client);
                self.profile_edit_modal.show(!go_back);
                return cmd;
            }
            Message::ImageViewMessage(msg) => {
                let (cmd, go_back) = self.image_viewer_modal.inner_mut().update(msg);
                self.image_viewer_modal.show(!go_back);
                return cmd;
            }
            Message::ChannelCreationMessage(msg) => {
                let (cmd, go_back) = self.create_channel_modal.inner_mut().update(msg, client);
                self.create_channel_modal.show(!go_back);
                return cmd;
            }
            Message::UpdateChannelMessage(msg) => {
                let (cmd, go_back) = self.update_channel_modal.inner_mut().update(msg, client);
                self.update_channel_modal.show(!go_back);
                return cmd;
            }
            Message::ShowUpdateChannelModal(guild_id, channel_id) => {
                self.update_channel_modal.show(true);
                self.error_text.clear();
                let modal_state = self.update_channel_modal.inner_mut();
                let chan = client
                    .get_channel(guild_id, channel_id)
                    .expect("channel not found in client?"); // should never panic, if it does it means client data is corrupted
                chan.user_perms.manage_channel = true;
                modal_state.channel_name_field.clear();
                modal_state.channel_name_field.push_str(&chan.name);
                modal_state.guild_id = guild_id;
                modal_state.channel_id = channel_id;
                return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
            }
            Message::HelpModal(should_show) => {
                should_show.and_do(|| self.help_modal.show(false));
            }
            Message::LogoutChoice(confirm) => {
                self.logout_modal.show(false);
                return self.logout_modal.inner_mut().update(confirm, client);
            }
            Message::MessageHistoryScrolled(scroll_perc) => {
                // these are safe since we dont show message history scroller if not in a channel
                let guild_id = self.current_guild_id.unwrap();
                let channel_id = self.current_channel_id.unwrap();

                if scroll_perc < 0.01 && scroll_perc <= self.prev_scroll_perc {
                    if let Some((oldest_msg_id, disp, reached_top, loading_messages_history, looking_at_message)) =
                        client.get_channel(guild_id, channel_id).map(|channel| {
                            (
                                channel.messages.values().next().and_then(|m| m.id.id()),
                                channel.messages.len(),
                                channel.reached_top,
                                &mut channel.loading_messages_history,
                                &mut channel.looking_at_message,
                            )
                        })
                    {
                        (*looking_at_message == disp.saturating_sub(1))
                            .and_do(|| *looking_at_message = disp.saturating_sub(SHOWN_MSGS_LIMIT + 1))
                            .or_do(|| *looking_at_message = looking_at_message.saturating_sub(1));

                        if !reached_top && *looking_at_message < 2 && !*loading_messages_history {
                            *loading_messages_history = true;
                            return client.mk_cmd(
                                |inner| async move {
                                    channel::get_channel_messages(
                                        &inner,
                                        GetChannelMessages::new(guild_id, channel_id)
                                            .before_message(oldest_msg_id.unwrap_or_default()),
                                    )
                                    .await
                                    .map(|response| {
                                        TopLevelMessage::GetEventsBackwardsResponse {
                                            messages: response.messages,
                                            reached_top: response.reached_top,
                                            guild_id,
                                            channel_id,
                                        }
                                    })
                                },
                                identity,
                            );
                        }
                    }
                } else if scroll_perc > 0.99 && scroll_perc >= self.prev_scroll_perc {
                    client.get_channel(guild_id, channel_id).and_do(|c| {
                        let disp = c.messages.len();
                        if c.looking_at_message > disp.saturating_sub(SHOWN_MSGS_LIMIT) {
                            c.looking_at_message = disp.saturating_sub(1);
                            if c.has_unread {
                                c.has_unread = false;
                            }
                        } else {
                            c.looking_at_message = c.looking_at_message.saturating_add(1).min(disp);
                        }
                    });
                }
                self.prev_scroll_perc = scroll_perc;
            }
            Message::SelectedMember(user_id) => {
                let modal = self.profile_edit_modal.inner_mut();
                modal.user_id = user_id;
                modal.is_edit = false;
                self.profile_edit_modal.show(true);
                return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
            }
            Message::SelectedGuildMenuOption(option) => match option {
                GuildMenuOption::NewChannel => {
                    self.create_channel_modal.inner_mut().guild_id = self.current_guild_id.unwrap(); // [ref:guild_menu_entry]
                    self.create_channel_modal.show(true);
                    self.error_text.clear();
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
                }
                GuildMenuOption::EditGuild => {
                    let guild_id = self.current_guild_id.unwrap(); // [ref:guild_menu_entry]
                    return client
                        .guilds
                        .get(&guild_id)
                        .map(|g| {
                            g.user_perms.change_info.map_or_else(
                                || {
                                    TopLevelMessage::Error(Box::new(ClientError::Custom(
                                        "Not permitted to edit guild information".to_string(),
                                    )))
                                },
                                || {
                                    TopLevelMessage::PushScreen(Box::new(TopLevelScreen::GuildSettings(
                                        super::GuildSettings::new(guild_id).into(),
                                    )))
                                },
                            )
                        })
                        .map_or_else(Command::none, |msg| Command::perform(ready(msg), identity));
                }
                GuildMenuOption::LeaveGuild => {
                    let guild_id = self.current_guild_id.unwrap(); // [ref:guild_menu_entry]
                    return client.mk_cmd(
                        |inner| async move { guild::leave_guild(&inner, GuildId::new(guild_id)).await },
                        |_| TopLevelMessage::Nothing,
                    );
                }
                GuildMenuOption::CopyGuildId => {
                    clip.write(
                        self.current_guild_id
                            .expect("this menu is only shown if a guild is selected") // [ref:guild_menu_entry]
                            .to_string(),
                    );
                }
                _ => {}
            },
            Message::SelectedAppMenuOption(option) => match option {
                ProfileMenuOption::Logout => {
                    self.logout_modal.show(true);
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
                }
                ProfileMenuOption::SwitchAccount => {
                    return Command::perform(client.logout(false), |result| {
                        result.unwrap().map_to_msg_def(|_| TopLevelMessage::PopScreen)
                    });
                }
                ProfileMenuOption::EditProfile => {
                    let modal = self.profile_edit_modal.inner_mut();
                    modal.user_id = client
                        .user_id
                        .expect("we dont go to main screen if we dont have a user id");
                    modal.is_edit = true;
                    self.profile_edit_modal.show(true);
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
                }
                ProfileMenuOption::Help => {
                    self.help_modal.show(true);
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache, clip);
                }
                ProfileMenuOption::Exit => {
                    return Command::perform(async { TopLevelMessage::Exit }, identity);
                }
                _ => {}
            },
            Message::ComposerMessageChanged(new_msg) => {
                self.message = new_msg;
                let gid = self.current_guild_id.unwrap();
                let cid = self.current_channel_id.unwrap();
                let user_id = client.user_id.unwrap();

                if let Some(typing) = client.get_member(user_id).map(|m| &mut m.typing_in_channel) {
                    let should_send = typing.map_or(false, |(g, c, since)| {
                        g != gid || c != cid || since.elapsed().as_secs() >= 5
                    });
                    if should_send {
                        *typing = Some((gid, cid, Instant::now()));
                        return client.mk_cmd(
                            |inner| async move { chat::typing(&inner, Typing::new(gid, cid)).await },
                            map_to_nothing,
                        );
                    }
                }
            }
            Message::ScrollToBottom(sent_channel_id) => {
                if let (Some(guild_id), Some(channel_id)) = (self.current_guild_id, self.current_channel_id) {
                    if sent_channel_id == channel_id {
                        scroll_to_bottom(client, guild_id, channel_id);
                        self.event_history_state.snap_to(1.0);
                    }
                }
            }
            Message::OpenContent {
                attachment,
                is_thumbnail,
            } => {
                let maybe_thumb = thumbnail_cache.thumbnails.get(&attachment.id).cloned();
                let content_path = client.content_store().content_path(&attachment.id);
                return if content_path.exists() {
                    Command::perform(
                        async move {
                            Ok(if is_thumbnail && maybe_thumb.is_none() {
                                let data = tokio::fs::read(&content_path).await?;
                                let bgra = image::load_from_memory(&data).unwrap().into_bgra8();

                                TopLevelMessage::DownloadedThumbnail {
                                    data: attachment,
                                    avatar: None,
                                    thumbnail: Some(ImageHandle::from_pixels(
                                        bgra.width(),
                                        bgra.height(),
                                        bgra.into_vec(),
                                    )),
                                    open: true,
                                }
                            } else if is_thumbnail {
                                TopLevelMessage::main(Message::OpenImageView {
                                    handle: maybe_thumb.unwrap(),
                                    path: content_path,
                                    name: attachment.name,
                                })
                            } else {
                                open::that_in_background(content_path);
                                TopLevelMessage::Nothing
                            })
                        },
                        |result: ClientResult<_>| result.unwrap_or_else(Into::into),
                    )
                } else {
                    let inner = client.inner_arc();
                    Command::perform(
                        async move {
                            let downloaded_file = download_extract_file(&inner, attachment.id.clone()).await?;
                            tokio::fs::write(&content_path, downloaded_file.data()).await?;

                            Ok(if is_thumbnail && maybe_thumb.is_none() {
                                let bgra = image::load_from_memory(downloaded_file.data()).unwrap().into_bgra8();
                                TopLevelMessage::DownloadedThumbnail {
                                    data: attachment,
                                    avatar: None,
                                    thumbnail: Some(ImageHandle::from_pixels(
                                        bgra.width(),
                                        bgra.height(),
                                        bgra.into_vec(),
                                    )),
                                    open: true,
                                }
                            } else if is_thumbnail {
                                TopLevelMessage::main(Message::OpenImageView {
                                    handle: maybe_thumb.unwrap(),
                                    path: content_path,
                                    name: attachment.name,
                                })
                            } else {
                                open::that_in_background(content_path);
                                TopLevelMessage::Nothing
                            })
                        },
                        |result: ClientResult<_>| result.unwrap_or_else(Into::into),
                    )
                };
            }
            Message::SendMessageComposer { guild_id, channel_id } => {
                if !self.message.trim().is_empty() {
                    match self.mode {
                        Mode::EditingMessage(message_id) => {
                            let new_content: String = self.message.drain(..).collect::<String>().trim().into();
                            if let Some(msg) = client
                                .get_channel(guild_id, channel_id)
                                .and_then(|c| c.messages.get_mut(&MessageId::Ack(message_id)))
                            {
                                msg.being_edited = Some(new_content.clone());
                            }
                            self.mode = Mode::Normal;
                            return Command::perform(
                                client.edit_msg_cmd(guild_id, channel_id, message_id, new_content),
                                |(guild_id, channel_id, message_id, err)| TopLevelMessage::MessageEdited {
                                    guild_id,
                                    channel_id,
                                    message_id,
                                    err,
                                },
                            );
                        }
                        Mode::Normal => {
                            let message = IcyMessage {
                                content: IcyContent::Text(self.message.drain(..).collect::<String>().trim().into()),
                                sender: client.user_id.unwrap(),
                                reply_to: self.reply_to.take(),
                                ..Default::default()
                            };
                            if let Some(cmd) =
                                client.send_msg_cmd(guild_id, channel_id, Duration::from_secs(0), message)
                            {
                                scroll_to_bottom(client, guild_id, channel_id);
                                self.event_history_state.snap_to(1.0);
                                return Command::perform(cmd, map_send_msg);
                            }
                        }
                    }
                } else if let Mode::EditingMessage(mid) = self.mode {
                    self.mode = Mode::Normal;
                    return Command::perform(
                        client.delete_msg_cmd(guild_id, channel_id, mid),
                        ResultExt::map_to_nothing,
                    );
                }
            }
            Message::SendFiles { guild_id, channel_id } => {
                let inner = client.inner_arc();
                let content_store = client.content_store_arc();
                let sender = client.user_id.unwrap();

                return Command::perform(
                    async move {
                        let ids = super::select_upload_files(&inner, content_store, false).await?;
                        Ok(TopLevelMessage::SendMessage {
                            message: IcyMessage {
                                content: IcyContent::Files(ids),
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
                                error!("{}", err);
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
                    if guild.channels.is_empty() && !guild.init_fetching {
                        guild.init_fetching = true;
                        let inner = client.inner_arc();
                        return Command::perform(
                            async move {
                                let guildid = GuildId::new(guild_id);
                                let channels_list = get_guild_channels(&inner, guildid).await?.channels;
                                let mut events = Vec::with_capacity(channels_list.len());
                                for chan in &channels_list {
                                    let manage_query = "channel.manage.change-information".to_string();
                                    let manage_perm = query_has_permission(
                                        &inner,
                                        QueryPermissions::new(guild_id, manage_query.clone())
                                            .channel_id(chan.channel_id),
                                    )
                                    .await?
                                    .ok;
                                    let send_msg_query = "message.send".to_string();
                                    let send_msg_perm = query_has_permission(
                                        &inner,
                                        QueryPermissions::new(guild_id, send_msg_query.clone())
                                            .channel_id(chan.channel_id),
                                    )
                                    .await?
                                    .ok;

                                    events.push(Event::PermissionUpdated(PermissionUpdated {
                                        guild_id,
                                        channel_id: chan.channel_id,
                                        query: manage_query,
                                        ok: manage_perm,
                                    }));
                                    events.push(Event::PermissionUpdated(PermissionUpdated {
                                        guild_id,
                                        channel_id: chan.channel_id,
                                        query: send_msg_query,
                                        ok: send_msg_perm,
                                    }));
                                }
                                let channel_events = channels_list
                                    .into_iter()
                                    .map(|c| {
                                        Event::CreatedChannel(ChannelCreated {
                                            guild_id,
                                            channel_id: c.channel_id,
                                            is_category: c.is_category,
                                            name: c.channel_name,
                                            metadata: c.metadata,
                                            ..Default::default()
                                        })
                                    })
                                    .rev();
                                events.extend(channel_events);
                                events.reverse();

                                let members = get_guild_members(&inner, guildid).await?.members;
                                events.reserve(members.len());
                                let member_events = members
                                    .into_iter()
                                    .map(|member_id| Event::JoinedMember(MemberJoined { member_id, guild_id }));
                                events.extend(member_events);

                                ClientResult::<_>::Ok(events)
                            },
                            move |res| TopLevelMessage::InitialLoad {
                                guild_id,
                                channel_id: None,
                                events: res.map_err(Box::new),
                            },
                        );
                    } else {
                        let switch_to = self
                            .guild_last_channels
                            .get(&guild_id)
                            .copied()
                            .or_else(|| guild.channels.first().map(|(id, _)| *id));

                        if let Some(id) = switch_to {
                            return self.update(Message::ChannelChanged(id), client, thumbnail_cache, clip);
                        }
                    }
                }
            }
            Message::ChannelChanged(channel_id) => {
                let guild_id = self.current_guild_id.unwrap();

                if let Some(channel_id) = self.current_channel_id {
                    client
                        .get_channel(guild_id, channel_id)
                        .and_do(|c| c.looking_at_channel = false);
                }

                self.mode = Mode::Normal;
                self.message.clear();
                self.current_channel_id = Some(channel_id);
                self.guild_last_channels.insert(guild_id, channel_id);

                if let Some(c) = client.get_channel(guild_id, channel_id) {
                    let disp = c.messages.len();
                    let reached_top = c.reached_top;
                    c.looking_at_channel = true;

                    (c.looking_at_message >= disp.saturating_sub(SHOWN_MSGS_LIMIT)).and_do(|| {
                        c.has_unread = false;
                        c.looking_at_message = disp.saturating_sub(1);
                        self.event_history_state.snap_to(1.0);
                    });

                    let mut cmds = Vec::with_capacity(2);
                    // Try to messages if we dont have any and we arent at the top
                    if !reached_top && disp == 0 && !c.init_fetching {
                        let convert_to_event = |m: GetChannelMessagesResponse| {
                            m.messages
                                .into_iter()
                                .map(|msg| {
                                    Event::SentMessage(Box::new(MessageSent {
                                        message: Some(msg),
                                        ..Default::default()
                                    }))
                                })
                                .rev()
                                .collect()
                        };
                        c.init_fetching = true;
                        let inner = client.inner_arc();
                        cmds.push(Command::perform(
                            async move {
                                get_channel_messages(&inner, GetChannelMessages::new(guild_id, channel_id))
                                    .await
                                    .map(convert_to_event)
                                    .map_err(ClientError::from)
                            },
                            move |res| TopLevelMessage::InitialLoad {
                                guild_id,
                                channel_id: Some(channel_id),
                                events: res.map_err(Box::new),
                            },
                        ));
                    }
                    return Command::batch(cmds);
                }
            }
            Message::NextBeforeGuild(before) => {
                let change_guild_to = if let Some(guild_pos) = self
                    .current_guild_id
                    .and_then(|guild_id| client.guilds.get_index_of(&guild_id))
                {
                    if before {
                        if guild_pos == 0 {
                            client.guilds.last().map(|(id, _)| *id)
                        } else {
                            client.guilds.get_index(guild_pos - 1).map(|(id, _)| *id)
                        }
                    } else if guild_pos == client.guilds.len().saturating_sub(1) {
                        client.guilds.first().map(|(id, _)| *id)
                    } else {
                        client.guilds.get_index(guild_pos + 1).map(|(id, _)| *id)
                    }
                } else if before {
                    client.guilds.last().map(|(id, _)| *id)
                } else {
                    client.guilds.first().map(|(id, _)| *id)
                };

                if let Some(guild_id) = change_guild_to {
                    return Command::perform(ready(TopLevelMessage::main(Message::GuildChanged(guild_id))), identity);
                }
            }
            Message::NextBeforeChannel(before) => {
                let change_channel_to =
                    if let Some(guild) = self.current_guild_id.and_then(|guild_id| client.guilds.get(&guild_id)) {
                        if let Some(chan_pos) = self
                            .current_channel_id
                            .and_then(|channel_id| guild.channels.get_index_of(&channel_id))
                        {
                            if before {
                                if chan_pos == 0 {
                                    guild.channels.last().map(|(id, _)| *id)
                                } else {
                                    guild.channels.get_index(chan_pos - 1).map(|(id, _)| *id)
                                }
                            } else if chan_pos == guild.channels.len().saturating_sub(1) {
                                guild.channels.first().map(|(id, _)| *id)
                            } else {
                                guild.channels.get_index(chan_pos + 1).map(|(id, _)| *id)
                            }
                        } else if before {
                            guild.channels.last().map(|(id, _)| *id)
                        } else {
                            guild.channels.first().map(|(id, _)| *id)
                        }
                    } else {
                        None
                    };

                if let Some(channel_id) = change_channel_to {
                    return Command::perform(
                        ready(TopLevelMessage::main(Message::ChannelChanged(channel_id))),
                        identity,
                    );
                }
            }
        }

        Command::none()
    }

    pub fn subscription(&self) -> Subscription<TopLevelMessage> {
        use iced_native::{keyboard, Event};

        fn filter_events(ev: Event, _status: iced_native::event::Status) -> Option<TopLevelMessage> {
            type Ke = keyboard::Event;
            type Kc = keyboard::KeyCode;

            match ev {
                Event::Keyboard(Ke::KeyReleased {
                    key_code: Kc::Escape, ..
                }) => Some(TopLevelMessage::main(Message::ChangeMode(Mode::Normal))),
                Event::Keyboard(Ke::KeyReleased {
                    key_code: Kc::K,
                    modifiers,
                }) => modifiers.control().then(|| TopLevelMessage::main(Message::QuickSwitch)),
                Event::Keyboard(Ke::KeyReleased {
                    key_code: Kc::Up,
                    modifiers,
                }) => {
                    let msg = if modifiers.control() {
                        if modifiers.alt() {
                            TopLevelMessage::main(Message::NextBeforeGuild(true))
                        } else {
                            return None;
                        }
                    } else if modifiers.alt() {
                        TopLevelMessage::main(Message::NextBeforeChannel(true))
                    } else {
                        TopLevelMessage::main(Message::EditLastMessage)
                    };
                    Some(msg)
                }
                Event::Keyboard(Ke::KeyReleased {
                    key_code: Kc::Down,
                    modifiers,
                }) => {
                    let msg = if modifiers.control() {
                        if modifiers.alt() {
                            TopLevelMessage::main(Message::NextBeforeGuild(false))
                        } else {
                            return None;
                        }
                    } else if modifiers.alt() {
                        TopLevelMessage::main(Message::NextBeforeChannel(false))
                    } else {
                        return None;
                    };
                    Some(msg)
                }
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
