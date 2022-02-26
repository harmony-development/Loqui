use client::{harmony_rust_sdk::api::chat::send_message_request::Attachment, tracing};
use eframe::epi::backend::RepaintSignal;
use std::{
    any::Any,
    cell::RefCell,
    future::Future,
    sync::{mpsc, Arc},
};

#[cfg(not(target_arch = "wasm32"))]
use tokio::spawn;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local as spawn;

type AnyItem = Box<dyn Any + Send + 'static>;

pub struct UploadMessageResult {
    pub guild_id: u64,
    pub channel_id: u64,
    pub attachments: Vec<Attachment>,
}

pub struct Futures {
    queue: RefCell<Vec<AnyItem>>,
    rx: mpsc::Receiver<AnyItem>,
    tx: mpsc::Sender<AnyItem>,
    rr: Option<Arc<dyn RepaintSignal>>,
}

impl Futures {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            rr: None,
            queue: RefCell::new(Vec::new()),
        }
    }

    pub fn init(&mut self, frame: &eframe::epi::Frame) {
        self.rr = Some(frame.lock().repaint_signal.clone());
    }

    pub fn spawn<
        #[cfg(not(target_arch = "wasm32"))] Fut: Future<Output = Out> + Send + 'static,
        #[cfg(target_arch = "wasm32")] Fut: Future<Output = Out> + 'static,
        Out: Send + 'static,
    >(
        &self,
        fut: Fut,
    ) {
        let tx = self.tx.clone();
        let rr = self.rr.clone();
        spawn(async move {
            let result = fut.await;
            let item = Box::new(result);
            if tx.send(item).is_err() {
                tracing::debug!("future output dropped before result was sent");
            }
            if let Some(rr) = rr {
                rr.request_repaint();
            }
        });
    }

    /// Polls the futures for any output(s).
    pub fn poll(&mut self) {
        while let Ok(item) = self.rx.try_recv() {
            self.queue.get_mut().push(item);
        }
    }

    /// Extract all outputs which have the type `T`.
    ///
    /// # Safety
    ///
    /// This is ONLY safe if:
    /// 1. The queue is currently *not* being accessed. This should always be the case,
    /// since `run` takes a mutable refence to self. `Futures` can also not be sent to
    /// other threads because of `RefCell` usage.
    /// 2. The returned iterator isn't kept longer than `self`. This should usually be the case,
    /// if you use `handle_future` macro, which is safe.
    #[allow(unsafe_code)]
    pub unsafe fn get<T>(&self) -> impl Iterator<Item = T>
    where
        T: 'static,
    {
        let queue = self.queue.as_ptr().as_mut().expect("not null");

        queue
            .drain_filter(|item| std::any::TypeId::of::<T>() == item.as_ref().type_id())
            .map(|item| *item.downcast::<T>().expect("cant fail, we compare type ids before"))
    }
}

macro_rules! handle_future {
    ($state:ident, |$val:ident: $val_ty:ty| $handler:expr) => {
        #[allow(unsafe_code)]
        for $val in unsafe { $state.futures.get::<$val_ty>() } {
            $handler
        }
    };
}

macro_rules! spawn_evs {
    ($state:ident, |$ev:ident, $client:ident| $fut:tt) => {{
        let $client = $state.client().clone();
        let _evs = $state.event_sender.clone();
        $state.futures.spawn(async move {
            let _ev = _evs;
            let $ev = &_ev;
            let out = { $fut };
            $crate::utils::ClientResult::Ok(out)
        });
    }};
}

macro_rules! spawn_client_fut {
    ($state:ident, |$client:ident| $fut:expr) => {{
        let $client = $state.client().clone();
        $state.futures.spawn(async move { $fut });
    }};
}

pub(crate) use handle_future;
pub(crate) use spawn_client_fut;
pub(crate) use spawn_evs;
