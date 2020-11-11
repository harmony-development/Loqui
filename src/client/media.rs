use std::path::Path;
use std::path::PathBuf;

pub use iced::image::Handle as ImageHandle;
use iced_native::image::Data;
use indexmap::IndexMap;
use ruma::api::exports::http::Uri;

pub fn make_content_path(content_url: &Uri) -> PathBuf {
    make_content_folder(content_url).join(make_content_filename(content_url))
}

pub fn make_content_filename(content_url: &Uri) -> PathBuf {
    let filename = content_url.path()[1..].to_string();
    PathBuf::from(filename)
}

pub fn make_content_folder(content_url: &Uri) -> PathBuf {
    let server_media_dir = format!(
        "{}content/{}",
        crate::data_dir!(),
        content_url.authority().unwrap().as_str().replace('.', "_")
    );
    PathBuf::from(server_media_dir)
}

pub fn infer_mimetype(data: &[u8]) -> String {
    infer::get(&data)
        .map(|filetype| filetype.mime_type().to_string())
        .unwrap_or_else(|| String::from("application/octet-stream"))
}

pub fn get_filename(path: impl AsRef<Path>) -> String {
    path.as_ref()
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("unknown"))
}

pub fn get_in_memory(handle: &ImageHandle) -> &[u8] {
    match handle.data() {
        Data::Bytes(raw) => raw.as_slice(),
        _ => panic!(),
    }
}

const MAX_CACHE_SIZE: usize = 1000 * 1000 * 100; // 100Mb
pub struct ThumbnailStore(IndexMap<Uri, ImageHandle>);

impl ThumbnailStore {
    pub fn new() -> Self {
        Self(IndexMap::new())
    }

    pub fn put_thumbnail(&mut self, thumbnail_url: Uri, thumbnail: ImageHandle) {
        let cache_size: usize = self.0.values().map(|h| get_in_memory(h).len()).sum();
        let thumbnail_size = get_in_memory(&thumbnail).len();

        if cache_size + thumbnail_size > MAX_CACHE_SIZE {
            let mut current_size = 0;
            let mut remove_upto = 0;
            for (index, size) in self.0.values().map(|h| get_in_memory(h).len()).enumerate() {
                if current_size >= thumbnail_size {
                    remove_upto = index + 1;
                    break;
                }
                current_size += size;
            }
            for index in 0..remove_upto {
                self.0.shift_remove_index(index);
            }
        } else {
            self.0.insert(thumbnail_url, thumbnail);
        }
    }

    pub fn get_thumbnail(&self, thumbnail_url: &Uri) -> Option<&ImageHandle> {
        self.0.get(thumbnail_url)
    }

    pub fn invalidate_thumbnail(&mut self, thumbnail_url: &Uri) {
        self.0.remove(thumbnail_url);
    }
}

#[derive(Debug, Clone)]
pub enum ContentType {
    Image,
    Audio,
    Video,
    Other,
}

impl ContentType {
    pub fn new(mimetype: &str) -> Self {
        use ContentType::*;

        if let Some(filetype) = mimetype.split('/').next() {
            return match filetype {
                "image" => Image,
                "audio" => Audio,
                "video" => Video,
                _ => Other,
            };
        }
        Other
    }
}

impl From<&str> for ContentType {
    fn from(other: &str) -> Self {
        ContentType::new(other)
    }
}
