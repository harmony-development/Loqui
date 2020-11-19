use iced::{
    button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider,
    text_input, Color,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Light, Theme::Dark];
    const SENDER_COLORS_DARK: [Color; 8] = [
        Color::from_rgb(
            0x6d as f32 / 255.0,
            0xdd as f32 / 255.0,
            0x18 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xfc as f32 / 255.0,
            0xd2 as f32 / 255.0,
            0x00 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xcc as f32 / 255.0,
            0xf9 as f32 / 255.0,
            0xff as f32 / 255.0,
        ),
        Color::from_rgb(
            0x3d as f32 / 255.0,
            0xdb as f32 / 255.0,
            0x8c as f32 / 255.0,
        ),
        Color::from_rgb(
            0xdd as f32 / 255.0,
            0x6a as f32 / 255.0,
            0x35 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xe2 as f32 / 255.0,
            0x22 as f32 / 255.0,
            0x45 as f32 / 255.0,
        ),
        Color::from_rgb(
            0x09 as f32 / 255.0,
            0xe5 as f32 / 255.0,
            0x38 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xd1 as f32 / 255.0,
            0x32 as f32 / 255.0,
            0x71 as f32 / 255.0,
        ),
    ];
    const SENDER_COLORS_LIGHT: [Color; 8] = [
        Color::from_rgb(
            0x6d as f32 / 255.0,
            0xdd as f32 / 255.0,
            0x18 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xfc as f32 / 255.0,
            0xd2 as f32 / 255.0,
            0x00 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xcc as f32 / 255.0,
            0xf9 as f32 / 255.0,
            0xff as f32 / 255.0,
        ),
        Color::from_rgb(
            0x3d as f32 / 255.0,
            0xdb as f32 / 255.0,
            0x8c as f32 / 255.0,
        ),
        Color::from_rgb(
            0xdd as f32 / 255.0,
            0x6a as f32 / 255.0,
            0x35 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xe2 as f32 / 255.0,
            0x22 as f32 / 255.0,
            0x45 as f32 / 255.0,
        ),
        Color::from_rgb(
            0x09 as f32 / 255.0,
            0xe5 as f32 / 255.0,
            0x38 as f32 / 255.0,
        ),
        Color::from_rgb(
            0xd1 as f32 / 255.0,
            0x32 as f32 / 255.0,
            0x71 as f32 / 255.0,
        ),
    ];

    pub fn calculate_sender_color(&self, name_len: usize) -> Color {
        match self {
            Theme::Light => Theme::SENDER_COLORS_LIGHT[name_len % Theme::SENDER_COLORS_LIGHT.len()],
            Theme::Dark => Theme::SENDER_COLORS_DARK[name_len % Theme::SENDER_COLORS_DARK.len()],
        }
    }
}

impl Default for Theme {
    fn default() -> Theme {
        Theme::Dark
    }
}

impl From<Theme> for Box<dyn container::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::Container.into(),
        }
    }
}

impl From<Theme> for Box<dyn radio::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::Radio.into(),
        }
    }
}

impl From<Theme> for Box<dyn text_input::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::TextInput.into(),
        }
    }
}

impl From<Theme> for Box<dyn button::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => light::Button.into(),
            Theme::Dark => dark::Button.into(),
        }
    }
}

impl From<Theme> for Box<dyn scrollable::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::Scrollable.into(),
        }
    }
}

impl From<Theme> for Box<dyn slider::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::Slider.into(),
        }
    }
}

impl From<Theme> for Box<dyn progress_bar::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::ProgressBar.into(),
        }
    }
}

impl From<Theme> for Box<dyn checkbox::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::Checkbox.into(),
        }
    }
}

impl From<Theme> for Box<dyn pick_list::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::PickList.into(),
        }
    }
}

impl From<Theme> for Box<dyn rule::StyleSheet> {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => Default::default(),
            Theme::Dark => dark::Rule.into(),
        }
    }
}

pub struct BrightContainer;

impl From<BrightContainer> for Box<dyn container::StyleSheet> {
    fn from(_: BrightContainer) -> Self {
        dark::BrightContainer.into()
    }
}

pub struct RoundContainer;

impl From<RoundContainer> for Box<dyn container::StyleSheet> {
    fn from(_: RoundContainer) -> Self {
        dark::RoundContainer.into()
    }
}

pub struct DarkTextInput;

impl From<DarkTextInput> for Box<dyn text_input::StyleSheet> {
    fn from(_: DarkTextInput) -> Self {
        dark::DarkTextInput.into()
    }
}

pub struct DarkButton;

impl From<DarkButton> for Box<dyn button::StyleSheet> {
    fn from(_: DarkButton) -> Self {
        dark::DarkButton.into()
    }
}

pub struct TransparentButton;

impl From<TransparentButton> for Box<dyn button::StyleSheet> {
    fn from(_: TransparentButton) -> Self {
        dark::TransparentButton.into()
    }
}

mod light {
    use iced::{button, Color, Vector};

    pub struct Button;

    impl button::StyleSheet for Button {
        fn active(&self) -> button::Style {
            button::Style {
                background: Color::from_rgb(0.11, 0.42, 0.87).into(),
                border_radius: 12,
                shadow_offset: Vector::new(1.0, 1.0),
                text_color: Color::from_rgb8(0xEE, 0xEE, 0xEE),
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
    use iced::{
        button, checkbox, container, pick_list, progress_bar, radio, rule, scrollable, slider,
        text_input, Color,
    };

    const DARK_BG: Color = Color::from_rgb(
        0x36 as f32 / 255.0,
        0x39 as f32 / 255.0,
        0x3F as f32 / 255.0,
    );

    const BRIGHT_BG: Color = Color::from_rgb(
        0x44 as f32 / 255.0,
        0x48 as f32 / 255.0,
        0x4F as f32 / 255.0,
    );

    const ACCENT: Color = Color::from_rgb(
        0x60 as f32 / 255.0,
        0x64 as f32 / 255.0,
        0x6B as f32 / 255.0,
    );

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
                border_radius: 8,
                border_width: 1,
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
                border_width: 1,
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
                border_width: 3,
                border_color: ACCENT,
                ..self.active()
            }
        }

        fn placeholder_color(&self) -> Color {
            Color::from_rgb(0.4, 0.4, 0.4)
        }

        fn value_color(&self) -> Color {
            Color::WHITE
        }

        fn selection_color(&self) -> Color {
            ACCENT
        }

        fn hovered(&self) -> text_input::Style {
            text_input::Style {
                border_width: 2,
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
                border_radius: 8,
                border_width: 0,
                border_color: ACCENT,
            }
        }

        fn focused(&self) -> text_input::Style {
            text_input::Style {
                border_width: 3,
                border_color: ACCENT,
                ..self.active()
            }
        }

        fn placeholder_color(&self) -> Color {
            Color::from_rgb(0.6, 0.6, 0.6)
        }

        fn value_color(&self) -> Color {
            Color::WHITE
        }

        fn selection_color(&self) -> Color {
            ACCENT
        }

        fn hovered(&self) -> text_input::Style {
            text_input::Style {
                border_width: 2,
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
                border_radius: 8,
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
                border_width: 1,
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
                border_radius: 0,
                border_width: 0,
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
                border_radius: 8,
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
                border_width: 1,
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
                border_radius: 2,
                border_width: 0,
                border_color: Color::TRANSPARENT,
                scroller: scrollable::Scroller {
                    color: Color::TRANSPARENT,
                    border_radius: 2,
                    border_width: 0,
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
                    color: Color::from_rgb(0.85, 0.85, 0.85),
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
                    shape: slider::HandleShape::Circle { radius: 9 },
                    color: ACCENT,
                    border_width: 0,
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
                    color: Color::from_rgb(0.85, 0.85, 0.85),
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
                border_radius: 10,
            }
        }
    }

    pub struct Checkbox;

    impl checkbox::StyleSheet for Checkbox {
        fn active(&self, is_checked: bool) -> checkbox::Style {
            checkbox::Style {
                background: if is_checked { ACCENT } else { BRIGHT_BG }.into(),
                checkmark_color: Color::WHITE,
                border_radius: 2,
                border_width: 1,
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
                border_width: 0,
                ..pick_list::Menu::default()
            }
        }

        fn active(&self) -> pick_list::Style {
            pick_list::Style {
                background: DARK_BG.into(),
                text_color: Color::WHITE,
                border_width: 0,
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
                radius: 1,
                fill_mode: rule::FillMode::Padded(15),
            }
        }
    }
}
