use eframe::egui::{Key, Response, Ui};

pub use anyhow::{anyhow, bail, ensure, Error};
pub use client::error::{ClientError, ClientResult};

pub trait TextInputExt {
    fn did_submit(&self, ui: &Ui) -> bool;
}

impl TextInputExt for Response {
    fn did_submit(&self, ui: &Ui) -> bool {
        self.lost_focus() && ui.input().key_pressed(Key::Enter)
    }
}

pub(crate) use futures::{handle_future, spawn_future};

pub mod futures {
    use eframe::epi::RepaintSignal;
    use std::{
        any::{Any, TypeId},
        collections::HashMap,
        future::Future,
        hash::{BuildHasherDefault, Hasher},
        sync::Arc,
    };
    use tokio::sync::oneshot;

    #[derive(Default)]
    struct IdHasher(u64);

    impl Hasher for IdHasher {
        fn write(&mut self, _: &[u8]) {
            unreachable!("TypeId calls write_u64");
        }

        #[inline]
        fn write_u64(&mut self, id: u64) {
            self.0 = id;
        }

        #[inline]
        fn finish(&self) -> u64 {
            self.0
        }
    }

    type FutureMap = HashMap<TypeId, oneshot::Receiver<AnyItem>, BuildHasherDefault<IdHasher>>;

    type AnyItem = Box<dyn Any + Send + 'static>;

    pub enum FutureProgress<T> {
        NotFound,
        Cancelled,
        InProgress,
        Done(T),
    }

    impl<T> FutureProgress<T> {
        #[inline]
        pub fn is_done(&self) -> bool {
            matches!(self, FutureProgress::Done(_))
        }

        #[inline]
        pub fn is_cancelled(&self) -> bool {
            matches!(self, FutureProgress::Cancelled)
        }

        #[inline]
        pub fn is_in_progress(&self) -> bool {
            matches!(self, FutureProgress::InProgress)
        }

        #[inline]
        pub fn extract(self) -> T {
            match self {
                FutureProgress::Done(value) => value,
                _ => panic!("tried to extract future value but its not done yet"),
            }
        }
    }

    #[derive(Default)]
    pub struct Futures {
        inner: FutureMap,
        rr: Option<Arc<dyn RepaintSignal>>,
    }

    impl Futures {
        pub fn init(&mut self, frame: &eframe::epi::Frame) {
            self.rr = Some(frame.repaint_signal());
        }

        pub fn spawn<Id, Fut, Out>(&mut self, fut: Fut)
        where
            Fut: Future<Output = Out> + Send + 'static,
            Out: Send + 'static,
            Id: 'static,
        {
            let (tx, rx) = oneshot::channel::<AnyItem>();

            let rr = self.rr.clone().expect("futures not initialized yet -- this is a bug");
            tokio::spawn(async move {
                let result = fut.await;
                let item = Box::new(result);
                assert!(!tx.send(item).is_err(), "future output dropped before result was sent");
                rr.request_repaint();
            });

            self.inner.insert(TypeId::of::<Id>(), rx);
        }

        pub fn get<Id, T>(&mut self) -> FutureProgress<T>
        where
            T: 'static,
            Id: 'static,
        {
            let id = TypeId::of::<Id>();

            let progress = if let Some(rx) = self.inner.get_mut(&id) {
                let res = rx.try_recv();
                match res {
                    Ok(item) => FutureProgress::Done(*item.downcast::<T>().expect("value is not of type T")),
                    Err(oneshot::error::TryRecvError::Empty) => FutureProgress::InProgress,
                    Err(oneshot::error::TryRecvError::Closed) => FutureProgress::Cancelled,
                }
            } else {
                FutureProgress::NotFound
            };

            if progress.is_done() || progress.is_cancelled() {
                self.inner.remove(&id);
            }

            progress
        }
    }

    macro_rules! spawn_future {
        ($state:ident, $id:ty, $fut:expr) => {
            $state.futures.spawn::<$id, _, _>($fut);
        };
    }

    macro_rules! handle_future {
        ($state:ident, $id:ty, |$val:ident: $val_ty:ty| $handler:expr) => {
            if let $crate::utils::futures::FutureProgress::Done($val) = $state.futures.get::<$id, $val_ty>() {
                $handler
            }
        };
    }

    pub(crate) use handle_future;
    pub(crate) use spawn_future;
}
