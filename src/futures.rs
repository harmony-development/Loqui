use client::tracing;
use eframe::epi::backend::RepaintSignal;
use std::{
    any::Any,
    future::Future,
    sync::{mpsc, Arc},
};

#[cfg(not(target_arch = "wasm32"))]
use tokio::spawn;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local as spawn;

type AnyItem = Box<dyn Any + Send + 'static>;

pub struct Futures {
    queue: Vec<AnyItem>,
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
            queue: Vec::new(),
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

    pub fn run(&mut self) {
        while let Ok(item) = self.rx.try_recv() {
            self.queue.push(item);
        }
    }

    pub fn get<T>(&mut self) -> Option<T>
    where
        T: 'static,
    {
        let mut to_get = None;
        for (index, item) in self.queue.iter().enumerate() {
            if std::any::TypeId::of::<T>() == item.as_ref().type_id() {
                to_get = Some(index);
            }
        }
        to_get.map(|index| {
            let item = self.queue.remove(index);
            *item.downcast::<T>().expect("cant fail, we compare type ids before")
        })
    }
}

macro_rules! spawn_future {
    ($state:ident, $fut:expr) => {
        $state.futures.spawn::<_, _>($fut);
    };
}

macro_rules! handle_future {
    ($state:ident, |$val:ident: $val_ty:ty| $handler:expr) => {
        while let Some($val) = $state.futures.get::<$val_ty>() {
            $handler
        }
    };
}

macro_rules! spawn_evs {
    ($state:ident, |$ev:ident, $client:ident| $fut:tt) => {{
        let $client = $state.client().clone();
        $state.futures.spawn(async move {
            let mut _evs = Vec::new();
            let $ev = &mut _evs;
            {
                $fut
            }
            $crate::utils::ClientResult::Ok(_evs)
        });
    }};
}

macro_rules! spawn_client_fut {
    ($state:ident, |$client:ident| $fut:tt) => {{
        let $client = $state.client().clone();
        $state.futures.spawn(async move {
            let res = $fut;
            $crate::utils::ClientResult::Ok(res)
        });
    }};
}

pub(crate) use handle_future;
pub(crate) use spawn_client_fut;
pub(crate) use spawn_evs;
pub(crate) use spawn_future;
