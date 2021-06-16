use iced::{svg::Handle, Svg};
use linemd::{render_as_svg, SvgConfig, SvgViewportDimensions};

use crate::length;

pub fn markdown_svg(md: &str) -> Svg {
    let tokens = linemd::parse(md);
    let svg = render_as_svg(
        tokens,
        SvgConfig::default()
            .font_family("Noto Sans")
            .dimensions(SvgViewportDimensions::OnlyWidth(100)),
    );
    client::tracing::info!("svg: {}", svg);
    Svg::new(Handle::from_memory(svg)).width(length!(=100))
}
