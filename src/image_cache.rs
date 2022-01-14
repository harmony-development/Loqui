use client::{harmony_rust_sdk::api::rest::FileId, smol_str::SmolStr, AHashMap};
use eframe::{
    egui::TextureId,
    epi::{self, Image},
};

#[derive(Default)]
pub struct ImageCache {
    avatar: AHashMap<FileId, (TextureId, [f32; 2])>,
    minithumbnail: AHashMap<FileId, (TextureId, [f32; 2])>,
    image: AHashMap<FileId, (TextureId, [f32; 2])>,
}

impl ImageCache {
    pub fn add(&mut self, frame: &eframe::epi::Frame, image: LoadedImage) {
        match image.kind.as_str() {
            "guild" | "avatar" => add_generic(&mut self.avatar, frame, image),
            "minithumbnail" => add_generic(&mut self.minithumbnail, frame, image),
            _ => add_generic(&mut self.image, frame, image),
        }
    }

    pub fn get_avatar(&self, id: &FileId) -> Option<(TextureId, [f32; 2])> {
        self.avatar.get(id).copied()
    }

    pub fn get_thumbnail(&self, id: &FileId) -> Option<(TextureId, [f32; 2])> {
        self.minithumbnail.get(id).copied()
    }

    pub fn get_image(&self, id: &FileId) -> Option<(TextureId, [f32; 2])> {
        self.image.get(id).copied()
    }
}

fn add_generic(map: &mut AHashMap<FileId, (TextureId, [f32; 2])>, frame: &epi::Frame, image: LoadedImage) {
    if let Some((tex_id, _)) = map.remove(&image.id) {
        frame.free_texture(tex_id);
    }

    let dimensions = image.image.size;
    let texid = frame.alloc_texture(image.image);
    map.insert(image.id, (texid, [dimensions[0] as f32, dimensions[1] as f32]));
}

pub struct LoadedImage {
    pub image: Image,
    pub id: FileId,
    pub kind: SmolStr,
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

    use client::{
        harmony_rust_sdk::api::{exports::prost::bytes::Bytes, rest::FileId},
        smol_str::SmolStr,
    };
    use image_worker::{ArchivedImageLoaded, ImageData, ImageLoaded};
    use js_sys::Uint8Array;
    use std::sync::mpsc::Sender;
    use wasm_bindgen::{prelude::*, JsCast};
    use web_sys::{MessageEvent, Worker as WebWorker};

    impl LoadedImage {
        pub fn from_archive(data: &ArchivedImageLoaded) -> Self {
            use std::str::FromStr;

            let dimensions = [data.dimensions[0] as usize, data.dimensions[1] as usize];
            let image = Image::from_rgba_unmultiplied(dimensions, data.pixels.as_slice());
            let id = FileId::from_str(data.id.as_str());

            Self {
                image,
                id: id.unwrap(),
                kind: data.kind.as_str().into(),
            }
        }
    }

    struct Worker {
        inner: WebWorker,
    }

    // i hate web
    #[allow(unsafe_code)]
    unsafe impl Send for Worker {}
    #[allow(unsafe_code)]
    unsafe impl Sync for Worker {}

    lazy_static::lazy_static! {
        static ref IMAGE_WORKER: Worker = spawn_worker();
    }

    fn spawn_worker() -> Worker {
        let web = WebWorker::new("./image_worker.js").expect("failed to start worker");
        Worker { inner: web }
    }

    pub fn set_image_channel(tx: Sender<LoadedImage>) {
        let handler = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data: Uint8Array = event.data().dyn_into().unwrap_throw();
            let data = data.to_vec();
            #[allow(unsafe_code)]
            let loaded = unsafe { rkyv::archived_root::<ImageLoaded>(&data) };
            let image = LoadedImage::from_archive(loaded);
            let _ = tx.send(image);
        }) as Box<dyn FnMut(_)>);

        IMAGE_WORKER.inner.set_onmessage(Some(handler.as_ref().unchecked_ref()));

        handler.forget();
    }

    pub fn decode_image(data: Bytes, id: FileId, kind: SmolStr) {
        let val = rkyv::to_bytes::<_, 2048>(&ImageData {
            data: data.to_vec(),
            kind: kind.to_string(),
            id: id.into(),
        })
        .unwrap()
        .into_vec();

        let data = Uint8Array::new_with_length(val.len() as u32);
        data.copy_from(&val);

        IMAGE_WORKER
            .inner
            .post_message(&data)
            .expect_throw("failed to decode image");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod op {
    use super::*;

    use std::sync::{mpsc::Sender, Mutex};

    use client::{
        harmony_rust_sdk::api::{exports::prost::bytes::Bytes, rest::FileId},
        smol_str::SmolStr,
    };
    use once_cell::sync::OnceCell;

    impl LoadedImage {
        pub fn load(data: Bytes, id: FileId, kind: SmolStr) -> Self {
            let loaded = image_worker::load_image_logic(data.as_ref(), kind.as_str());
            let image = Image::from_rgba_unmultiplied(loaded.dimensions, loaded.pixels.as_slice());

            Self { image, id, kind }
        }
    }

    lazy_static::lazy_static! {
        static ref CHANNEL: Mutex<OnceCell<Sender<LoadedImage>>> = Mutex::new(OnceCell::new());
    }

    pub fn set_image_channel(tx: Sender<LoadedImage>) {
        CHANNEL.lock().unwrap().set(tx).unwrap();
    }

    pub fn decode_image(data: Bytes, id: FileId, kind: SmolStr) {
        tokio::task::spawn_blocking(move || {
            let loaded = LoadedImage::load(data, id, kind);
            let _ = CHANNEL.lock().unwrap().get().expect("no channel").send(loaded);
        });
    }
}
