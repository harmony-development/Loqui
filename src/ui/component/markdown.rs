use std::hash::Hash;

use super::*;

use iced::Font;
use iced_graphics::{Backend, Defaults, Primitive};
use iced_native::{
    layout, mouse, Background, Color, Element, Hasher, Layout, Length, Point, Rectangle, Size,
    Widget,
};
use iced_wgpu::Renderer;
use linemd::Token;

pub struct MarkdownRenderer<Msg> {
    tokens: Vec<Token>,
    url_msg: Option<fn(String) -> Msg>,
    width: Length,
    height: Length,
    text_size: Option<u16>,
    font: Font,
}

impl<Msg> MarkdownRenderer<Msg> {
    pub fn new(text: impl AsRef<str>) -> Self {
        Self {
            tokens: linemd::parse(text),
            url_msg: None,
            width: Length::Shrink,
            height: Length::Shrink,
            text_size: None,
            font: Font::Default,
        }
    }

    pub fn on_url(mut self, msg: fn(String) -> Msg) -> Self {
        self.url_msg = Some(msg);
        self
    }
}

impl<Msg> Widget<Msg, Renderer> for MarkdownRenderer<Msg> {
    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        self.height
    }

    fn layout(&self, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        use iced_native::text::Renderer;

        let limits = limits.width(self.width()).height(self.height());
        let bounds = limits.max();

        let text_size = self.text_size.unwrap_or_else(|| renderer.default_size());

        let mut content = String::default();

        for token in &self.tokens {
            match token {
                Token::Text {
                    value,
                    bold: _,
                    italic: _,
                } => content.push_str(value),
                Token::Code(value) => content.push_str(value),
                Token::CodeFence { code, attrs: _ } => content.push_str(code),
                Token::LineBreak => content.push('\n'),
                _ => {}
            }
        }

        let (width, height) = renderer.measure(&content, text_size, self.font, bounds);

        let size = limits.resolve(Size::new(width, height));
        layout::Node::new(size)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        defaults: &Defaults,
        layout: Layout,
        cursor_position: Point,
        viewport: &Rectangle,
    ) -> (Primitive, mouse::Interaction) {
        use iced_native::text::Renderer;

        let mut content = String::default();

        for token in &self.tokens {
            match token {
                Token::Text {
                    value,
                    bold: _,
                    italic: _,
                } => content.push_str(value),
                Token::Code(value) => content.push_str(value),
                Token::CodeFence { code, attrs: _ } => content.push_str(code),
                Token::LineBreak => content.push('\n'),
                _ => {}
            }
        }

        iced_wgpu::Renderer::draw(
            renderer,
            defaults,
            layout.bounds(),
            &content,
            self.text_size.unwrap_or_else(|| renderer.default_size()),
            self.font,
            None,
            iced::HorizontalAlignment::Center,
            iced::VerticalAlignment::Center,
        )
    }

    fn hash_layout(&self, state: &mut Hasher) {
        self.tokens.hash(state);
        self.url_msg.hash(state);
    }
}

impl<'a, Msg: 'a> Into<Element<'a, Msg, Renderer>> for MarkdownRenderer<Msg> {
    fn into(self) -> Element<'a, Msg, Renderer> {
        Element::new(self)
    }
}
