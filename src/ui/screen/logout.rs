use crate::{
    label, label_button, length, space,
    ui::{
        component::*,
        style::{Theme, ERROR_COLOR},
    },
};

pub type Message = bool;

#[derive(Debug, Default)]
pub struct LogoutModal {
    logout_approve_but_state: button::State,
    logout_cancel_but_state: button::State,
}

impl LogoutModal {
    pub fn view(&mut self, theme: Theme, confirmation: bool) -> Element<Message> {
        if confirmation {
            fill_container(label!("Logging out...").size(30))
                .style(theme)
                .into()
        } else {
            let make_button = |state, confirm| {
                let text = if confirm { "Yes" } else { "No" };

                label_button!(state, text)
                    .style(theme)
                    .on_press(confirm)
                    .width(length!(+))
            };

            let logout_confirm_panel = column(
                    vec![
                        label!("Do you want to logout?").into(),
                        label!("This will delete your current session and you will need to login with your password.")
                            .color(ERROR_COLOR)
                            .into(),
                        row(
                            vec![
                                make_button(&mut self.logout_approve_but_state, true).into(),
                                space!(w+).into(),
                                make_button(&mut self.logout_cancel_but_state, false).into(),
                        ])
                        .width(length!(+))
                        .into(),
                    ])
                    .spacing(12);

            row(vec![
                space!(w % 3).into(),
                column(vec![
                    space!(h % 4).into(),
                    fill_container(logout_confirm_panel.width(length!(+)).height(length!(+)))
                        .height(length!(% 3))
                        .style(theme.round())
                        .into(),
                    space!(h % 4).into(),
                ])
                .width(length!(% 3))
                .height(length!(+))
                .into(),
                space!(w % 3).into(),
            ])
            .height(length!(+))
            .width(length!(+))
            .into()
        }
    }
}
