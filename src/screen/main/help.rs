use iced_aw::Card;

use crate::{component::*, length, style::*};

pub type Message = bool;

const HELP: &str = include_str!("help.txt");

#[derive(Debug, Default, Clone)]
pub struct HelpModal;

impl HelpModal {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        Container::new(
            Card::new(
                label!("Help").width(length!(=512 - PADDING - SPACING)),
                label!(HELP).width(length!(=512)),
            )
            .style(theme.round())
            .on_close(true),
        )
        .style(theme.round().border_width(0.0))
        .center_x()
        .center_y()
        .into()
    }
}
