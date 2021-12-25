use std::convert::identity;

use client::{
    harmony_rust_sdk::api::{exports::prost::bytes::Bytes, rest::FileId},
    smol_str::SmolStr,
    AHashMap,
};
use eframe::egui::{self, Color32, TextureId};
use image::DynamicImage;

#[derive(Default)]
pub struct ImageCache {
    avatar: AHashMap<FileId, (TextureId, [f32; 2])>,
    minithumbnail: AHashMap<FileId, (TextureId, [f32; 2])>,
    image: AHashMap<FileId, (TextureId, [f32; 2])>,
}

impl ImageCache {
    pub fn add(&mut self, frame: &mut eframe::epi::Frame<'_>, image: LoadedImage) {
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

fn add_generic(
    map: &mut AHashMap<FileId, (TextureId, [f32; 2])>,
    frame: &mut eframe::epi::Frame<'_>,
    image: LoadedImage,
) {
    if let Some((id, _)) = map.remove(&image.id) {
        frame.tex_allocator().free(id);
    }

    let id = frame
        .tex_allocator()
        .alloc_srgba_premultiplied(image.dimensions, &image.pixels);
    map.insert(image.id, (id, [image.dimensions.0 as f32, image.dimensions.1 as f32]));
}

pub struct LoadedImage {
    pixels: Vec<Color32>,
    dimensions: (usize, usize),
    id: FileId,
    kind: SmolStr,
}

impl LoadedImage {
    #[inline]
    pub fn id(&self) -> &FileId {
        &self.id
    }

    pub async fn load(data: Bytes, id: FileId, kind: SmolStr) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        use tokio::task::spawn_blocking;
        #[cfg(target_arch = "wasm32")]
        let spawn_blocking = |f| wasm_bindgen_futures::spawn_local(async move { f() });

        let modify = match kind.as_str() {
            "minithumbnail" => |image: DynamicImage| image.blur(4.0),
            "guild" | "avatar" => |image: DynamicImage| image.resize(64, 64, image::imageops::FilterType::Lanczos3),
            _ => identity,
        };
        spawn_blocking(move || Self::load_inner(data, id, kind, modify))
            .await
            .unwrap()
    }

    fn load_inner(data: Bytes, id: FileId, kind: SmolStr, modify: fn(DynamicImage) -> DynamicImage) -> Self {
        let image = image::load_from_memory(data.as_ref()).unwrap();
        let image = modify(image);
        let (pixels, dimensions) = image_to_egui(image);

        Self {
            pixels,
            dimensions,
            id,
            kind,
        }
    }
}

fn image_to_egui(image: DynamicImage) -> (Vec<Color32>, (usize, usize)) {
    let buf = image.to_rgba8();
    let dimensions = (buf.width() as usize, buf.height() as usize);
    let pixels = buf.into_vec();
    let pixels = pixels
        .chunks(4)
        .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect::<Vec<_>>();
    (pixels, dimensions)
}
