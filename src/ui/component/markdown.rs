use html_parser::{Dom, DomVariant, ElementVariant, Node};

use std::hash::Hash;

use iced_graphics::{backend::Text, Backend, Defaults, Primitive};
use iced_native::{
    layout, mouse, Background, Color, Element, Hasher, Layout, Length, Point, Rectangle, Size,
    Widget,
};

pub struct Markdown<Renderer: self::Renderer> {
    dom: Dom,
    font: Renderer::Font,
    width: Length,
    height: Length,
}

impl<Renderer: self::Renderer> Markdown<Renderer> {
    pub fn new(md: impl AsRef<str>) -> Self {
        let html = comrak::markdown_to_html(md.as_ref(), &comrak::ComrakOptions::default());
        let dom = Dom::parse(&html).unwrap();

        Self {
            dom,
            font: Default::default(),
            width: Length::Shrink,
            height: Length::Shrink,
        }
    }
}

impl<Message, Renderer> Widget<Message, Renderer> for Markdown<Renderer>
where
    Renderer: self::Renderer,
{
    fn width(&self) -> Length {
        Length::Shrink
    }

    fn height(&self) -> Length {
        Length::Shrink
    }

    fn layout(&self, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        let limits = limits.width(self.width).height(self.height);
        let bounds = limits.max();
        let size = limits.resolve(renderer.measure(&self.dom, self.font, bounds));

        layout::Node::new(size)
    }

    fn hash_layout(&self, state: &mut Hasher) {
        struct Marker;
        std::any::TypeId::of::<Marker>().hash(state);

        fn hash_node(node: &Node, state: &mut Hasher) {
            match node {
                Node::Element(el) => {
                    el.classes.hash(state);
                    el.id.hash(state);
                    for (name, val) in &el.attributes {
                        name.hash(state);
                        val.hash(state);
                    }
                    for node in &el.children {
                        hash_node(node, state);
                    }
                }
                Node::Text(text) => {
                    text.hash(state);
                }
                _ => {}
            }
        }

        for node in &self.dom.children {
            hash_node(node, state);
        }

        self.width.hash(state);
        self.height.hash(state);
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        defaults: &Renderer::Defaults,
        layout: Layout<'_>,
        cursor_position: Point,
        viewport: &Rectangle,
    ) -> Renderer::Output {
        renderer.draw(
            &self.dom,
            self.font,
            defaults,
            layout,
            cursor_position,
            viewport,
        )
    }
}

pub trait Renderer: iced_native::Renderer {
    type Font: Default + Copy;

    fn calculate_node_size(&self, node: &Node, font: Self::Font, bounds: Size) -> (f32, f32);

    fn measure(&self, dom: &Dom, font: Self::Font, bounds: Size) -> Size;

    fn draw_node(
        &mut self,
        node: &Node,
        font: Self::Font,
        defaults: &Defaults,
        layout: Layout,
        cursor_position: Point,
        current_position: Point,
    ) -> Primitive;

    fn draw(
        &mut self,
        dom: &Dom,
        font: Self::Font,
        defaults: &Defaults,
        layout: Layout,
        cursor_position: Point,
        viewport: &Rectangle,
    ) -> Self::Output;
}

impl Renderer for iced_wgpu::Renderer {
    type Font = iced_graphics::Font;

    fn calculate_node_size(&self, node: &Node, font: Self::Font, bounds: Size) -> (f32, f32) {
        let (mut width, mut height) = (0.0_f32, 0.0_f32);

        match node {
            Node::Element(el) => {
                for node in &el.children {
                    let (node_width, node_height) = self.calculate_node_size(node, font, bounds);
                    width = width.max(node_width);
                    height += node_height;
                }
            }
            Node::Text(text) => {
                let (node_width, node_height) = self.backend().measure(
                    text.as_str(),
                    self.backend().default_size() as f32,
                    font,
                    bounds,
                );

                width = node_width;
                height = node_height;
            }
            Node::Comment(_) => {}
        }

        (width, height)
    }

    fn measure(&self, dom: &Dom, font: Self::Font, bounds: Size) -> Size {
        let (mut width, mut height) = (0.0_f32, 0.0_f32);

        for node in &dom.children {
            let (node_width, node_height) = self.calculate_node_size(node, font, bounds);
            width = width.max(node_width);
            height += node_height;
        }

        Size::new(width, height)
    }

    fn draw_node(
        &mut self,
        node: &Node,
        font: Self::Font,
        defaults: &Defaults,
        layout: Layout,
        cursor_position: Point,
        current_position: Point,
    ) -> Primitive {
        match node {
            Node::Element(el) => {
                let primitives = el
                    .children
                    .iter()
                    .map(|node| {
                        self.draw_node(node, font, defaults, layout, cursor_position, current_position)
                    })
                    .collect();
                Primitive::Group { primitives }
            }
            Node::Text(text) => {
                use iced_native::text::Renderer;

                let (width, height)
                let horizontal_alignment = iced_native::HorizontalAlignment::Center;
                let vertical_alignment = iced_native::VerticalAlignment::Center;

                let x = match horizontal_alignment {
                    iced_native::HorizontalAlignment::Left => bounds.x,
                    iced_native::HorizontalAlignment::Center => bounds.center_x(),
                    iced_native::HorizontalAlignment::Right => bounds.x + bounds.width,
                };

                let y = match vertical_alignment {
                    iced_native::VerticalAlignment::Top => bounds.y,
                    iced_native::VerticalAlignment::Center => bounds.center_y(),
                    iced_native::VerticalAlignment::Bottom => bounds.y + bounds.height,
                };

                Primitive::Text {
                    content: text.clone(),
                    size: f32::from(self.default_size()),
                    bounds: Rectangle { x, y, ..bounds },
                    color: defaults.text.color,
                    font,
                    horizontal_alignment,
                    vertical_alignment,
                }
            }
            Node::Comment(_) => Primitive::None,
        }
    }

    fn draw(
        &mut self,
        dom: &Dom,
        font: Self::Font,
        defaults: &Defaults,
        layout: Layout,
        cursor_position: Point,
        viewport: &Rectangle,
    ) -> Self::Output {
        let primitives = dom
            .children
            .iter()
            .map(|node| self.draw_node(defaults, node, font, bounds))
            .collect();
        let mouse = mouse::Interaction::default();

        (Primitive::Group { primitives }, mouse)
    }
}
