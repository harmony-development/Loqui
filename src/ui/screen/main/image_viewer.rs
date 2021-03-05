use std::path::PathBuf;

use crate::{
    client::content::ImageHandle,
    label_button, length, space,
    ui::{component::*, style::Theme},
};

use iced::image::{viewer, Viewer};

#[derive(Debug, Clone)]
pub enum Message {
    OpenExternal,
    Close,
}

#[derive(Debug, Default, Clone)]
pub struct ImageViewerModal {
    pub image_handle: Option<(ImageHandle, PathBuf)>,
    viewer_state: viewer::State,
    external_but_state: button::State,
    close_but_state: button::State,
}

impl ImageViewerModal {
    pub fn view(&mut self, theme: Theme) -> Element<Message> {
        if let Some(handle) = self.image_handle.as_ref().map(|(handle, _)| handle.clone()) {
            column(vec![
                row(vec![
                    space!(w % 1).into(),
                    Viewer::new(&mut self.viewer_state, handle)
                        .height(length!(%8))
                        .width(length!(%8))
                        .into(),
                    space!(w % 1).into(),
                ])
                .height(length!(%26))
                .into(),
                row(vec![
                    label_button!(&mut self.external_but_state, "Open externally")
                        .height(length!(+))
                        .style(theme)
                        .on_press(Message::OpenExternal)
                        .into(),
                    space!(w+).into(),
                    label_button!(&mut self.close_but_state, "Close")
                        .height(length!(+))
                        .style(theme)
                        .on_press(Message::Close)
                        .into(),
                ])
                .width(length!(+))
                .height(length!(%2))
                .into(),
            ])
            .into()
        } else {
            fill_container(space!(w+)).into()
        }
    }

    pub fn update(&mut self, msg: Message) -> (Command<super::super::Message>, bool) {
        let can_go_back;

        match msg {
            Message::OpenExternal => {
                if let Some((_, path)) = self.image_handle.as_ref() {
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
