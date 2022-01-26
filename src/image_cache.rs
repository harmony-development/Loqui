use client::{harmony_rust_sdk::api::rest::FileId, AHashMap};
use eframe::egui::{self, ImageData as Image, TextureHandle};

#[derive(Default)]
pub struct ImageCache {
    avatar: AHashMap<FileId, (TextureHandle, [f32; 2])>,
    minithumbnail: AHashMap<FileId, (TextureHandle, [f32; 2])>,
    image: AHashMap<FileId, (TextureHandle, [f32; 2])>,
}

impl ImageCache {
    pub fn add(&mut self, ctx: &egui::Context, image: LoadedImage) {
        match image.kind.as_str() {
            "guild" | "avatar" => add_generic(&mut self.avatar, ctx, image),
            "minithumbnail" => add_generic(&mut self.minithumbnail, ctx, image),
            _ => add_generic(&mut self.image, ctx, image),
        }
    }

    /// Get an avatar image. Avatars are always 64 x 64
    pub fn get_avatar(&self, id: &FileId) -> Option<(&TextureHandle, [f32; 2])> {
        self.avatar.get(id).map(|(tex, size)| (tex, *size))
    }

    /// Get a minithumbnail image. Minithumbnails are always blurred
    pub fn get_thumbnail(&self, id: &FileId) -> Option<(&TextureHandle, [f32; 2])> {
        self.minithumbnail.get(id).map(|(tex, size)| (tex, *size))
    }

    /// Get some image.
    pub fn get_image(&self, id: &FileId) -> Option<(&TextureHandle, [f32; 2])> {
        self.image.get(id).map(|(tex, size)| (tex, *size))
    }
}

fn add_generic(map: &mut AHashMap<FileId, (TextureHandle, [f32; 2])>, ctx: &egui::Context, image: LoadedImage) {
    let dimensions = image.image.size();
    let texid = ctx.load_texture(image.id.to_string(), image.image);
    map.insert(image.id, (texid, [dimensions[0] as f32, dimensions[1] as f32]));
}

pub struct LoadedImage {
    pub image: Image,
    pub id: FileId,
    pub kind: String,
}

impl LoadedImage {
    #[inline]
    pub fn id(&self) -> &FileId {
        &self.id
    }
}

#[cfg(target_arch = "wasm32")]
pub mod op {
    use super::*;

    use client::harmony_rust_sdk::api::{exports::prost::bytes::Bytes, rest::FileId};
    use eframe::epi::backend::RepaintSignal;
    use egui::ColorImage;
    use image_worker::{ArchivedImageLoaded, ImageData, ImageLoaded};
    use js_sys::Uint8Array;
    use std::{
        lazy::SyncOnceCell,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::SyncSender as Sender,
            Arc, RwLock,
        },
    };
    use wasm_bindgen::{prelude::*, JsCast};
    use web_sys::{MessageEvent, Worker as WebWorker};

    impl LoadedImage {
        pub fn from_archive(data: &ArchivedImageLoaded) -> Self {
            use std::str::FromStr;

            let dimensions = [data.dimensions[0] as usize, data.dimensions[1] as usize];
            let image = Image::Color(ColorImage::from_rgba_unmultiplied(dimensions, data.pixels.as_slice()));
            let id = FileId::from_str(data.id.as_str());

            Self {
                image,
                id: id.unwrap(),
                kind: data.kind.as_str().into(),
            }
        }
    }

    const INITIAL_WORKER_COUNT: usize = 4;

    struct Worker {
        inner: WebWorker,
    }

    #[allow(unsafe_code)]
    unsafe impl Sync for Worker {}
    #[allow(unsafe_code)]
    unsafe impl Send for Worker {}

    struct WorkerPool {
        inner: RwLock<Vec<(Arc<AtomicBool>, Arc<Worker>)>>,
        tx: Sender<LoadedImage>,
        rr: Arc<dyn RepaintSignal>,
    }

    impl WorkerPool {
        fn new(chan: Sender<LoadedImage>, rr: Arc<dyn RepaintSignal>) -> Self {
            Self {
                inner: RwLock::new(
                    std::iter::repeat_with(|| {
                        let is_usable = Arc::new(AtomicBool::new(true));
                        let inner = spawn_worker(chan.clone(), rr.clone(), is_usable.clone());
                        (is_usable, Arc::new(Worker { inner }))
                    })
                    .take(INITIAL_WORKER_COUNT)
                    .collect(),
                ),
                tx: chan,
                rr,
            }
        }

        fn get_worker(&self) -> Arc<Worker> {
            #[allow(unsafe_code)]
            unsafe {
                let mut worker_index = None;

                for (index, (is_usable, _)) in self.inner.read().unwrap_unchecked().iter().enumerate() {
                    if is_usable.load(Ordering::SeqCst) {
                        worker_index = Some(index);
                    }
                }

                if worker_index.is_none() {
                    let is_usable = Arc::new(AtomicBool::new(true));
                    let inner = spawn_worker(self.tx.clone(), self.rr.clone(), is_usable.clone());
                    let mut workers = self.inner.write().unwrap_unchecked();
                    workers.push((is_usable, Arc::new(Worker { inner })));
                    worker_index = Some(workers.len() - 1);
                }

                let worker_index = worker_index.unwrap_unchecked();

                let workers = self.inner.read().unwrap_unchecked();
                let (is_usable, worker) = workers.get(worker_index).unwrap_unchecked();

                is_usable.store(false, Ordering::SeqCst);

                worker.clone()
            }
        }
    }

    static WORKER_POOL: SyncOnceCell<WorkerPool> = SyncOnceCell::new();

    fn spawn_worker(tx: Sender<LoadedImage>, rr: Arc<dyn RepaintSignal>, is_usable: Arc<AtomicBool>) -> WebWorker {
        let worker = WebWorker::new("./image_worker.js").expect("failed to start worker");

        let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data: Uint8Array = event.data().dyn_into().unwrap_throw();
            let data = data.to_vec();
            if data.is_empty() {
                is_usable.store(true, Ordering::SeqCst);
                return;
            }
            #[allow(unsafe_code)]
            let loaded = unsafe { rkyv::archived_root::<ImageLoaded>(&data) };
            let image = LoadedImage::from_archive(loaded);
            let _ = tx.send(image);
            rr.request_repaint();
            is_usable.store(true, Ordering::SeqCst);
        }) as Box<dyn FnMut(_)>);

        worker.set_onmessage(Some(handler.as_ref().unchecked_ref()));

        handler.forget();

        worker
    }

    pub fn set_image_channel(tx: Sender<LoadedImage>, rr: Arc<dyn RepaintSignal>) {
        let worker_pool = WorkerPool::new(tx, rr);
        if WORKER_POOL.set(worker_pool).is_err() {
            unreachable!("worker pool must only be init once -- this is a bug");
        }
    }

    pub fn decode_image(data: Bytes, id: FileId, kind: String) {
        let val = rkyv::to_bytes::<_, 2048>(&ImageData {
            data: data.to_vec(),
            kind,
            id: id.into(),
        })
        .unwrap()
        .into_vec();

        let data = Uint8Array::new_with_length(val.len() as u32);
        data.copy_from(&val);

        WORKER_POOL
            .get()
            .expect("must be initialized")
            .get_worker()
            .inner
            .post_message(&data)
            .expect_throw("failed to decode image");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod op {
    use super::*;

    use std::{
        lazy::SyncOnceCell,
        sync::{mpsc::SyncSender as Sender, Arc},
    };

    use client::{
        harmony_rust_sdk::api::{exports::prost::bytes::Bytes, rest::FileId},
        tracing,
    };
    use eframe::{egui::ColorImage, epi::backend::RepaintSignal};

    impl LoadedImage {
        pub fn load(data: Bytes, id: FileId, kind: String) -> Option<Self> {
            let Some(loaded) = image_worker::load_image_logic(data.as_ref(), kind.as_str()) else {
                tracing::error!(
                    "could not load an image (id {}); most likely unsupported format",
                    id
                );
                return None;
            };
            let image = Image::Color(ColorImage::from_rgba_unmultiplied(
                loaded.dimensions,
                loaded.pixels.as_slice(),
            ));

            Some(Self { image, id, kind })
        }
    }

    static CHANNEL: SyncOnceCell<(Sender<LoadedImage>, Arc<dyn RepaintSignal>)> = SyncOnceCell::new();

    /// This should only be called once.
    pub fn set_image_channel(tx: Sender<LoadedImage>, rr: Arc<dyn RepaintSignal>) {
        if CHANNEL.set((tx, rr)).is_err() {
            unreachable!("image channel already set -- this is a bug");
        }
    }

    /// Do not call this before calling `set_image_channel`.
    pub fn decode_image(data: Bytes, id: FileId, kind: String) {
        tokio::task::spawn_blocking(move || {
            let Some(loaded) = LoadedImage::load(data, id, kind) else { return };
            let chan = CHANNEL.get().expect("no image channel set -- this is a bug");
            let _ = chan.0.send(loaded);
            chan.1.request_repaint();
        });
    }
}
