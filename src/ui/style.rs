use crate::color;
use iced::{
    button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider,
    text_input, Color,
};

pub const MESSAGE_TIMESTAMP_SIZE: u16 = 13;
pub const MESSAGE_SIZE: u16 = 16;
pub const MESSAGE_SENDER_SIZE: u16 = 19;
pub const DATE_SEPERATOR_SIZE: u16 = 22;

pub const PADDING: u16 = 16;
pub const SPACING: u16 = 4;

pub const ERROR_COLOR: Color = color!(. 1.0, 0.0, 0.0);
pub const SUCCESS_COLOR: Color = color!(. 0.0, 1.0, 0.0);
pub const ALT_COLOR: Color = color!(. 0.5, 0.5, 0.5);

pub const AVATAR_WIDTH: u16 = 32;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    dark: bool,
    secondary: bool,
    round: bool,
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

    pub const fn secondary(mut self) -> Self {
        self.secondary = true;
        self
    }

    pub const fn round(mut self) -> Self {
        self.round = true;
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            dark: true,
            secondary: false,
            round: false,
        }
    }
}

impl From<Theme> for Box<dyn container::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            if theme.secondary {
                dark::BrightContainer.into()
            } else if theme.round {
                dark::RoundContainer.into()
            } else {
                dark::Container.into()
            }
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn radio::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            dark::Radio.into()
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn text_input::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            if theme.secondary {
                dark::DarkTextInput.into()
            } else {
                dark::TextInput.into()
            }
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn button::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            if theme.secondary {
                dark::DarkButton.into()
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
        if theme.dark {
            dark::Scrollable.into()
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn slider::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            dark::Slider.into()
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn progress_bar::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            dark::ProgressBar.into()
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn checkbox::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            dark::Checkbox.into()
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn pick_list::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            dark::PickList.into()
        } else {
            Default::default()
        }
    }
}

impl From<Theme> for Box<dyn rule::StyleSheet> {
    fn from(theme: Theme) -> Self {
        if theme.dark {
            dark::Rule.into()
        } else {
            Default::default()
        }
    }
}

pub struct TransparentButton;

impl From<TransparentButton> for Box<dyn button::StyleSheet> {
    fn from(_: TransparentButton) -> Self {
        dark::TransparentButton.into()
    }
}

mod light {
    use crate::color;
    use iced::{button, Color, Vector};

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
        button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider,
        text_input, Color,
    };

    const DARK_BG: Color = color!(0x36, 0x39, 0x3F);
    const BRIGHT_BG: Color = color!(0x44, 0x48, 0x4F);
    const ACCENT: Color = color!(0x60, 0x64, 0x6B);

    pub struct Container;

    impl container::StyleSheet for Container {
        fn style(&self) -> container::Style {
            container::Style {
                background: DARK_BG.into(),
                text_color: Color::WHITE.into(),
                ..container::Style::default()
            }
        }
    }

    pub struct RoundContainer;

    impl container::StyleSheet for RoundContainer {
        fn style(&self) -> container::Style {
            container::Style {
                border_color: DARK_BG,
                border_radius: 8.0,
                border_width: 1.0,
                ..Container.style()
            }
        }
    }

    pub struct BrightContainer;

    impl container::StyleSheet for BrightContainer {
        fn style(&self) -> container::Style {
            container::Style {
                background: BRIGHT_BG.into(),
                ..Container.style()
            }
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
                background: Color {
                    a: 0.5,
                    ..BRIGHT_BG
                }
                .into(),
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
            Color::WHITE
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
            Color::WHITE
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
                text_color: Color::WHITE,
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
            self.hovered()
        }
    }

    pub struct TransparentButton;

    impl button::StyleSheet for TransparentButton {
        fn active(&self) -> button::Style {
            button::Style {
                background: None,
                border_color: Color::TRANSPARENT,
                border_radius: 0.0,
                border_width: 0.0,
                text_color: Color::WHITE,
                ..button::Style::default()
            }
        }

        fn hovered(&self) -> button::Style {
            self.active()
        }

        fn pressed(&self) -> button::Style {
            self.active()
        }

        fn disabled(&self) -> button::Style {
            self.active()
        }
    }

    pub struct Button;

    impl button::StyleSheet for Button {
        fn active(&self) -> button::Style {
            button::Style {
                background: BRIGHT_BG.into(),
                border_radius: 8.0,
                text_color: Color::WHITE,
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
            self.hovered()
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
                background: Color {
                    a: 0.5,
                    ..BRIGHT_BG
                }
                .into(),
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
                text_color: Color::WHITE,
                selected_background: ACCENT.into(),
                selected_text_color: Color::WHITE,
                border_width: 0.0,
                ..pick_list::Menu::default()
            }
        }

        fn active(&self) -> pick_list::Style {
            pick_list::Style {
                background: DARK_BG.into(),
                text_color: Color::WHITE,
                border_width: 0.0,
                ..pick_list::Style::default()
            }
        }

        fn hovered(&self) -> pick_list::Style {
            pick_list::Style {
                background: ACCENT.into(),
                ..self.active()
            }
        }
    }

    pub struct Rule;

    impl rule::StyleSheet for Rule {
        fn style(&self) -> rule::Style {
            rule::Style {
                color: BRIGHT_BG,
                width: 2,
                radius: 1.0,
                fill_mode: rule::FillMode::Padded(15),
            }
        }
    }
}
