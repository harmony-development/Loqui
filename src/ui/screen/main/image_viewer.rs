use std::path::PathBuf;

use super::super::Message as TopLevelMessage;

use crate::{
    client::content::ImageHandle,
    label_button, length,
    ui::{
        component::*,
        style::{Theme, PADDING, SPACING},
    },
};

use iced::image::{viewer, Viewer};
use iced_aw::Card;

#[derive(Debug, Clone)]
pub enum Message {
    OpenExternal,
    Close,
}

#[derive(Debug, Default, Clone)]
pub struct ImageViewerModal {
    pub image_handle: Option<(ImageHandle, (PathBuf, String))>,
    viewer_state: viewer::State,
    external_but_state: button::State,
    close_but_state: button::State,
}

impl ImageViewerModal {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if let Some((handle, name)) = self
            .image_handle
            .as_ref()
            .map(|(handle, (_, name))| (handle.clone(), name))
        {
            Container::new(
                Card::new(
                    label!(name).width(length!(= 720 - PADDING - SPACING)),
                    Container::new(
                        Viewer::new(&mut self.viewer_state, handle).width(length!(= 720)),
                    )
                    .center_x()
                    .center_y()
                    .width(length!(= 720)),
                )
                .foot(
                    label_button!(&mut self.external_but_state, "Open externally")
                        .style(theme)
                        .on_press(Message::OpenExternal),
                )
                .style(theme.round())
                .on_close(Message::Close),
            )
            .style(theme.round())
            .center_x()
            .center_y()
            .into()
        } else {
            unreachable!()
        }
    }

    pub fn update(&mut self, msg: Message) -> (Command<TopLevelMessage>, bool) {
        let can_go_back;

        match msg {
            Message::OpenExternal => {
                if let Some((_, (path, _))) = self.image_handle.as_ref() {
                    open::that_in_background(path);
                }
                can_go_back = false;
            }
            Message::Close => {
                // clear viewer state
                self.viewer_state = Default::default();
                can_go_back = true;
            }
        }

        (Command::none(), can_go_back)
    }
}
