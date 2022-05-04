use client::AHashMap;
use eframe::egui::{self, ImageData as Image, TextureHandle};

#[derive(Default)]
pub struct ImageCache {
    avatar: AHashMap<String, (TextureHandle, [f32; 2])>,
    minithumbnail: AHashMap<String, (TextureHandle, [f32; 2])>,
    image: AHashMap<String, (TextureHandle, [f32; 2])>,
    bg_image: Option<(TextureHandle, [f32; 2])>,
}

impl ImageCache {
    pub fn add(&mut self, ctx: &egui::Context, image: LoadedImage) {
        match image.kind.as_str() {
            "guild" | "avatar" => add_generic(&mut self.avatar, ctx, image),
            "minithumbnail" => add_generic(&mut self.minithumbnail, ctx, image),
            "bg_image" => {
                let dimensions = image.image.size();
                let texid = ctx.load_texture(image.id.to_string(), image.image);
                self.bg_image = Some((texid, [dimensions[0] as f32, dimensions[1] as f32]));
            }
            _ => add_generic(&mut self.image, ctx, image),
        }
    }

    /// Get an avatar image. Avatars are always 64 x 64
    pub fn get_avatar(&self, id: &str) -> Option<(&TextureHandle, [f32; 2])> {
        self.avatar.get(id).map(|(tex, size)| (tex, *size))
    }

    /// Get a minithumbnail image. Minithumbnails are always blurred
    pub fn get_thumbnail(&self, id: &str) -> Option<(&TextureHandle, [f32; 2])> {
        self.minithumbnail.get(id).map(|(tex, size)| (tex, *size))
    }

    /// Get some image.
    pub fn get_image(&self, id: &str) -> Option<(&TextureHandle, [f32; 2])> {
        self.image.get(id).map(|(tex, size)| (tex, *size))
    }

    pub fn get_bg_image(&self) -> Option<(&TextureHandle, [f32; 2])> {
        self.bg_image.as_ref().map(|(tex, size)| (tex, *size))
    }
}

fn add_generic(map: &mut AHashMap<String, (TextureHandle, [f32; 2])>, ctx: &egui::Context, image: LoadedImage) {
    client::tracing::debug!("decoded image id {}, kind {}", image.id, image.kind);
    let dimensions = image.image.size();
    let texid = ctx.load_texture(image.id.to_string(), image.image);
    map.insert(image.id, (texid, [dimensions[0] as f32, dimensions[1] as f32]));
}

pub struct LoadedImage {
    pub image: Image,
    pub id: String,
    pub kind: String,
}

impl LoadedImage {
    #[inline]
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(target_arch = "wasm32")]
pub mod op {
    use super::*;

    use client::{harmony_rust_sdk::api::exports::prost::bytes::Bytes, tracing};
    use egui::ColorImage;
    use image_worker::{ArchivedImageLoaded, ImageData, ImageLoaded};
    use js_sys::Uint8Array;
    use std::{lazy::SyncOnceCell, sync::mpsc::SyncSender as Sender};
    use wasm_bindgen::{prelude::*, JsCast};
    use web_sys::{window, Blob, BlobPropertyBag, MessageEvent, Url, Worker as WebWorker};

    impl LoadedImage {
        pub fn from_archive(data: &ArchivedImageLoaded) -> Self {
            use std::str::FromStr;

            let dimensions = [data.dimensions[0] as usize, data.dimensions[1] as usize];
            let image = Image::Color(ColorImage::from_rgba_unmultiplied(dimensions, data.pixels.as_slice()));

            Self {
                image,
                id: data.id.to_string(),
                kind: data.kind.as_str().into(),
            }
        }
    }

    struct WorkerPool {
        inner: WebWorker,
    }

    impl WorkerPool {
        fn new(chan: Sender<LoadedImage>, rr: egui::Context) -> Self {
            Self {
                inner: spawn_worker(chan, rr),
            }
        }

        fn get_worker(&self) -> &WebWorker {
            &self.inner
        }
    }

    #[allow(unsafe_code)]
    unsafe impl Sync for WorkerPool {}
    #[allow(unsafe_code)]
    unsafe impl Send for WorkerPool {}

    static WORKER_POOL: SyncOnceCell<WorkerPool> = SyncOnceCell::new();

    fn spawn_worker(tx: Sender<LoadedImage>, rr: egui::Context) -> WebWorker {
        let origin = window()
            .expect("window to be available")
            .location()
            .origin()
            .expect("origin to be available");

        let script = js_sys::Array::new();
        let name = "image_worker";
        script.push(&format!(r#"importScripts("{origin}/{name}.js");wasm_bindgen("{origin}/{name}_bg.wasm");"#).into());

        let blob = Blob::new_with_str_sequence_and_options(&script, BlobPropertyBag::new().type_("text/javascript"))
            .expect("blob creation succeeds");

        let url = Url::create_object_url_with_blob(&blob).expect("url creation succeeds");

        let worker = WebWorker::new(&url).expect("failed to spawn worker");

        let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data: Uint8Array = event.data().dyn_into().unwrap_throw();
            let data = data.to_vec();
            if data.is_empty() {
                return;
            }
            #[allow(unsafe_code)]
            let loaded = unsafe { rkyv::archived_root::<ImageLoaded>(&data) };
            let image = LoadedImage::from_archive(loaded);
            let _ = tx.send(image);
            rr.request_repaint();
        }) as Box<dyn FnMut(_)>);

        worker.set_onmessage(Some(handler.as_ref().unchecked_ref()));

        handler.forget();

        worker
    }

    pub fn set_image_channel(tx: Sender<LoadedImage>, rr: egui::Context) {
        let worker_pool = WorkerPool::new(tx, rr);
        if WORKER_POOL.set(worker_pool).is_err() {
            unreachable!("worker pool must only be init once -- this is a bug");
        }
    }

    pub fn decode_image(data: Bytes, id: String, kind: String) {
        tracing::debug!("sending image (id {id}) for decoding");

        let val = rkyv::to_bytes::<_, 2048>(&ImageData {
            data: data.to_vec(),
            kind,
            id,
        })
        .unwrap()
        .into_vec();

        let data = Uint8Array::new_with_length(val.len() as u32);
        data.copy_from(&val);

        WORKER_POOL
            .get()
            .expect_throw("must be initialized")
            .get_worker()
            .post_message(&data)
            .expect_throw("failed to decode image");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod op {
    use super::*;

    use std::{lazy::SyncOnceCell, sync::mpsc::SyncSender as Sender};

    use client::{harmony_rust_sdk::api::exports::prost::bytes::Bytes, tracing};
    use eframe::egui::ColorImage;

    impl LoadedImage {
        pub fn load(data: Bytes, id: String, kind: String) -> Option<Self> {
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

    static CHANNEL: SyncOnceCell<(Sender<LoadedImage>, egui::Context)> = SyncOnceCell::new();

    /// This should only be called once.
    pub fn set_image_channel(tx: Sender<LoadedImage>, rr: egui::Context) {
        if CHANNEL.set((tx, rr)).is_err() {
            unreachable!("image channel already set -- this is a bug");
        }
    }

    /// Do not call this before calling `set_image_channel`.
    pub fn decode_image(data: Bytes, id: String, kind: String) {
        tracing::debug!("sending image (id {id}) for decoding");

        tokio::task::spawn_blocking(move || {
            let Some(loaded) = LoadedImage::load(data, id, kind) else { return };
            let chan = CHANNEL.get().expect("no image channel set -- this is a bug");
            let _ = chan.0.send(loaded);
            chan.1.request_repaint();
        });
    }
}
