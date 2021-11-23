use std::{
    cmp::Ordering,
    convert::identity,
    fmt::{self, Display, Formatter},
    ops::Not,
    path::PathBuf,
    time::{Duration, Instant},
};

use super::{Message as TopLevelMessage, Screen as TopLevelScreen};
use channel::GetChannelMessages;
use chat::Typing;
use client::{
    bool_ext::BoolExt,
    content,
    error::ClientResult,
    harmony_rust_sdk::{
        api::{
            chat::{
                all_permissions::{MESSAGES_SEND, ROLES_GET, ROLES_USER_MANAGE},
                get_channel_messages_request::Direction,
                stream_event::{ChannelCreated, Event as ChatEvent, MemberJoined, RoleCreated, UserRolesUpdated},
                Event, GetChannelMessagesResponse, GetGuildChannelsRequest, GetGuildMembersRequest,
                GetGuildRolesRequest, GetUserRolesRequest,
            },
            profile::UserStatus,
            rest::FileId,
        },
        client::{
            api::{
                chat::{self, channel, GuildId},
                profile::UpdateProfile,
                rest::download_extract_file,
            },
            error::ClientError as InnerClientError,
            exports::reqwest::StatusCode,
        },
    },
    message::MessageId,
    render_text,
    smol_str::SmolStr,
    tracing::error,
    IndexMap, OptionExt,
};
use iced::{futures::future::ready, rule::FillMode, Tooltip};
use iced_aw::{modal, Modal};

use chan_guild_list::build_guild_list;
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
        event_history::{EventHistoryButsState, MessageMenuOption, SHOWN_MSGS_LIMIT},
        *,
    },
    label, label_button, length,
    screen::{map_send_msg, map_to_nothing, select_files, truncate_string, ClientExt, ResultExt},
    space,
    style::{tuple_to_iced_color, Theme, AVATAR_WIDTH, DEF_SIZE, MESSAGE_SIZE, PADDING, SPACING},
};

use self::quick_switcher::QuickSwitcherModal;

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
    ManageEmotes,
    Help,
    Logout,
    SwitchAccount,
    CopyToken,
    Exit,
}

impl Display for ProfileMenuOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let w = match self {
            ProfileMenuOption::EditProfile => "Edit Profile",
            ProfileMenuOption::ManageEmotes => "Manage Emotes",
            ProfileMenuOption::Help => "Help",
            ProfileMenuOption::Logout => "Logout",
            ProfileMenuOption::SwitchAccount => "Switch Account",
            ProfileMenuOption::CopyToken => "Copy Token",
            ProfileMenuOption::Exit => "Exit",
        };

        f.write_str(w)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildMenuOption {
    EditGuild,
    LeaveGuild,
}

impl Display for GuildMenuOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let w = match self {
            GuildMenuOption::EditGuild => "Edit Guild",
            GuildMenuOption::LeaveGuild => "Leave Guild",
        };

        f.write_str(w)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    FocusComposer(char),
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
    SelectFilesToSend,
    UploadFiles {
        guild_id: u64,
        channel_id: u64,
        files: Vec<PathBuf>,
    },
    UploadResult {
        guild_id: u64,
        channel_id: u64,
        result: Box<ClientResult<Vec<Attachment>>>,
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
    /// Modal message passing
    ImageViewMessage(image_viewer::Message),
    QuickSwitchMsg(quick_switcher::Message),
    ProfileEditMsg(profile_edit::Message),
    HelpModal(help::Message),
    /// Sent when the user clicks the `+` button (guild discovery)
    OpenCreateJoinGuild,
    /// Sent when the user picks a new status
    ChangeUserStatus(UserStatus),
    GotoReply(MessageId),
    ClearReply,
    NextBeforeGuild(bool),
    NextBeforeChannel(bool),
    CopyToClipboard(String),
    MessageMenuSelected(MessageMenuOption),
    AutoCompleteBefore,
    AutoCompleteNext,
    AutoComplete,
}

#[derive(Debug, Default, Clone)]
pub struct MainScreen {
    // Event history area state
    event_history_state: scrollable::State,
    history_buts_sate: EventHistoryButsState,
    send_file_but_state: button::State,
    composer_state: text_input::State,
    goto_reply_state: button::State,
    clear_reply_state: button::State,
    scroll_to_bottom_but_state: button::State,
    before_after_completion_items: (Option<SmolStr>, Option<SmolStr>),
    completion_current: Option<SmolStr>,

    // Room area state
    channel_menu_state: pick_list::State<GuildMenuOption>,
    menu_state: pick_list::State<ProfileMenuOption>,
    guilds_list_state: scrollable::State,
    guilds_buts_state: Vec<button::State>,
    channels_list_state: scrollable::State,
    channels_buts_state: Vec<button::State>,
    members_buts_state: Vec<button::State>,
    members_list_state: scrollable::State,
    status_list: pick_list::State<UserStatus>,

    // Modal states
    logout_modal: modal::State<LogoutModal>,
    pub image_viewer_modal: modal::State<ImageViewerModal>,
    quick_switcher_modal: modal::State<QuickSwitcherModal>,
    profile_edit_modal: modal::State<ProfileEditModal>,
    help_modal: modal::State<HelpModal>,

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
        theme: &'a Theme,
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
            self.guilds_buts_state.as_mut_slice(),
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
                ProfileMenuOption::ManageEmotes,
                ProfileMenuOption::Help,
                ProfileMenuOption::SwitchAccount,
                ProfileMenuOption::Logout,
                ProfileMenuOption::Exit,
            ],
            None,
            Message::SelectedAppMenuOption,
        )
        .placeholder(current_username)
        .width(length!(+))
        .padding(PADDING / 2)
        .style(theme.placeholder_color(theme.user_theme.text));

        let status_menu = PickList::new(
            &mut self.status_list,
            vec![
                UserStatus::Online,
                UserStatus::DoNotDisturb,
                UserStatus::Idle,
                UserStatus::OfflineUnspecified,
                UserStatus::Streaming,
            ],
            Some(current_profile.map_or(UserStatus::Online, |m| m.status)),
            Message::ChangeUserStatus,
        )
        .width(length!(+))
        .padding(PADDING / 2)
        .style(theme);

        if let Some((guild, guild_id)) = self
            .current_guild_id
            .as_ref()
            .and_then(|id| Some((guilds.get(id)?, *id)))
        {
            let mut sorted_members = guild
                .members
                .keys()
                .flat_map(|id| client.members.get(id).map(|m| (id, m)))
                .collect::<Vec<_>>();
            sorted_members.sort_unstable_by(|(_, member), (_, other_member)| {
                let name = member.username.as_str().cmp(other_member.username.as_str());
                let offline = matches!(member.status, UserStatus::OfflineUnspecified);
                let other_offline = matches!(other_member.status, UserStatus::OfflineUnspecified);

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
                (
                    Scrollable::new(&mut self.members_list_state)
                        .spacing(SPACING)
                        .padding(PADDING),
                    None,
                ),
                |(mut list, last_role_id), (state, (user_id, member))| {
                    const TRUNCATE_LEN: usize = 10;

                    let highest_role = guild.highest_role_for_member(**user_id).map(|(id, role)| (id, role));
                    let sender_name_color =
                        highest_role.map_or(Color::WHITE, |(_, role)| tuple_to_iced_color(role.color));
                    let mut username = label!(truncate_string(&member.username, TRUNCATE_LEN)).color(sender_name_color);
                    // Set text color to a more dimmed one if the user is offline
                    if matches!(member.status, UserStatus::OfflineUnspecified) {
                        username = username.color(Color {
                            a: 0.4,
                            ..sender_name_color
                        });
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
                            .style(theme.border_width(2.5).border_color(status_color))
                            .into(),
                    ];

                    if highest_role.is_some() && highest_role.map(|(id, _)| *id) != last_role_id {
                        list = list.push(
                            Row::with_children(vec![
                                label!(highest_role.unwrap().1.name.as_str()).size(DEF_SIZE - 1).into(),
                                Rule::horizontal(SPACING * 2).style(theme.secondary()).into(),
                            ])
                            .align_items(Align::Center),
                        );
                    }

                    let but = Button::new(
                        state,
                        Row::with_children(content)
                            .align_items(Align::Center)
                            .padding(PADDING / 3),
                    )
                    .style(theme.secondary().border_width(2.0))
                    .on_press(Message::SelectedMember(**user_id))
                    .width(length!(+));

                    let elem: Element<Message> = if member.username.chars().count() > TRUNCATE_LEN {
                        Tooltip::new(but, member.username.as_str(), iced::tooltip::Position::Left)
                            .style(theme)
                            .into()
                    } else {
                        but.into()
                    };

                    list = list.push(elem);

                    (list, highest_role.map(|(id, _)| *id))
                },
            );

            // [tag:guild_menu_entry]
            let channel_menu_entries = vec![GuildMenuOption::EditGuild, GuildMenuOption::LeaveGuild];

            let channel_menu = PickList::new(
                &mut self.channel_menu_state,
                channel_menu_entries,
                None,
                Message::SelectedGuildMenuOption,
            )
            .placeholder(truncate_string(&guild.name, 16))
            .width(length!(+))
            .padding(PADDING / 2)
            .style(theme.placeholder_color(theme.user_theme.text));

            self.channels_buts_state
                .resize_with(guild.channels.len(), Default::default);

            // Build the room list
            let channels_list = if guild.channels.is_empty() {
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

            screen_widgets.push(
                Container::new(Column::with_children(vec![channel_menu.into(), channels_list]))
                    .width(length!(= 220))
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
                    client,
                    guild,
                    channel,
                    &client.members,
                    current_user_id,
                    channel.looking_at_message,
                    &mut self.event_history_state,
                    &mut self.history_buts_sate,
                    self.mode,
                    theme,
                );

                let icon_size = (PADDING / 4) * 3 + MESSAGE_SIZE;

                let mk_seperator = || {
                    Rule::horizontal(0)
                        .style(theme.border_width(2.0).border_radius(0.0).padded(FillMode::Full))
                        .into()
                };
                let mut message_area_widgets = Vec::with_capacity(8);
                message_area_widgets.push(message_history_list);
                message_area_widgets.push(mk_seperator());
                if !channel.uploading_files.is_empty() {
                    let widgets = std::iter::once("Uploading files: ")
                        .chain(channel.uploading_files.iter().map(String::as_str))
                        .map(|label| label!(label).size(MESSAGE_SIZE).into())
                        .collect();
                    message_area_widgets.push(
                        Container::new(Row::with_children(widgets).align_items(Align::Center).spacing(SPACING))
                            .center_y()
                            .center_x()
                            .padding(PADDING / 2)
                            .into(),
                    );
                    message_area_widgets.push(mk_seperator());
                }
                if let Some(reply_message) = self.reply_to.map(|id| {
                    let id = MessageId::Ack(id);
                    channel.messages.get(&id).map(|m| (id, m))
                }) {
                    let widget = make_reply_message(
                        reply_message,
                        client,
                        theme,
                        Message::GotoReply,
                        &mut self.goto_reply_state,
                    );
                    let clear_reply_but = Button::new(&mut self.clear_reply_state, icon(Icon::X))
                        .style(theme)
                        .padding(PADDING / 4)
                        .on_press(Message::ClearReply);
                    message_area_widgets.push(
                        Container::new(
                            Row::with_children(vec![
                                label!("Replying to").size(MESSAGE_SIZE).into(),
                                widget.into(),
                                space!(w+).into(),
                                clear_reply_but.into(),
                            ])
                            .spacing(SPACING)
                            .align_items(Align::Center),
                        )
                        .center_x()
                        .center_y()
                        .padding(PADDING / 2)
                        .into(),
                    );
                    message_area_widgets.push(mk_seperator());
                }

                let mut autocompleting = false;
                if let Some((word, _, _)) = self.composer_state.get_word_at_cursor(&self.message) {
                    const LEN: u16 = MESSAGE_SIZE + 10;
                    if word.starts_with(':') && !word.ends_with(':') {
                        let emote_name = word.trim_end_matches(':').trim_start_matches(':');
                        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
                        let mut matched_emotes = client
                            .get_all_emotes()
                            .flat_map(|(id, name)| Some((matcher.fuzzy(name, emote_name, false)?.0, id, name)))
                            .collect::<Vec<_>>();

                        if !matched_emotes.is_empty() {
                            matched_emotes.sort_unstable_by_key(|(score, _, _)| *score);
                            matched_emotes.truncate(8);

                            if let Some(pos) = self
                                .completion_current
                                .as_deref()
                                .and_then(|s| matched_emotes.iter().position(|(_, _, os)| s == *os))
                            {
                                self.before_after_completion_items = (
                                    (pos == 0)
                                        .not()
                                        .then(|| matched_emotes.get(pos - 1))
                                        .flatten()
                                        .or_else(|| matched_emotes.last())
                                        .map(|(_, _, name)| SmolStr::new(name)),
                                    matched_emotes
                                        .get(pos + 1)
                                        .or_else(|| matched_emotes.first())
                                        .map(|(_, _, name)| SmolStr::new(name)),
                                );
                            } else {
                                self.before_after_completion_items = (
                                    matched_emotes.last().map(|(_, _, name)| SmolStr::new(name)),
                                    matched_emotes.first().map(|(_, _, name)| SmolStr::new(name)),
                                );
                            }

                            let current = self.completion_current.clone();
                            message_area_widgets.push(
                                Row::with_children(
                                    matched_emotes
                                        .into_iter()
                                        .map(|(_, image_id, emote_name)| {
                                            let image =
                                                match thumbnail_cache.emotes.get(&FileId::Id(image_id.to_string())) {
                                                    Some(h) => Image::new(h.clone())
                                                        .width(length!(= LEN))
                                                        .height(length!(= LEN))
                                                        .into(),
                                                    None => space!(= LEN, LEN).into(),
                                                };
                                            let bg_color = (current.as_deref() == Some(emote_name))
                                                .then(|| theme.user_theme.accent)
                                                .unwrap_or(theme.user_theme.primary_bg);
                                            Container::new(
                                                Row::with_children(vec![
                                                    image,
                                                    label!(emote_name).size(MESSAGE_SIZE).into(),
                                                ])
                                                .align_items(Align::Center)
                                                .spacing(SPACING / 2),
                                            )
                                            .style(theme.background_color(bg_color).round())
                                            .padding(PADDING / 4)
                                            .into()
                                        })
                                        .collect(),
                                )
                                .align_items(Align::Center)
                                .spacing(SPACING)
                                .padding(PADDING / 4)
                                .into(),
                            );
                            message_area_widgets.push(mk_seperator());
                            autocompleting = true;
                        }
                    } else if word.starts_with('@') {
                        let member_name = word.trim_start_matches('@');
                        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
                        let mut matched_members = guild
                            .members
                            .keys()
                            .flat_map(|id| {
                                client.members.get(id).and_then(|member| {
                                    Some((
                                        matcher.fuzzy(member.username.as_str(), member_name, false)?.0,
                                        member.avatar_url.as_ref(),
                                        member.username.as_str(),
                                    ))
                                })
                            })
                            .collect::<Vec<_>>();

                        if !matched_members.is_empty() {
                            matched_members.sort_unstable_by_key(|(score, _, _)| *score);
                            matched_members.truncate(8);

                            if let Some(pos) = self
                                .completion_current
                                .as_deref()
                                .and_then(|s| matched_members.iter().position(|(_, _, os)| s == *os))
                            {
                                self.before_after_completion_items = (
                                    (pos == 0)
                                        .not()
                                        .then(|| matched_members.get(pos - 1))
                                        .flatten()
                                        .or_else(|| matched_members.last())
                                        .map(|(_, _, name)| SmolStr::new(name)),
                                    matched_members
                                        .get(pos + 1)
                                        .or_else(|| matched_members.first())
                                        .map(|(_, _, name)| SmolStr::new(name)),
                                );
                            } else {
                                self.before_after_completion_items = (
                                    matched_members.last().map(|(_, _, name)| SmolStr::new(name)),
                                    matched_members.first().map(|(_, _, name)| SmolStr::new(name)),
                                );
                            }

                            let current = self.completion_current.clone();
                            message_area_widgets.push(
                                Row::with_children(
                                    matched_members
                                        .into_iter()
                                        .map(|(_, image_id, member_name)| {
                                            let mut widgets = Vec::with_capacity(2);
                                            if let Some(h) = image_id.and_then(|id| thumbnail_cache.avatars.get(id)) {
                                                widgets.push(
                                                    Image::new(h.clone())
                                                        .width(length!(= LEN))
                                                        .height(length!(= LEN))
                                                        .into(),
                                                );
                                            }
                                            widgets.push(label!(member_name).size(MESSAGE_SIZE).into());
                                            let bg_color = (current.as_deref() == Some(member_name))
                                                .then(|| theme.user_theme.accent)
                                                .unwrap_or(theme.user_theme.primary_bg);
                                            Container::new(
                                                Row::with_children(widgets)
                                                    .align_items(Align::Center)
                                                    .spacing(SPACING / 2),
                                            )
                                            .style(theme.background_color(bg_color).round())
                                            .padding(PADDING / 4)
                                            .into()
                                        })
                                        .collect(),
                                )
                                .align_items(Align::Center)
                                .spacing(SPACING)
                                .padding(PADDING / 4)
                                .into(),
                            );
                            message_area_widgets.push(mk_seperator());
                            autocompleting = true;
                        }
                    }
                }
                if !autocompleting {
                    self.completion_current = None;
                    self.before_after_completion_items = (None, None);
                }

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
                if !typing_names.is_empty() {
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
                    message_area_widgets.push(typing_users.into());
                }

                let mut send_file_button =
                    Button::new(&mut self.send_file_but_state, icon(Icon::Upload).size(icon_size))
                        .style(theme.secondary().border_width(2.0))
                        .padding(PADDING / 4);
                if channel.uploading_files.is_empty() {
                    send_file_button = send_file_button.on_press(Message::SelectFilesToSend);
                }
                let send_file_button =
                    Tooltip::new(send_file_button, "Click to upload a file", iced::tooltip::Position::Top).style(theme);

                let message_composer = if channel.has_perm(MESSAGES_SEND) {
                    match self.mode {
                        Mode::Normal | Mode::EditingMessage(_) => TextInput::new(
                            &mut self.composer_state,
                            "Enter your message here...",
                            self.message.as_str(),
                            Message::ComposerMessageChanged,
                        )
                        .padding((PADDING / 4) * 3)
                        .size(MESSAGE_SIZE)
                        .style(theme.secondary().border_width(2.0))
                        .on_submit(Message::SendMessageComposer { guild_id, channel_id })
                        .width(length!(+))
                        .into(),
                    }
                } else {
                    fill_container(label!("You don't have permission to send a message here"))
                        .padding((PADDING / 4) * 3)
                        .height(length!(-))
                        .style(theme.border_width(0.0))
                        .into()
                };

                let mut bottom_area_widgets = vec![send_file_button.into(), message_composer];

                if channel.looking_at_message < channel.messages.len().saturating_sub(SHOWN_MSGS_LIMIT) {
                    bottom_area_widgets.push(
                        Tooltip::new(
                            Button::new(
                                &mut self.scroll_to_bottom_but_state,
                                icon(Icon::ArrowDown).size(icon_size),
                            )
                            .padding(PADDING / 4)
                            .style(theme.secondary())
                            .on_press(Message::ScrollToBottom(channel_id)),
                            "Scroll to bottom",
                            iced::tooltip::Position::Top,
                        )
                        .style(theme.secondary().border_width(2.0))
                        .into(),
                    );
                }

                message_area_widgets.push(
                    Container::new(
                        Row::with_children(bottom_area_widgets)
                            .spacing(SPACING * 2)
                            .width(length!(+)),
                    )
                    .width(length!(+))
                    .padding(PADDING / 2)
                    .into(),
                );

                let message_area = Column::with_children(message_area_widgets);

                screen_widgets.push(fill_container(message_area).style(theme).into());
            } else {
                let no_selected_channel_warning =
                    fill_container(label!("Select a channel").size(35).color(theme.user_theme.dimmed_text))
                        .style(theme);

                screen_widgets.push(no_selected_channel_warning.into());
            }
            screen_widgets.push(
                Container::new(
                    Column::with_children(vec![
                        menu.into(),
                        member_list.0.into(),
                        space!(h+).into(),
                        status_menu.into(),
                    ])
                    .width(length!(+))
                    .height(length!(+)),
                )
                .width(length!(= 220))
                .height(length!(+))
                .style(theme)
                .into(),
            );
        } else {
            let no_selected_guild_warning = fill_container(
                label!("Select / join a guild")
                    .size(35)
                    .color(theme.user_theme.dimmed_text),
            )
            .style(theme);

            screen_widgets.push(no_selected_guild_warning.into());
            screen_widgets.push(
                Container::new(
                    Column::with_children(vec![menu.into(), space!(h+).into(), status_menu.into()])
                        .width(length!(+))
                        .height(length!(+)),
                )
                .width(length!(= 220))
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
                            .color(theme.user_theme.error)
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
                        channel.looking_at_message = pos;
                        self.event_history_state.snap_to(0.0);
                    }
                }
            }
            Message::ChangeUserStatus(new_status) => {
                return client.mk_cmd(
                    |inner| async move { inner.call(UpdateProfile::default().with_new_status(new_status)).await },
                    |_| TopLevelMessage::Nothing,
                );
            }
            Message::OpenCreateJoinGuild => {
                return TopLevelScreen::push_screen_cmd(TopLevelScreen::GuildDiscovery(
                    super::GuildDiscovery::default().into(),
                ));
            }
            Message::QuickSwitch => {
                self.quick_switcher_modal.show(!self.quick_switcher_modal.is_shown());
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
                quick_switcher::Message::SwitchToChannel { guild_id, channel_id } => {
                    let cmd = self.update(Message::GuildChanged(guild_id), client, thumbnail_cache);
                    let cmd2 = self.update(Message::ChannelChanged(channel_id), client, thumbnail_cache);
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
                        return self.update(Message::ChangeMode(Mode::EditingMessage(mid)), client, thumbnail_cache);
                    }
                }
            }
            Message::ChangeMode(mode) => {
                if let Mode::EditingMessage(mid) = mode {
                    if let (Some(gid), Some(cid)) = (self.current_guild_id, self.current_channel_id) {
                        if let Some(msg) = client
                            .guilds
                            .get(&gid)
                            .and_then(|g| g.channels.get(&cid))
                            .and_then(|c| c.messages.get(&MessageId::Ack(mid)))
                        {
                            self.composer_state.focus();
                            if let IcyContent::Text(text) = &msg.content {
                                client::tracing::debug!("editing message: {} / \"{}\"", mid, text);
                                self.message.clear();
                                self.message
                                    .push_str(&render_text(text, &client.members, &client.emote_packs));
                            }
                        }
                    } else {
                        self.composer_state.unfocus();
                        self.message.clear();
                    }
                }
                if self.current_guild_id.is_some() && self.current_channel_id.is_some() {
                    if let (Mode::EditingMessage(_), Mode::Normal) = (self.mode, mode) {
                        self.composer_state.unfocus();
                        self.message.clear();
                    }
                    if let (Mode::Normal, Mode::Normal) = (self.mode, mode) {
                        self.composer_state.unfocus();
                        self.error_text.clear();
                    }
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
                return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
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
                                channel.last_known_message_id,
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
                                    inner
                                        .call(
                                            GetChannelMessages::new(guild_id, channel_id)
                                                .with_message_id(oldest_msg_id),
                                        )
                                        .await
                                        .map(|response| TopLevelMessage::GetChannelMessagesResponse {
                                            messages: response
                                                .messages
                                                .into_iter()
                                                .flat_map(|m| {
                                                    let msg = m.message?;
                                                    Some((m.message_id, msg))
                                                })
                                                .collect(),
                                            reached_top: response.reached_top,
                                            guild_id,
                                            channel_id,
                                            message_id: oldest_msg_id,
                                            direction: Direction::BeforeUnspecified,
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
                modal.guild_id = self.current_guild_id;
                modal.is_edit = false;
                self.profile_edit_modal.show(true);
                return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
            }
            Message::SelectedGuildMenuOption(option) => match option {
                GuildMenuOption::EditGuild => {
                    let guild_id = self.current_guild_id.unwrap(); // [ref:guild_menu_entry]
                    return client.guilds.get(&guild_id).map_or_else(Command::none, |_| {
                        Command::perform(
                            ready(TopLevelMessage::PushScreen(Box::new(TopLevelScreen::GuildSettings(
                                super::GuildSettings::new(guild_id).into(),
                            )))),
                            identity,
                        )
                    });
                }
                GuildMenuOption::LeaveGuild => {
                    let guild_id = self.current_guild_id.unwrap(); // [ref:guild_menu_entry]
                    return client.mk_cmd(
                        |inner| async move { inner.chat().await.leave_guild(GuildId::new(guild_id)).await },
                        |_| TopLevelMessage::Nothing,
                    );
                }
            },
            Message::SelectedAppMenuOption(option) => match option {
                ProfileMenuOption::ManageEmotes => {
                    return TopLevelScreen::push_screen_cmd(TopLevelScreen::EmoteManagement(Box::new(
                        Default::default(),
                    )));
                }
                ProfileMenuOption::Logout => {
                    self.logout_modal.show(true);
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
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
                    modal.guild_id = None;
                    modal.is_edit = true;
                    self.profile_edit_modal.show(true);
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
                }
                ProfileMenuOption::CopyToken => {
                    let token = client.auth_status().session().unwrap().session_token.clone();
                    return iced::clipboard::write(token);
                }
                ProfileMenuOption::Help => {
                    self.help_modal.show(true);
                    return self.update(Message::ChangeMode(Mode::Normal), client, thumbnail_cache);
                }
                ProfileMenuOption::Exit => {
                    return Command::perform(async { TopLevelMessage::Exit }, identity);
                }
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
                            |inner| async move { inner.chat().await.typing(Typing::new(gid, cid)).await },
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
                                    emote: None,
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
                                    emote: None,
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
                let replace_stuff = |text: &str| {
                    let mut text = text.to_string();
                    if let Some(guild) = client.guilds.get(&self.current_guild_id.unwrap()) {
                        for (id, member) in client.members.iter().filter(|(id, _)| guild.members.contains_key(id)) {
                            use client::byte_writer::Writer;
                            use std::fmt::Write;

                            let mut pattern_arr = [b'0'; 23];
                            write!(Writer(&mut pattern_arr), "<@{}>", id).unwrap();
                            text = text.replace(
                                &format!("@{}", member.username),
                                (unsafe { std::str::from_utf8_unchecked(&pattern_arr) }).trim_end_matches(|c| c != '>'),
                            );
                        }
                    }
                    for pack in client.emote_packs.values() {
                        for (image_id, name) in &pack.emotes {
                            text = text.replace(&format!(":{}:", name), &format!("<:{}:>", image_id));
                        }
                    }
                    text
                };

                if !self.message.trim().is_empty() {
                    match self.mode {
                        Mode::EditingMessage(message_id) => {
                            let new_content = replace_stuff(self.message.trim());
                            self.message.clear();
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
                                content: IcyContent::Text(replace_stuff(self.message.trim())),
                                sender: client.user_id.unwrap(),
                                reply_to: self.reply_to.take(),
                                ..Default::default()
                            };
                            self.message.clear();
                            if let Some(cmd) = client.send_msg_cmd(
                                guild_id,
                                channel_id,
                                Duration::from_secs(0),
                                MessageId::default(),
                                message,
                            ) {
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
            Message::SelectFilesToSend => {
                if let (Some(guild_id), Some(channel_id)) = (self.current_guild_id, self.current_channel_id) {
                    return Command::perform(select_files(false), move |result| {
                        result.map_or_else(
                            |err| {
                                if matches!(err, ClientError::Custom(_)) {
                                    error!("{}", err);
                                    TopLevelMessage::Nothing
                                } else {
                                    TopLevelMessage::Error(Box::new(err))
                                }
                            },
                            |files| {
                                TopLevelMessage::main(Message::UploadFiles {
                                    guild_id,
                                    channel_id,
                                    files,
                                })
                            },
                        )
                    });
                }
            }
            Message::UploadFiles {
                guild_id,
                channel_id,
                files,
            } => {
                if let Some(channel) = client.get_channel(guild_id, channel_id) {
                    channel.uploading_files = files.iter().map(content::get_filename).collect();

                    let inner = client.inner_arc();
                    let content_store = client.content_store_arc();
                    return Command::perform(
                        async move { super::upload_files(&inner, content_store, files).await },
                        move |result| {
                            TopLevelMessage::main(Message::UploadResult {
                                guild_id,
                                channel_id,
                                result: Box::new(result),
                            })
                        },
                    );
                }
            }
            Message::UploadResult {
                guild_id,
                channel_id,
                result,
            } => {
                if let Some(channel) = client.get_channel(guild_id, channel_id) {
                    channel.uploading_files.clear();
                    match *result {
                        Ok(attachments) => {
                            let sender = client.user_id.unwrap();
                            return Command::perform(
                                ready(TopLevelMessage::SendMessage {
                                    message: IcyMessage {
                                        content: IcyContent::Files(attachments),
                                        sender,
                                        ..Default::default()
                                    },
                                    retry_after: Duration::from_secs(0),
                                    guild_id,
                                    channel_id,
                                }),
                                identity,
                            );
                        }
                        Err(err) => {
                            return Command::perform(ready(TopLevelMessage::Error(Box::new(err))), identity);
                        }
                    }
                }
            }
            Message::GuildChanged(guild_id) => {
                self.mode = Mode::Normal;
                self.message.clear();
                self.current_guild_id = Some(guild_id);
                if let Some(guild) = client.get_guild(guild_id) {
                    if guild.channels.is_empty() && !guild.init_fetching {
                        guild.init_fetching = true;
                        let get_roles = guild.has_perm(ROLES_GET);
                        let get_user_roles = guild.has_perm(ROLES_USER_MANAGE);
                        let inner = client.inner_arc();
                        return Command::perform(
                            async move {
                                let channels_list = inner.call(GetGuildChannelsRequest::new(guild_id)).await?.channels;
                                let mut events: Vec<ClientResult<Event>> = Vec::with_capacity(channels_list.len());
                                events.extend(channels_list.into_iter().filter_map(|c| {
                                    let channel = c.channel?;
                                    Some(Ok(Event::Chat(ChatEvent::CreatedChannel(ChannelCreated {
                                        guild_id,
                                        channel_id: c.channel_id,
                                        kind: channel.kind,
                                        name: channel.channel_name,
                                        metadata: channel.metadata,
                                        ..Default::default()
                                    }))))
                                }));

                                if get_roles {
                                    events.extend(inner.call(GetGuildRolesRequest::new(guild_id)).await.map_or_else(
                                        |err| vec![Err(err.into())],
                                        |roles| {
                                            roles
                                                .roles
                                                .into_iter()
                                                .filter_map(|r| {
                                                    let role = r.role?;
                                                    Some(Ok(Event::Chat(ChatEvent::RoleCreated(RoleCreated {
                                                        guild_id,
                                                        role_id: r.role_id,
                                                        color: role.color,
                                                        hoist: role.hoist,
                                                        name: role.name,
                                                        pingable: role.pingable,
                                                    }))))
                                                })
                                                .collect::<Vec<_>>()
                                        },
                                    ));
                                }

                                let members = inner.call(GetGuildMembersRequest::new(guild_id)).await?.members;
                                events.reserve(members.len() * 2);
                                if get_user_roles {
                                    for id in &members {
                                        events.push(
                                            inner
                                                .call(GetUserRolesRequest::new(guild_id, *id))
                                                .await
                                                .map(|resp| {
                                                    Event::Chat(ChatEvent::UserRolesUpdated(UserRolesUpdated {
                                                        guild_id,
                                                        user_id: *id,
                                                        new_role_ids: resp.roles,
                                                    }))
                                                })
                                                .map_err(Into::into),
                                        );
                                    }
                                }
                                let member_events = members.into_iter().map(|member_id| {
                                    Ok(Event::Chat(ChatEvent::JoinedMember(MemberJoined {
                                        member_id,
                                        guild_id,
                                    })))
                                });
                                events.extend(member_events);

                                ClientResult::Ok(events)
                            },
                            move |events| TopLevelMessage::InitialGuildLoad { guild_id, events },
                        );
                    } else {
                        let switch_to = self
                            .guild_last_channels
                            .get(&guild_id)
                            .copied()
                            .or_else(|| guild.channels.first().map(|(id, _)| *id));

                        if let Some(id) = switch_to {
                            return self.update(Message::ChannelChanged(id), client, thumbnail_cache);
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
                        let convert_to_event = move |m: GetChannelMessagesResponse| {
                            Box::new(TopLevelMessage::GetChannelMessagesResponse {
                                guild_id,
                                channel_id,
                                message_id: 0,
                                messages: m
                                    .messages
                                    .into_iter()
                                    .flat_map(|m| {
                                        let msg = m.message?;
                                        Some((m.message_id, msg))
                                    })
                                    .collect(),
                                reached_top: m.reached_top,
                                direction: Direction::BeforeUnspecified,
                            })
                        };
                        c.init_fetching = true;
                        let inner = client.inner_arc();
                        cmds.push(Command::perform(
                            async move {
                                inner
                                    .call(GetChannelMessages::new(guild_id, channel_id))
                                    .await
                                    .map(convert_to_event)
                                    .map_err(ClientError::from)
                            },
                            move |events| TopLevelMessage::InitialChannelLoad {
                                guild_id,
                                channel_id,
                                events,
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
            Message::FocusComposer(c) => {
                if self.current_guild_id.is_some()
                    && self.current_channel_id.is_some()
                    && !self.composer_state.is_focused()
                {
                    self.composer_state.focus();
                    self.message.push(c);
                    self.composer_state.move_cursor_to_end();
                }
            }
            Message::CopyToClipboard(value) => return iced::clipboard::write(value),
            Message::MessageMenuSelected(option) => match option {
                MessageMenuOption::Copy(id) => {
                    if let (Some(guild_id), Some(channel_id)) = (self.current_guild_id, self.current_channel_id) {
                        return client
                            .guilds
                            .get(&guild_id)
                            .and_then(|g| g.channels.get(&channel_id))
                            .and_then(|c| c.messages.get(&id))
                            .map_or_else(Command::none, |m| {
                                if let IcyContent::Text(text) = &m.content {
                                    iced::clipboard::write(text.clone())
                                } else {
                                    Command::none()
                                }
                            });
                    }
                }
                MessageMenuOption::Reply(id) => self.reply_to = Some(id),
                MessageMenuOption::Edit(id) => {
                    return self.update(Message::ChangeMode(Mode::EditingMessage(id)), client, thumbnail_cache);
                }
                MessageMenuOption::Delete(message_id) => {
                    if let (Some(guild_id), Some(channel_id)) = (self.current_guild_id, self.current_channel_id) {
                        return Command::perform(
                            client.delete_msg_cmd(guild_id, channel_id, message_id),
                            ResultExt::map_to_nothing,
                        );
                    }
                }
                MessageMenuOption::CopyMessageId(id) => return iced::clipboard::write(id.to_string()),
            },
            Message::ClearReply => self.reply_to = None,
            Message::AutoCompleteBefore => {
                if let Some(before) = &self.before_after_completion_items.0 {
                    self.completion_current = Some(before.clone());
                    self.composer_state.unfocus();
                }
            }
            Message::AutoCompleteNext => {
                if let Some(after) = &self.before_after_completion_items.1 {
                    self.completion_current = Some(after.clone());
                    self.composer_state.unfocus();
                }
            }
            Message::AutoComplete => {
                if let Some(completion_item) = self.completion_current.take() {
                    if let Some((word, start, end)) = self.composer_state.get_word_at_cursor(&self.message) {
                        if word.starts_with(':') && !word.ends_with(':') {
                            self.message.drain(start..end);

                            let mut idx = start;
                            self.message.insert(idx, ':');
                            idx += 1;
                            self.message.insert_str(idx, completion_item.as_str());
                            idx += completion_item.len();
                            self.message.insert(idx, ':');

                            self.composer_state.focus();
                            self.composer_state.move_cursor_to(idx + 1);
                        } else if word.starts_with('@') {
                            self.message.drain(start..end);

                            let mut idx = start;
                            self.message.insert(idx, '@');
                            idx += 1;
                            self.message.insert_str(idx, completion_item.as_str());
                            idx += completion_item.len();

                            self.composer_state.focus();
                            self.composer_state.move_cursor_to(idx + 1);
                        }
                    }
                }
            }
        }

        Command::none()
    }

    pub fn subscription(&self) -> Subscription<TopLevelMessage> {
        use iced_native::{event::Status, keyboard, Event};

        let filter_events = |ev: Event, status: Status| -> Option<TopLevelMessage> {
            type Ke = keyboard::Event;
            type Kc = keyboard::KeyCode;

            match ev {
                Event::Keyboard(Ke::KeyPressed {
                    key_code: Kc::Escape, ..
                }) => Some(TopLevelMessage::main(Message::ChangeMode(Mode::Normal))),
                Event::Keyboard(Ke::KeyPressed {
                    key_code: Kc::K,
                    modifiers,
                }) => modifiers.control().then(|| TopLevelMessage::main(Message::QuickSwitch)),
                Event::Keyboard(Ke::KeyPressed {
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
                Event::Keyboard(Ke::KeyPressed {
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
                Event::Keyboard(Ke::KeyPressed {
                    key_code: Kc::Tab,
                    modifiers,
                }) => {
                    let msg = modifiers
                        .shift()
                        .then(|| Message::AutoCompleteBefore)
                        .unwrap_or(Message::AutoCompleteNext);
                    Some(TopLevelMessage::main(msg))
                }
                Event::Keyboard(Ke::KeyPressed {
                    key_code: Kc::Enter, ..
                }) => Some(TopLevelMessage::main(Message::AutoComplete)),
                Event::Keyboard(Ke::CharacterReceived(c)) => (matches!(status, Status::Ignored)
                    && !['', '\t'].contains(&c))
                .then(|| TopLevelMessage::main(Message::FocusComposer(c))),
                _ => None,
            }
        };

        iced_native::subscription::events_with(filter_events)
    }

    pub fn on_error(&mut self, error: ClientError) -> Command<TopLevelMessage> {
        if let ClientError::Internal(InnerClientError::Reqwest(error)) = &error {
            if error.status() == Some(StatusCode::NOT_FOUND) {
                return Command::none();
            }
        }
        self.error_text = error.to_string();
        self.logout_modal.show(false);
        Command::none()
    }
}
