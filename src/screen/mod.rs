use prelude::*;

mod prelude {
    pub use super::Screen as AppScreen;
    pub use crate::{app::State, utils::*};
    pub use eframe::{
        egui::{self, Layout, Ui},
        epi,
    };
}

pub(crate) mod auth;

pub trait Screen: 'static {
    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>, app: &mut State);
}

type BoxedScreen = Box<dyn Screen>;

pub struct ScreenStack {
    stack: Vec<BoxedScreen>,
}

impl ScreenStack {
    pub fn new<S: Screen>(initial_screen: S) -> Self {
        Self {
            // Make sure we can't create a `ScreenStack` without screen to ensure that stack can't be empty [tag:screenstack_cant_start_empty]
            stack: vec![Box::new(initial_screen)],
        }
    }

    #[inline(always)]
    pub fn current(&self) -> &dyn Screen {
        self.stack.last().unwrap().as_ref() // this is safe cause of [ref:screenstack_cant_become_empty] [ref:screenstack_cant_start_empty]
    }

    #[inline(always)]
    pub fn current_mut(&mut self) -> &mut dyn Screen {
        self.stack.last_mut().unwrap().as_mut() // this is safe cause of [ref:screenstack_cant_become_empty] [ref:screenstack_cant_start_empty]
    }

    pub fn clear<S: Screen>(&mut self, screen: S) {
        self.stack.clear();
        self.stack.push(Box::new(screen));
    }

    pub fn push<S: Screen>(&mut self, screen: S) {
        self.stack.push(Box::new(screen));
    }

    pub fn pop(&mut self) -> Option<BoxedScreen> {
        // There must at least one screen remain to ensure [tag:screenstack_cant_become_empty]
        (self.stack.len() > 1).then(|| {
            let screen = self.stack.pop();
            screen.unwrap()
        })
    }
}
