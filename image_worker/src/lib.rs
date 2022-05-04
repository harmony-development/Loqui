#![feature(let_else)]

use image::{DynamicImage, GenericImageView};
#[cfg(target_arch = "wasm32")]
use rkyv::{Archive, Deserialize, Serialize};

#[cfg_attr(target_arch = "wasm32", derive(Archive, Deserialize, Serialize))]
pub struct ImageLoaded {
    pub pixels: Vec<u8>,
    pub dimensions: [usize; 2],
    pub kind: String,
    pub id: String,
}

#[cfg(target_arch = "wasm32")]
#[derive(Archive, Deserialize, Serialize)]
pub struct ImageData {
    pub data: Vec<u8>,
    pub kind: String,
    pub id: String,
}

#[cfg(target_arch = "wasm32")]
pub fn load_image(data: Vec<u8>) -> Vec<u8> {
    #[allow(unsafe_code)]
    let image_data = unsafe { rkyv::archived_root::<ImageData>(&data) };
    tracing::debug!("received image (id {})", image_data.id);
    let Some(mut loaded) = load_image_logic(image_data.data.as_ref(), image_data.kind.as_str()) else {
        tracing::error!(
            "could not load an image (id {}); most likely unsupported format",
            image_data.id
        );
        return Vec::new();
    };
    loaded.kind = image_data.kind.to_string();
    loaded.id = image_data.id.to_string();

    rkyv::to_bytes::<_, 2048>(&loaded).unwrap().into()
}

pub fn load_image_logic(data: &[u8], kind: &str) -> Option<ImageLoaded> {
    let modify = match kind {
        "minithumbnail" => |image: DynamicImage| image.blur(4.0),
        "guild" | "avatar" => |image: DynamicImage| image.resize(96, 96, image::imageops::FilterType::Lanczos3),
        _ => |image: DynamicImage| {
            if image.dimensions().0 > 1280 || image.dimensions().1 > 720 {
                image.resize(1280, 720, image::imageops::FilterType::Triangle)
            } else {
                image
            }
        },
    };

    let image = image::load_from_memory(data).ok()?;
    let image = modify(image);
    let image = image.to_rgba8();

    let dimensions = [image.width() as usize, image.height() as usize];

    Some(ImageLoaded {
        pixels: image.into_vec(),
        dimensions,
        id: String::new(),
        kind: String::new(),
    })
}
