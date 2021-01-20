use crate::{
    client::{channel::Channels, content::ThumbnailCache, guild::Guilds},
    ui::{
        component::*,
        style::{Theme, PADDING, SPACING},
    },
};
use fuzzy_matcher::skim::SkimMatcherV2;
use harmony_rust_sdk::client::api::rest::FileId;

/// Builds a room list.
#[allow(clippy::clippy::too_many_arguments)]
pub fn build_channel_list<'a, Message: Clone + 'a>(
    channels: &Channels,
    current_channel_id: Option<u64>,
    channel_filter_text: &str,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [button::State],
    on_button_press: fn(u64) -> Message,
    theme: Theme,
) -> (Element<'a, Message>, Option<u64>) {
    let mut channels = channels
        .iter()
        .map(|(channel_id, channel)| (*channel_id, channel.name.clone(), channel.is_category))
        .collect::<Vec<(u64, String, bool)>>();

    if channel_filter_text.is_empty() {
        channels.sort_unstable_by(|(_, channel_name, _), (_, other_channel_name, _)| {
            channel_name.cmp(&other_channel_name)
        });
    } else {
        let matcher = SkimMatcherV2::default();

        let mut channels_filtered = channels
            .drain(..)
            .flat_map(|(channel_id, channel_name, is_category)| {
                Some((
                    matcher.fuzzy(&channel_name, channel_filter_text, false)?.0, // extract match score
                    channel_id,
                    channel_name,
                    is_category,
                ))
            })
            .collect::<Vec<_>>();
        channels_filtered.sort_unstable_by_key(|(score, _, _, _)| *score);
        channels = channels_filtered
            .into_iter()
            .rev()
            .map(|(_, channel_id, channel_name, is_category)| {
                (channel_id, channel_name, is_category)
            })
            .collect();
    }

    let first_channel_id = channels.first().cloned().map(|(id, _, _)| id);

    let mut channel_list = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(Length::Fill)
        .spacing(SPACING * 2)
        .padding(PADDING / 4);

    let is_current_channel = |channel_id: u64| {
        if let Some(id) = current_channel_id {
            if channel_id == id {
                return true;
            }
        }
        false
    };

    for ((channel_id, channel_name, is_category), button_state) in
        channels.into_iter().zip(buttons_state.iter_mut())
    {
        let channel_name_prefix = if is_category { "+" } else { "#" };
        let channel_name_formatted = format!("{}{}", channel_name_prefix, channel_name);
        let content = label(channel_name_formatted);

        let mut but = Button::new(button_state, content)
            .width(Length::Fill)
            .style(theme.secondary());

        if !is_current_channel(channel_id) {
            but = but.on_press(on_button_press(channel_id));
        }

        channel_list = channel_list.push(but);
    }

    (channel_list.into(), first_channel_id)
}

#[allow(clippy::clippy::too_many_arguments)]
pub fn build_guild_list<'a, Message: Clone + 'a>(
    guilds: &Guilds,
    thumbnail_cache: &ThumbnailCache,
    current_guild_id: Option<u64>,
    guild_filter_text: &str,
    state: &'a mut scrollable::State,
    buttons_state: &'a mut [button::State],
    on_button_press: fn(u64) -> Message,
    theme: Theme,
) -> (Element<'a, Message>, Option<u64>) {
    let mut guilds = guilds
        .iter()
        .map(|(guild_id, guild)| (*guild_id, guild.name.clone(), guild.picture.clone()))
        .collect::<Vec<(u64, String, Option<FileId>)>>();

    if guild_filter_text.is_empty() {
        guilds.sort_unstable_by(|(_, guild_name, _), (_, other_guild_name, _)| {
            guild_name.cmp(&other_guild_name)
        });
    } else {
        let matcher = SkimMatcherV2::default();

        let mut guilds_filtered = guilds
            .drain(..)
            .flat_map(|(guild_id, guild_name, guild_picture)| {
                Some((
                    matcher.fuzzy(&guild_name, guild_filter_text, false)?.0, // extract match score
                    guild_id,
                    guild_name,
                    guild_picture,
                ))
            })
            .collect::<Vec<_>>();
        guilds_filtered.sort_unstable_by_key(|(score, _, _, _)| *score);
        guilds = guilds_filtered
            .into_iter()
            .rev()
            .map(|(_, guild_id, guild_name, guild_picture)| (guild_id, guild_name, guild_picture))
            .collect();
    }

    let first_guild_id = guilds.first().cloned().map(|(id, _, _)| id);

    let mut guild_list = Scrollable::new(state)
        .style(theme)
        .align_items(Align::Start)
        .height(Length::Fill)
        .spacing(SPACING * 2)
        .padding(PADDING / 4);

    let is_current_guild = |room_id: u64| {
        if let Some(id) = current_guild_id {
            if room_id == id {
                return true;
            }
        }
        false
    };

    for ((guild_id, guild_name, guild_picture), button_state) in
        guilds.into_iter().zip(buttons_state.iter_mut())
    {
        let content = fill_container(
            if let Some(handle) = guild_picture
                .map(|guild_picture| thumbnail_cache.get_thumbnail(&guild_picture))
                .flatten()
            {
                Element::from(Image::new(handle.clone()))
            } else {
                Element::from(label(guild_name.chars().next().unwrap()).size(30))
            },
        );

        let mut but = Button::new(button_state, content)
            .width(Length::Fill)
            .style(theme.secondary());

        if !is_current_guild(guild_id) {
            but = but.on_press(on_button_press(guild_id));
        }

        guild_list = guild_list.push(but);
    }

    (guild_list.into(), first_guild_id)
}
