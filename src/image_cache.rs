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

    pub fn get_avatar(&self, id: &FileId) -> Option<(&TextureHandle, [f32; 2])> {
        self.avatar.get(id).map(|(tex, size)| (tex, *size))
    }

    pub fn get_thumbnail(&self, id: &FileId) -> Option<(&TextureHandle, [f32; 2])> {
        self.minithumbnail.get(id).map(|(tex, size)| (tex, *size))
    }

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
    use crate::utils::pool::Pool;

    use client::harmony_rust_sdk::api::{exports::prost::bytes::Bytes, rest::FileId};
    use egui::ColorImage;
    use image_worker::{ArchivedImageLoaded, ImageData, ImageLoaded};
    use js_sys::Uint8Array;
    use std::{cell::RefCell, sync::mpsc::Sender};
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

    struct WorkerPool {
        inner: Pool<WebWorker>,
        channel: RefCell<Option<Sender<LoadedImage>>>,
    }

    // i hate web
    #[allow(unsafe_code)]
    unsafe impl Send for WorkerPool {}
    #[allow(unsafe_code)]
    unsafe impl Sync for WorkerPool {}

    lazy_static::lazy_static! {
        static ref WORKER_POOL: WorkerPool = WorkerPool {
            inner: Pool::new(spawn_worker),
            channel: RefCell::new(None),
        };
    }

    fn spawn_worker() -> WebWorker {
        let worker = WebWorker::new("./image_worker.js").expect("failed to start worker");
        let tx = WORKER_POOL
            .channel
            .borrow()
            .as_ref()
            .expect("worker pool not initialized")
            .clone();

        let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data: Uint8Array = event.data().dyn_into().unwrap_throw();
            let data = data.to_vec();
            #[allow(unsafe_code)]
            let loaded = unsafe { rkyv::archived_root::<ImageLoaded>(&data) };
            let image = LoadedImage::from_archive(loaded);
            let _ = tx.send(image);
        }) as Box<dyn FnMut(_)>);

        worker.set_onmessage(Some(handler.as_ref().unchecked_ref()));

        handler.forget();

        worker
    }

    pub fn set_image_channel(tx: Sender<LoadedImage>) {
        WORKER_POOL.channel.borrow_mut().replace(tx);
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
            .inner
            .get()
            .post_message(&data)
            .expect_throw("failed to decode image");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod op {
    use super::*;

    use std::sync::{mpsc::Sender, Mutex};

    use client::harmony_rust_sdk::api::{exports::prost::bytes::Bytes, rest::FileId};
    use eframe::egui::ColorImage;

    impl LoadedImage {
        pub fn load(data: Bytes, id: FileId, kind: String) -> Self {
            let loaded = image_worker::load_image_logic(data.as_ref(), kind.as_str());
            let image = Image::Color(ColorImage::from_rgba_unmultiplied(
                loaded.dimensions,
                loaded.pixels.as_slice(),
            ));

            Self { image, id, kind }
        }
    }

    lazy_static::lazy_static! {
        static ref CHANNEL: Mutex<Option<Sender<LoadedImage>>> = Mutex::new(None);
    }

    pub fn set_image_channel(tx: Sender<LoadedImage>) {
        CHANNEL.lock().expect("poisoned").replace(tx);
    }

    pub fn decode_image(data: Bytes, id: FileId, kind: String) {
        tokio::task::spawn_blocking(move || {
            let loaded = LoadedImage::load(data, id, kind);
            let _ = CHANNEL
                .lock()
                .expect("poisoned")
                .as_ref()
                .expect("no channel")
                .send(loaded);
        });
    }
}
