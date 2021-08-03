use crate::color;
use client::{bool_ext::BoolExt, harmony_rust_sdk::api::harmonytypes::UserStatus};
use iced::{
    button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider, text_input, toggler, Color,
};
use iced_aw::tabs;

pub const DEF_SIZE: u16 = 20;
pub const MESSAGE_TIMESTAMP_SIZE: u16 = 14;
pub const MESSAGE_SIZE: u16 = 18;
pub const MESSAGE_SENDER_SIZE: u16 = 21;
pub const DATE_SEPERATOR_SIZE: u16 = 24;

pub const PADDING: u16 = 16;
pub const SPACING: u16 = 4;

pub const ERROR_COLOR: Color = color!(. 1.0, 0.0, 0.0);
pub const SUCCESS_COLOR: Color = color!(. 0.0, 1.0, 0.0);
pub const ALT_COLOR: Color = color!(. 0.5, 0.5, 0.5);

pub const AVATAR_WIDTH: u16 = 44;
pub const PROFILE_AVATAR_WIDTH: u16 = 96;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    dark: bool,
    secondary: bool,
    round: bool,
    embed: bool,
    overrides: OverrideStyle,
}

impl Theme {
    const SENDER_COLORS: [Color; 8] = [
        color!(109, 221, 24),
        color!(252, 210, 0),
        color!(204, 249, 255),
        color!(61, 219, 140),
        color!(221, 106, 53),
        color!(226, 34, 69),
        color!(9, 229, 56),
        color!(209, 50, 113),
    ];

    pub const fn calculate_sender_color(&self, name_len: usize) -> Color {
        Theme::SENDER_COLORS[name_len % Theme::SENDER_COLORS.len()]
    }

    pub fn status_color(&self, status: UserStatus) -> Color {
        match status {
            UserStatus::Offline => ALT_COLOR,
            UserStatus::DoNotDisturb => color!(160, 0, 0),
            UserStatus::Idle => color!(200, 140, 0),
            UserStatus::OnlineUnspecified => color!(0, 160, 0),
            UserStatus::Streaming => color!(160, 0, 160),
        }
    }

    pub const fn secondary(mut self) -> Self {
        self.secondary = true;
        self
    }

    pub const fn round(mut self) -> Self {
        self.round = true;
        self
    }

    pub const fn embed(mut self) -> Self {
        self.embed = true;
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.overrides.border_color = Some(color);
        self
    }

    pub fn border_radius(mut self, radius: f32) -> Self {
        self.overrides.border_radius = Some(radius);
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.overrides.border_width = Some(width);
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            dark: true,
            secondary: false,
            round: false,
            embed: false,
            overrides: Default::default(),
        }
    }
}

pub struct TabBar;

impl From<Theme> for Box<dyn tabs::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            dark::TabBar.into()
        } else {
            light::TabBar.into()
        }
    }
}

impl From<Theme> for Box<dyn container::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| {
            if theme.secondary {
                if theme.round {
                    dark::BrightRoundContainer(theme.overrides).into()
                } else {
                    dark::BrightContainer(theme.overrides).into()
                }
            } else if theme.round {
                dark::RoundContainer(theme.overrides).into()
            } else {
                dark::Container(theme.overrides).into()
            }
        })
    }
}

impl From<Theme> for Box<dyn radio::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::Radio.into())
    }
}

impl From<Theme> for Box<dyn text_input::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| {
            if theme.secondary {
                dark::DarkTextInput.into()
            } else {
                dark::TextInput.into()
            }
        })
    }
}

impl From<Theme> for Box<dyn button::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            if theme.secondary {
                dark::DarkButton.into()
            } else if theme.embed {
                dark::EmbedButton.into()
            } else {
                dark::Button.into()
            }
        } else {
            light::Button.into()
        }
    }
}

impl From<Theme> for Box<dyn scrollable::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::Scrollable.into())
    }
}

impl From<Theme> for Box<dyn slider::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::Slider.into())
    }
}

impl From<Theme> for Box<dyn progress_bar::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::ProgressBar.into())
    }
}

impl From<Theme> for Box<dyn checkbox::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::Checkbox.into())
    }
}

impl From<Theme> for Box<dyn pick_list::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::PickList.into())
    }
}

impl From<Theme> for Box<dyn rule::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| {
            if theme.secondary {
                dark::RuleBright.into()
            } else {
                dark::Rule.into()
            }
        })
    }
}

impl From<Theme> for Box<dyn iced_aw::modal::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::Modal.into())
    }
}

impl From<Theme> for Box<dyn iced_aw::card::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::Card.into())
    }
}

impl From<Theme> for Box<dyn toggler::StyleSheet> {
    fn from(theme: Theme) -> Self {
        theme.dark.map_or_default(|| dark::Toggler.into())
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct OverrideStyle {
    border_color: Option<Color>,
    border_radius: Option<f32>,
    border_width: Option<f32>,
}

impl OverrideStyle {
    fn container(self, mut style: container::Style) -> container::Style {
        if let Some(color) = self.border_color {
            style.border_color = color;
        }
        if let Some(radius) = self.border_radius {
            style.border_radius = radius;
        }
        if let Some(width) = self.border_width {
            style.border_width = width;
        }
        style
    }
}

mod light {
    use crate::color;
    use iced::{button, Background, Color, Vector};
    use iced_aw::style::tab_bar::Style;
    use iced_aw::tabs;

    pub struct TabBar;

    impl tabs::StyleSheet for TabBar {
        fn active(&self, is_selected: bool) -> tabs::Style {
            let tab_label_background = if is_selected {
                Background::Color(Color::BLACK)
            } else {
                Background::Color(Color::WHITE)
            };

            let text_color = if is_selected { Color::WHITE } else { Color::BLACK };

            Style {
                background: None,
                border_color: None,
                border_width: 0.0,
                tab_label_background,
                tab_label_border_color: Color::TRANSPARENT,
                tab_label_border_width: 0.0,
                icon_color: text_color,
                text_color,
            }
        }

        fn hovered(&self, is_selected: bool) -> tabs::Style {
            let tab_label_background = Background::Color(Color::BLACK);
            let text_color = Color::WHITE;

            Style {
                tab_label_background,
                icon_color: text_color,
                text_color,
                ..self.active(is_selected)
            }
        }
    }

    pub struct Button;

    impl button::StyleSheet for Button {
        fn active(&self) -> button::Style {
            button::Style {
                background: color!(28, 108, 223).into(),
                border_radius: 12.0,
                shadow_offset: Vector::new(1.0, 1.0),
                text_color: color!(238, 238, 238),
                ..button::Style::default()
            }
        }

        fn hovered(&self) -> button::Style {
            button::Style {
                text_color: Color::WHITE,
                shadow_offset: Vector::new(1.0, 2.0),
                ..self.active()
            }
        }
    }
}

mod dark {
    use crate::color;
    use iced::{
        button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider, text_input, toggler,
        Background, Color,
    };
    use iced_aw::tabs::Style;
    use iced_aw::{card, modal, tabs};

    use super::OverrideStyle;

    const DARK_BG: Color = color!(0x0A, 0x0D, 0x13);
    const BRIGHT_BG: Color = color!(0x16, 0x19, 0x1F);
    const DISABLED: Color = color!(0x26, 0x29, 0x2F);
    const ACCENT: Color = color!(0x00, 0x8F, 0xCF); // 00BFFF
    const DISABLED_TEXT: Color = color!(0xDD, 0xDD, 0xDD);
    const TEXT_COLOR: Color = color!(0xEE, 0xEE, 0xEE);

    pub struct Toggler;

    impl toggler::StyleSheet for Toggler {
        fn active(&self, is_active: bool) -> toggler::Style {
            let mut style = toggler::Style {
                background: DARK_BG,
                foreground: ACCENT,
                background_border: Some(BRIGHT_BG),
                foreground_border: None,
            };

            if !is_active {
                style.foreground = DISABLED;
            }

            style
        }

        fn hovered(&self, _is_active: bool) -> toggler::Style {
            toggler::Style {
                background: DARK_BG,
                foreground: ACCENT,
                background_border: Some(BRIGHT_BG),
                foreground_border: Some(BRIGHT_BG),
            }
        }
    }

    pub struct TabBar;

    impl tabs::StyleSheet for TabBar {
        fn active(&self, is_selected: bool) -> tabs::Style {
            let tab_label_background = if is_selected {
                Background::Color(BRIGHT_BG)
            } else {
                Background::Color(DARK_BG)
            };

            let text_color = if is_selected { ACCENT } else { Color::WHITE };

            Style {
                background: None,
                border_color: None,
                border_width: 0.0,
                tab_label_background,
                tab_label_border_color: Color::TRANSPARENT,
                tab_label_border_width: 0.0,
                icon_color: text_color,
                text_color,
            }
        }

        fn hovered(&self, is_selected: bool) -> tabs::Style {
            let tab_label_background = Background::Color(BRIGHT_BG);
            let text_color = ACCENT;

            Style {
                tab_label_background,
                icon_color: text_color,
                text_color,
                ..self.active(is_selected)
            }
        }
    }

    pub struct Card;

    impl card::StyleSheet for Card {
        fn active(&self) -> card::Style {
            card::Style {
                background: DARK_BG.into(),
                head_background: BRIGHT_BG.into(),
                border_color: BRIGHT_BG,
                foot_background: DARK_BG.into(),
                body_text_color: TEXT_COLOR,
                foot_text_color: TEXT_COLOR,
                head_text_color: TEXT_COLOR,
                close_color: TEXT_COLOR,
                border_width: 0.0,
                border_radius: 6.0,
                ..Default::default()
            }
        }
    }

    pub struct Modal;

    impl modal::StyleSheet for Modal {
        fn active(&self) -> modal::Style {
            modal::Style {
                background: Color { a: 0.7, ..Color::BLACK }.into(),
            }
        }
    }

    pub struct Container(pub(super) OverrideStyle);

    impl container::StyleSheet for Container {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                background: DARK_BG.into(),
                text_color: Some(TEXT_COLOR),
                ..container::Style::default()
            })
        }
    }

    pub struct RoundContainer(pub(super) OverrideStyle);

    impl container::StyleSheet for RoundContainer {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                border_color: DARK_BG,
                border_radius: 8.0,
                border_width: 2.0,
                ..Container(self.0).style()
            })
        }
    }

    pub struct BrightRoundContainer(pub(super) OverrideStyle);

    impl container::StyleSheet for BrightRoundContainer {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                border_color: BRIGHT_BG,
                border_radius: 8.0,
                border_width: 2.0,
                ..BrightContainer(self.0).style()
            })
        }
    }

    pub struct BrightContainer(pub(super) OverrideStyle);

    impl container::StyleSheet for BrightContainer {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                background: BRIGHT_BG.into(),
                ..Container(self.0).style()
            })
        }
    }

    pub struct Radio;

    impl radio::StyleSheet for Radio {
        fn active(&self) -> radio::Style {
            radio::Style {
                background: BRIGHT_BG.into(),
                dot_color: ACCENT,
                border_width: 1.0,
                border_color: ACCENT,
            }
        }

        fn hovered(&self) -> radio::Style {
            radio::Style {
                background: Color { a: 0.5, ..BRIGHT_BG }.into(),
                ..self.active()
            }
        }
    }

    pub struct DarkTextInput;

    impl text_input::StyleSheet for DarkTextInput {
        fn active(&self) -> text_input::Style {
            text_input::Style {
                background: DARK_BG.into(),
                ..TextInput.active()
            }
        }

        fn focused(&self) -> text_input::Style {
            text_input::Style {
                border_width: 3.0,
                border_color: ACCENT,
                ..self.active()
            }
        }

        fn placeholder_color(&self) -> Color {
            color!(. 0.4, 0.4, 0.4)
        }

        fn value_color(&self) -> Color {
            TextInput.value_color()
        }

        fn selection_color(&self) -> Color {
            TextInput.selection_color()
        }

        fn hovered(&self) -> text_input::Style {
            text_input::Style {
                border_width: 2.0,
                border_color: Color { a: 0.5, ..ACCENT },
                ..self.focused()
            }
        }
    }

    pub struct TextInput;

    impl text_input::StyleSheet for TextInput {
        fn active(&self) -> text_input::Style {
            text_input::Style {
                background: BRIGHT_BG.into(),
                border_radius: 8.0,
                border_width: 0.0,
                border_color: ACCENT,
            }
        }

        fn focused(&self) -> text_input::Style {
            text_input::Style {
                border_width: 3.0,
                border_color: ACCENT,
                ..self.active()
            }
        }

        fn placeholder_color(&self) -> Color {
            color!(153, 153, 153)
        }

        fn value_color(&self) -> Color {
            TEXT_COLOR
        }

        fn selection_color(&self) -> Color {
            ACCENT
        }

        fn hovered(&self) -> text_input::Style {
            text_input::Style {
                border_width: 2.0,
                border_color: Color { a: 0.5, ..ACCENT },
                ..self.focused()
            }
        }
    }

    pub struct DarkButton;

    impl button::StyleSheet for DarkButton {
        fn active(&self) -> button::Style {
            button::Style {
                background: DARK_BG.into(),
                border_radius: 8.0,
                text_color: TEXT_COLOR,
                ..button::Style::default()
            }
        }

        fn hovered(&self) -> button::Style {
            button::Style {
                background: ACCENT.into(),
                ..self.active()
            }
        }

        fn pressed(&self) -> button::Style {
            button::Style {
                border_width: 1.0,
                border_color: Color::WHITE,
                ..self.hovered()
            }
        }

        fn disabled(&self) -> button::Style {
            button::Style {
                background: DISABLED.into(),
                text_color: DISABLED_TEXT,
                ..self.active()
            }
        }
    }

    pub struct EmbedButton;

    impl button::StyleSheet for EmbedButton {
        fn active(&self) -> button::Style {
            DarkButton.active()
        }

        fn hovered(&self) -> button::Style {
            DarkButton.hovered()
        }

        fn pressed(&self) -> button::Style {
            DarkButton.pressed()
        }

        fn disabled(&self) -> button::Style {
            DarkButton.active()
        }
    }

    pub struct Button;

    impl button::StyleSheet for Button {
        fn active(&self) -> button::Style {
            button::Style {
                background: BRIGHT_BG.into(),
                border_radius: 8.0,
                text_color: TEXT_COLOR,
                ..button::Style::default()
            }
        }

        fn hovered(&self) -> button::Style {
            button::Style {
                background: ACCENT.into(),
                ..self.active()
            }
        }

        fn pressed(&self) -> button::Style {
            button::Style {
                border_width: 1.0,
                border_color: Color::WHITE,
                ..self.hovered()
            }
        }

        fn disabled(&self) -> button::Style {
            button::Style {
                background: DISABLED.into(),
                text_color: DISABLED_TEXT,
                ..self.active()
            }
        }
    }

    pub struct Scrollable;

    impl scrollable::StyleSheet for Scrollable {
        fn active(&self) -> scrollable::Scrollbar {
            scrollable::Scrollbar {
                background: Color::TRANSPARENT.into(),
                border_radius: 2.0,
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
                scroller: scrollable::Scroller {
                    color: Color::TRANSPARENT,
                    border_radius: 2.0,
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                },
            }
        }

        fn hovered(&self) -> scrollable::Scrollbar {
            let active = self.active();

            scrollable::Scrollbar {
                background: Color { a: 0.5, ..BRIGHT_BG }.into(),
                scroller: scrollable::Scroller {
                    color: ACCENT,
                    ..active.scroller
                },
                ..active
            }
        }

        fn dragging(&self) -> scrollable::Scrollbar {
            let hovered = self.hovered();

            scrollable::Scrollbar {
                scroller: scrollable::Scroller {
                    color: color!(217, 217, 217),
                    ..hovered.scroller
                },
                ..hovered
            }
        }
    }

    pub struct Slider;

    impl slider::StyleSheet for Slider {
        fn active(&self) -> slider::Style {
            slider::Style {
                rail_colors: (ACCENT, Color { a: 0.1, ..ACCENT }),
                handle: slider::Handle {
                    shape: slider::HandleShape::Circle { radius: 9.0 },
                    color: ACCENT,
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                },
            }
        }

        fn hovered(&self) -> slider::Style {
            let active = self.active();

            slider::Style {
                handle: slider::Handle {
                    color: ACCENT,
                    ..active.handle
                },
                ..active
            }
        }

        fn dragging(&self) -> slider::Style {
            let active = self.active();

            slider::Style {
                handle: slider::Handle {
                    color: color!(217, 217, 217),
                    ..active.handle
                },
                ..active
            }
        }
    }

    pub struct ProgressBar;

    impl progress_bar::StyleSheet for ProgressBar {
        fn style(&self) -> progress_bar::Style {
            progress_bar::Style {
                background: BRIGHT_BG.into(),
                bar: ACCENT.into(),
                border_radius: 10.0,
            }
        }
    }

    pub struct Checkbox;

    impl checkbox::StyleSheet for Checkbox {
        fn active(&self, is_checked: bool) -> checkbox::Style {
            checkbox::Style {
                background: if is_checked { ACCENT } else { BRIGHT_BG }.into(),
                checkmark_color: Color::WHITE,
                border_radius: 2.0,
                border_width: 1.0,
                border_color: ACCENT,
            }
        }

        fn hovered(&self, is_checked: bool) -> checkbox::Style {
            checkbox::Style {
                background: Color {
                    a: 0.8,
                    ..if is_checked { ACCENT } else { BRIGHT_BG }
                }
                .into(),
                ..self.active(is_checked)
            }
        }
    }

    pub struct PickList;

    impl pick_list::StyleSheet for PickList {
        fn menu(&self) -> pick_list::Menu {
            pick_list::Menu {
                background: BRIGHT_BG.into(),
                text_color: TEXT_COLOR,
                selected_background: ACCENT.into(),
                selected_text_color: TEXT_COLOR,
                border_width: 3.0,
                border_color: Color::TRANSPARENT,
            }
        }

        fn active(&self) -> pick_list::Style {
            pick_list::Style {
                background: DARK_BG.into(),
                text_color: TEXT_COLOR,
                border_width: 8.0,
                border_radius: 8.0,
                border_color: DARK_BG,
                ..pick_list::Style::default()
            }
        }

        fn hovered(&self) -> pick_list::Style {
            pick_list::Style {
                background: ACCENT.into(),
                border_color: ACCENT,
                ..self.active()
            }
        }
    }

    pub struct Rule;

    impl rule::StyleSheet for Rule {
        fn style(&self) -> rule::Style {
            rule::Style {
                color: DARK_BG,
                width: 3,
                radius: 8.0,
                fill_mode: rule::FillMode::Padded(10),
            }
        }
    }

    pub struct RuleBright;

    impl rule::StyleSheet for RuleBright {
        fn style(&self) -> rule::Style {
            rule::Style {
                color: BRIGHT_BG,
                ..Rule.style()
            }
        }
    }
}
