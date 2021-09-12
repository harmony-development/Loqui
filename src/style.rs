use crate::color;
use client::{content::ColorschemeRaw, harmony_rust_sdk::api::profile::UserStatus};
use hex_color::HexColor;
use iced::{
    button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider, text_input, toggler, Color,
};
use iced_aw::{
    style::{self},
    tabs,
};

pub const DEF_SIZE: u16 = 20;
pub const MESSAGE_TIMESTAMP_SIZE: u16 = 14;
pub const MESSAGE_SIZE: u16 = 18;
pub const MESSAGE_SENDER_SIZE: u16 = 21;
pub const DATE_SEPERATOR_SIZE: u16 = 24;

pub const PADDING: u16 = 16;
pub const SPACING: u16 = 4;

pub const AVATAR_WIDTH: u16 = 44;
pub const PROFILE_AVATAR_WIDTH: u16 = 96;

pub const ERROR_COLOR: Color = color!(. 1.0, 0.0, 0.0);
pub const SUCCESS_COLOR: Color = color!(. 0.0, 1.0, 0.0);
pub const ALT_COLOR: Color = color!(. 0.65, 0.65, 0.65);
pub const DARKER: Color = color!(0x14, 0x14, 0x11);
pub const DARK_BG: Color = color!(0x1b, 0x1b, 0x18);
pub const BRIGHT_BG: Color = color!(0x26, 0x26, 0x22);
pub const DISABLED: Color = color!(0x67, 0x30, 0x28);
pub const ACCENT: Color = color!(0xaa, 0x50, 0x42);
pub const DISABLED_TEXT: Color = color!(0xe2, 0xc9, 0x9f);
pub const TEXT_COLOR: Color = color!(0xf2, 0xee, 0xd3);
#[derive(Debug, Clone, Copy, Default)]
pub struct Theme {
    secondary: bool,
    round: bool,
    embed: bool,
    overrides: OverrideStyle,
    pub colorscheme: Colorscheme,
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
            UserStatus::OfflineUnspecified => ALT_COLOR,
            UserStatus::DoNotDisturb => color!(160, 0, 0),
            UserStatus::Idle => color!(200, 140, 0),
            UserStatus::Online | UserStatus::Mobile => color!(0, 160, 0),
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

    pub fn background_color(mut self, color: Color) -> Self {
        self.overrides.background_color = Some(color);
        self
    }

    pub fn padded(mut self, pad: rule::FillMode) -> Self {
        self.overrides.padded = Some(pad);
        self
    }

    pub fn icon_size(mut self, icon_size: f32) -> Self {
        self.overrides.icon_size = Some(icon_size);
        self
    }

    pub fn text_color(mut self, text_color: Color) -> Self {
        self.overrides.text_color = Some(text_color);
        self
    }

    pub fn placeholder_color(mut self, placeholder_color: Color) -> Self {
        self.overrides.placeholder_color = Some(placeholder_color);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Colorscheme {
    pub error: Color,
    pub success: Color,
    pub border: Color,
    pub primary_bg: Color,
    pub secondary_bg: Color,
    pub disabled_bg: Color,
    pub text: Color,
    pub disabled_text: Color,
    pub accent: Color,
    pub mention_color: Color,
}

impl Default for Colorscheme {
    fn default() -> Self {
        Self {
            error: ERROR_COLOR,
            success: SUCCESS_COLOR,
            border: DARKER,
            primary_bg: DARK_BG,
            secondary_bg: BRIGHT_BG,
            disabled_bg: DISABLED,
            text: TEXT_COLOR,
            disabled_text: DISABLED_TEXT,
            accent: ACCENT,
            mention_color: color!(225, 225, 0, 180),
        }
    }
}

impl From<ColorschemeRaw> for Colorscheme {
    fn from(value: ColorschemeRaw) -> Self {
        let default = Colorscheme::default();
        Self {
            error: value.error.parse_to_color().unwrap_or(default.error),
            success: value.success.parse_to_color().unwrap_or(default.success),
            border: value.border.parse_to_color().unwrap_or(default.border),
            primary_bg: value.primary_bg.parse_to_color().unwrap_or(default.primary_bg),
            secondary_bg: value.secondary_bg.parse_to_color().unwrap_or(default.secondary_bg),
            disabled_bg: value.disabled_bg.parse_to_color().unwrap_or(default.disabled_bg),
            text: value.text.parse_to_color().unwrap_or(default.text),
            disabled_text: value.disabled_text.parse_to_color().unwrap_or(default.disabled_text),
            accent: value.accent.parse_to_color().unwrap_or(default.accent),
            mention_color: value.mention_color.parse_to_color().unwrap_or(default.mention_color),
        }
    }
}

trait ParseToColor {
    fn parse_to_color(&self) -> Option<Color>;
}

impl ParseToColor for String {
    fn parse_to_color(&self) -> Option<Color> {
        self.parse::<HexColor>()
            .ok()
            .map(|color| Color::from_rgb8(color.r, color.g, color.b))
    }
}

pub fn tuple_to_iced_color(color: [u8; 3]) -> Color {
    Color::from_rgb8(color[0], color[1], color[2])
}

impl From<Theme> for Box<dyn tabs::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::TabBar(theme.colorscheme).into()
    }
}

impl From<Theme> for Box<dyn container::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.secondary {
            if theme.round {
                styles::BrightRoundContainer(theme.overrides, theme.colorscheme).into()
            } else {
                styles::BrightContainer(theme.overrides, theme.colorscheme).into()
            }
        } else if theme.round {
            styles::RoundContainer(theme.overrides, theme.colorscheme).into()
        } else {
            styles::Container(theme.overrides, theme.colorscheme).into()
        }
    }
}

impl From<Theme> for Box<dyn radio::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::Radio(theme.colorscheme).into()
    }
}

impl From<Theme> for Box<dyn text_input::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.secondary {
            styles::DarkTextInput(theme.colorscheme).into()
        } else {
            styles::TextInput(theme.colorscheme).into()
        }
    }
}

impl From<Theme> for Box<dyn button::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.secondary {
            styles::DarkButton(theme.overrides, theme.colorscheme).into()
        } else if theme.embed {
            styles::EmbedButton(theme.overrides, theme.colorscheme).into()
        } else {
            styles::Button(theme.overrides, theme.colorscheme).into()
        }
    }
}

impl From<Theme> for Box<dyn scrollable::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::Scrollable(theme.colorscheme).into()
    }
}

impl From<Theme> for Box<dyn slider::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::Slider(theme.colorscheme).into()
    }
}

impl From<Theme> for Box<dyn progress_bar::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::ProgressBar(theme.colorscheme).into()
    }
}

impl From<Theme> for Box<dyn checkbox::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::Checkbox(theme.colorscheme).into()
    }
}

impl From<Theme> for Box<dyn pick_list::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::PickList(theme.colorscheme, theme.overrides).into()
    }
}

impl From<Theme> for Box<dyn rule::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.secondary {
            styles::RuleBright(theme.overrides, theme.colorscheme).into()
        } else {
            styles::Rule(theme.overrides, theme.colorscheme).into()
        }
    }
}

impl From<Theme> for Box<dyn iced_aw::modal::StyleSheet> {
    fn from(_: Theme) -> Self {
        styles::Modal.into()
    }
}

impl From<Theme> for Box<dyn iced_aw::card::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::Card(theme.colorscheme).into()
    }
}

impl From<Theme> for Box<dyn toggler::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::Toggler(theme.colorscheme).into()
    }
}

/*impl From<Theme> for Box<dyn number_input::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::NumberInput(theme.colorscheme).into()
    }
}*/

impl From<Theme> for Box<dyn style::color_picker::StyleSheet> {
    fn from(theme: Theme) -> Self {
        styles::ColorPicker(theme.colorscheme).into()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OverrideStyle {
    border_color: Option<Color>,
    border_radius: Option<f32>,
    border_width: Option<f32>,
    background_color: Option<Color>,
    padded: Option<rule::FillMode>,
    icon_size: Option<f32>,
    text_color: Option<Color>,
    placeholder_color: Option<Color>,
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
        if let Some(color) = self.background_color {
            style.background = Some(color.into());
        }
        if let Some(color) = self.text_color {
            style.text_color = Some(color);
        }
        style
    }

    fn button(self, mut style: button::Style) -> button::Style {
        if let Some(color) = self.border_color {
            style.border_color = color;
        }
        if let Some(radius) = self.border_radius {
            style.border_radius = radius;
        }
        if let Some(width) = self.border_width {
            style.border_width = width;
        }
        if let Some(color) = self.background_color {
            style.background = Some(color.into());
        }
        if let Some(color) = self.text_color {
            style.text_color = color;
        }
        style
    }

    fn rule(self, mut style: rule::Style) -> rule::Style {
        if let Some(color) = self.border_color {
            style.color = color;
        }
        if let Some(radius) = self.border_radius {
            style.radius = radius;
        }
        if let Some(width) = self.border_width {
            style.width = width as u16;
        }
        if let Some(mode) = self.padded {
            style.fill_mode = mode;
        }
        style
    }

    fn pick_list(self, mut style: pick_list::Style) -> pick_list::Style {
        if let Some(color) = self.border_color {
            style.border_color = color;
        }
        if let Some(radius) = self.border_radius {
            style.border_radius = radius;
        }
        if let Some(width) = self.border_width {
            style.border_width = width;
        }
        if let Some(color) = self.background_color {
            style.background = color.into();
        }
        if let Some(icon_size) = self.icon_size {
            style.icon_size = icon_size;
        }
        if let Some(color) = self.text_color {
            style.text_color = color;
        }
        if let Some(color) = self.placeholder_color {
            style.placeholder_color = color;
        }
        style
    }

    fn menu(self, mut style: pick_list::Menu) -> pick_list::Menu {
        if let Some(width) = self.border_width {
            style.border_width = width;
        }
        style
    }
}

mod styles {
    use super::{Colorscheme, OverrideStyle};
    use crate::color;
    use iced::{
        button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider, text_input, toggler,
        Background, Color,
    };
    use iced_aw::{
        style::{self, card, modal},
        tabs,
    };

    pub struct ColorPicker(pub Colorscheme);

    impl style::color_picker::StyleSheet for ColorPicker {
        fn active(&self) -> style::color_picker::Style {
            style::color_picker::Style {
                background: self.0.primary_bg.into(),
                border_radius: 15.0,
                border_width: 1.0,
                border_color: self.0.border,
                bar_border_radius: 5.0,
                bar_border_width: 1.0,
                bar_border_color: self.0.border,
            }
        }

        fn selected(&self) -> style::color_picker::Style {
            self.active()
        }

        fn hovered(&self) -> style::color_picker::Style {
            self.active()
        }

        fn focused(&self) -> style::color_picker::Style {
            style::color_picker::Style {
                border_color: Color::from_rgb(0.5, 0.5, 0.5),
                bar_border_color: Color::from_rgb(0.5, 0.5, 0.5),
                ..self.active()
            }
        }
    }

    /*pub struct NumberInput(pub Colorscheme);

    impl number_input::StyleSheet for NumberInput {
        fn active(&self) -> number_input::Style {
            number_input::Style {
                button_background: Some(self.0.primary_bg.into()),
                icon_color: self.0.text,
            }
        }
    }*/

    pub struct Toggler(pub Colorscheme);

    impl toggler::StyleSheet for Toggler {
        fn active(&self, is_active: bool) -> toggler::Style {
            let mut style = toggler::Style {
                background: self.0.primary_bg,
                foreground: self.0.accent,
                background_border: Some(self.0.secondary_bg),
                foreground_border: None,
            };

            if !is_active {
                style.foreground = self.0.disabled_bg;
            }

            style
        }

        fn hovered(&self, _is_active: bool) -> toggler::Style {
            toggler::Style {
                background: self.0.primary_bg,
                foreground: self.0.accent,
                background_border: Some(self.0.secondary_bg),
                foreground_border: Some(self.0.secondary_bg),
            }
        }
    }

    pub struct TabBar(pub Colorscheme);

    impl tabs::StyleSheet for TabBar {
        fn active(&self, is_selected: bool) -> tabs::Style {
            let tab_label_background = if is_selected {
                Background::Color(self.0.secondary_bg)
            } else {
                Background::Color(self.0.primary_bg)
            };

            let text_color = if is_selected { self.0.accent } else { self.0.text };

            tabs::Style {
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
            let tab_label_background = Background::Color(self.0.secondary_bg);
            let text_color = self.0.accent;

            tabs::Style {
                tab_label_background,
                icon_color: text_color,
                text_color,
                ..self.active(is_selected)
            }
        }
    }

    pub struct Card(pub Colorscheme);

    impl card::StyleSheet for Card {
        fn active(&self) -> card::Style {
            card::Style {
                background: self.0.primary_bg.into(),
                head_background: self.0.secondary_bg.into(),
                border_color: self.0.border,
                foot_background: self.0.primary_bg.into(),
                body_text_color: self.0.text,
                foot_text_color: self.0.text,
                head_text_color: self.0.text,
                close_color: self.0.text,
                border_width: 2.0,
                border_radius: 0.0,
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

    pub struct Container(pub OverrideStyle, pub Colorscheme);

    impl container::StyleSheet for Container {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                background: self.1.primary_bg.into(),
                text_color: Some(self.1.text),
                border_color: self.1.border,
                border_width: 1.5,
                border_radius: 0.0,
            })
        }
    }

    pub struct RoundContainer(pub OverrideStyle, pub Colorscheme);

    impl container::StyleSheet for RoundContainer {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                border_color: self.1.border,
                border_radius: 8.0,
                border_width: 2.0,
                ..Container(self.0, self.1).style()
            })
        }
    }

    pub struct BrightRoundContainer(pub OverrideStyle, pub Colorscheme);

    impl container::StyleSheet for BrightRoundContainer {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                border_color: self.1.secondary_bg,
                border_radius: 8.0,
                border_width: 2.0,
                ..BrightContainer(self.0, self.1).style()
            })
        }
    }

    pub struct BrightContainer(pub OverrideStyle, pub Colorscheme);

    impl container::StyleSheet for BrightContainer {
        fn style(&self) -> container::Style {
            self.0.container(container::Style {
                background: self.1.secondary_bg.into(),
                ..Container(self.0, self.1).style()
            })
        }
    }

    pub struct Radio(pub Colorscheme);

    impl radio::StyleSheet for Radio {
        fn active(&self) -> radio::Style {
            radio::Style {
                background: self.0.secondary_bg.into(),
                dot_color: self.0.accent,
                border_width: 1.0,
                border_color: self.0.accent,
            }
        }

        fn hovered(&self) -> radio::Style {
            radio::Style {
                background: Color {
                    a: 0.5,
                    ..self.0.secondary_bg
                }
                .into(),
                ..self.active()
            }
        }
    }

    pub struct DarkTextInput(pub Colorscheme);

    impl text_input::StyleSheet for DarkTextInput {
        fn active(&self) -> text_input::Style {
            text_input::Style {
                background: self.0.primary_bg.into(),
                ..TextInput(self.0).active()
            }
        }

        fn focused(&self) -> text_input::Style {
            text_input::Style {
                border_width: 3.0,
                border_color: self.0.accent,
                ..self.active()
            }
        }

        fn placeholder_color(&self) -> Color {
            color!(. 0.4, 0.4, 0.4)
        }

        fn value_color(&self) -> Color {
            TextInput(self.0).value_color()
        }

        fn selection_color(&self) -> Color {
            TextInput(self.0).selection_color()
        }

        fn hovered(&self) -> text_input::Style {
            text_input::Style {
                border_width: 2.0,
                border_color: Color {
                    a: 0.5,
                    ..self.0.accent
                },
                ..self.focused()
            }
        }
    }

    pub struct TextInput(pub Colorscheme);

    impl text_input::StyleSheet for TextInput {
        fn active(&self) -> text_input::Style {
            text_input::Style {
                background: self.0.secondary_bg.into(),
                border_radius: 0.0,
                border_width: 1.0,
                border_color: self.0.border,
            }
        }

        fn focused(&self) -> text_input::Style {
            text_input::Style {
                border_width: 3.0,
                border_color: self.0.accent,
                ..self.active()
            }
        }

        fn placeholder_color(&self) -> Color {
            color!(153, 153, 153)
        }

        fn value_color(&self) -> Color {
            self.0.text
        }

        fn selection_color(&self) -> Color {
            self.0.accent
        }

        fn hovered(&self) -> text_input::Style {
            text_input::Style {
                border_width: 2.0,
                border_color: Color {
                    a: 0.5,
                    ..self.0.accent
                },
                ..self.focused()
            }
        }
    }

    pub struct DarkButton(pub OverrideStyle, pub Colorscheme);

    impl button::StyleSheet for DarkButton {
        fn active(&self) -> button::Style {
            self.0.button(button::Style {
                background: self.1.primary_bg.into(),
                border_color: self.1.border,
                border_radius: 0.0,
                border_width: 1.0,
                text_color: self.1.text,
                ..button::Style::default()
            })
        }

        fn hovered(&self) -> button::Style {
            button::Style {
                background: self
                    .0
                    .background_color
                    .map_or(self.1.accent, |c| Color { a: c.a * 0.3, ..c })
                    .into(),
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
                background: self.1.disabled_bg.into(),
                text_color: self.1.disabled_text,
                ..self.active()
            }
        }
    }

    pub struct EmbedButton(pub OverrideStyle, pub Colorscheme);

    impl button::StyleSheet for EmbedButton {
        fn active(&self) -> button::Style {
            DarkButton(self.0, self.1).active()
        }

        fn hovered(&self) -> button::Style {
            DarkButton(self.0, self.1).hovered()
        }

        fn pressed(&self) -> button::Style {
            DarkButton(self.0, self.1).pressed()
        }

        fn disabled(&self) -> button::Style {
            DarkButton(self.0, self.1).active()
        }
    }

    pub struct Button(pub OverrideStyle, pub Colorscheme);

    impl button::StyleSheet for Button {
        fn active(&self) -> button::Style {
            self.0.button(button::Style {
                background: self.1.secondary_bg.into(),
                border_color: self.1.border,
                border_radius: 0.0,
                border_width: 1.0,
                text_color: self.1.text,
                ..button::Style::default()
            })
        }

        fn hovered(&self) -> button::Style {
            button::Style {
                background: self
                    .0
                    .background_color
                    .map_or(self.1.accent, |c| Color { a: c.a * 0.3, ..c })
                    .into(),
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
                background: self.1.disabled_bg.into(),
                text_color: self.1.disabled_text,
                ..self.active()
            }
        }
    }

    pub struct Scrollable(pub Colorscheme);

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
                background: Color {
                    a: 0.5,
                    ..self.0.secondary_bg
                }
                .into(),
                scroller: scrollable::Scroller {
                    color: self.0.accent,
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

    pub struct Slider(pub Colorscheme);

    impl slider::StyleSheet for Slider {
        fn active(&self) -> slider::Style {
            slider::Style {
                rail_colors: (
                    self.0.accent,
                    Color {
                        a: 0.1,
                        ..self.0.accent
                    },
                ),
                handle: slider::Handle {
                    shape: slider::HandleShape::Circle { radius: 9.0 },
                    color: self.0.accent,
                    border_width: 0.0,
                    border_color: Color::TRANSPARENT,
                },
            }
        }

        fn hovered(&self) -> slider::Style {
            let active = self.active();

            slider::Style {
                handle: slider::Handle {
                    color: self.0.accent,
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

    pub struct ProgressBar(pub Colorscheme);

    impl progress_bar::StyleSheet for ProgressBar {
        fn style(&self) -> progress_bar::Style {
            progress_bar::Style {
                background: self.0.secondary_bg.into(),
                bar: self.0.accent.into(),
                border_radius: 10.0,
            }
        }
    }

    pub struct Checkbox(pub Colorscheme);

    impl checkbox::StyleSheet for Checkbox {
        fn active(&self, is_checked: bool) -> checkbox::Style {
            checkbox::Style {
                background: if is_checked { self.0.accent } else { self.0.secondary_bg }.into(),
                checkmark_color: Color::WHITE,
                border_radius: 2.0,
                border_width: 1.0,
                border_color: self.0.accent,
            }
        }

        fn hovered(&self, is_checked: bool) -> checkbox::Style {
            checkbox::Style {
                background: Color {
                    a: 0.8,
                    ..if is_checked { self.0.accent } else { self.0.secondary_bg }
                }
                .into(),
                ..self.active(is_checked)
            }
        }
    }

    pub struct PickList(pub Colorscheme, pub OverrideStyle);

    impl pick_list::StyleSheet for PickList {
        fn menu(&self) -> pick_list::Menu {
            self.1.menu(pick_list::Menu {
                background: self.0.secondary_bg.into(),
                text_color: self.0.text,
                selected_background: self.0.accent.into(),
                selected_text_color: self.0.text,
                border_width: 3.0,
                border_color: Color::TRANSPARENT,
            })
        }

        fn active(&self) -> pick_list::Style {
            self.1.pick_list(pick_list::Style {
                background: self.0.primary_bg.into(),
                text_color: self.0.text,
                border_width: 1.5,
                border_radius: 0.0,
                border_color: self.0.border,
                ..pick_list::Style::default()
            })
        }

        fn hovered(&self) -> pick_list::Style {
            pick_list::Style {
                background: self.0.accent.into(),
                border_color: self.0.accent,
                ..self.active()
            }
        }
    }

    pub struct Rule(pub OverrideStyle, pub Colorscheme);

    impl rule::StyleSheet for Rule {
        fn style(&self) -> rule::Style {
            self.0.rule(rule::Style {
                color: self.1.border,
                width: 3,
                radius: 8.0,
                fill_mode: rule::FillMode::Padded(10),
            })
        }
    }

    pub struct RuleBright(pub OverrideStyle, pub Colorscheme);

    impl rule::StyleSheet for RuleBright {
        fn style(&self) -> rule::Style {
            self.0.rule(rule::Style {
                color: self.1.secondary_bg,
                ..Rule(self.0, self.1).style()
            })
        }
    }
}
