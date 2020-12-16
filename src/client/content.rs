use super::ClientError;
use http::Uri;
use iced_native::image::Data;
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

pub use iced::image::Handle as ImageHandle;

pub const SESSION_FILENAME: &str = "session";
pub const LOG_FILENAME: &str = "log";
pub const CONTENT_DIR_NAME: &str = "content";

pub fn infer_type_from_bytes(data: &[u8]) -> String {
    infer::get(&data)
        .map(|filetype| filetype.mime_type().to_string())
        .unwrap_or_else(|| String::from("application/octet-stream"))
}

pub fn get_filename<P: AsRef<Path>>(path: P) -> String {
    path.as_ref()
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("unknown"))
}

#[derive(Debug, Clone)]
pub struct ContentStore {
    session_file: PathBuf,
    log_file: PathBuf,
    content_dir: PathBuf,
}

impl Default for ContentStore {
    fn default() -> Self {
        let (session_file, log_file, content_dir) =
            match directories_next::ProjectDirs::from("nodomain", "yusdacra", "icy_matrix") {
                Some(app_dirs) => (
                    app_dirs.data_dir().join(SESSION_FILENAME),
                    app_dirs.data_dir().join(LOG_FILENAME),
                    app_dirs.data_dir().join(CONTENT_DIR_NAME),
                ),
                // Fallback to current working directory if no HOME is present
                None => (
                    SESSION_FILENAME.into(),
                    LOG_FILENAME.into(),
                    CONTENT_DIR_NAME.into(),
                ),
            };

        Self {
            session_file,
            log_file,
            content_dir,
        }
    }
}

impl ContentStore {
    pub fn content_path(&self, id: &Uri) -> PathBuf {
        let normalized_id = id
            .to_string()
            .replace(|c| [' ', '/', '\\', '.', ':'].contains(&c), "_");
        self.content_dir().join(normalized_id)
    }

    pub fn content_mimetype(&self, id: &Uri) -> String {
        infer::get_from_path(self.content_path(id))
            .map_or(None, Some)
            .flatten()
            .map(|filetype| filetype.mime_type().to_string())
            .unwrap_or_else(|| String::from("application/octet-stream"))
    }

    pub fn content_exists(&self, id: &Uri) -> bool {
        self.content_path(id).exists()
    }

    pub fn create_req_dirs(&self) -> Result<(), ClientError> {
        use std::fs::create_dir_all;

        create_dir_all(self.content_dir())?;
        create_dir_all(self.session_file().parent().unwrap_or(&Path::new(".")))?;
        create_dir_all(self.log_file().parent().unwrap_or(&Path::new(".")))?;

        Ok(())
    }

    pub fn content_dir(&self) -> &Path {
        self.content_dir.as_path()
    }

    pub fn session_file(&self) -> &Path {
        self.session_file.as_path()
    }

    pub fn log_file(&self) -> &Path {
        self.log_file.as_path()
    }
}

fn get_image_size_from_handle(handle: &ImageHandle) -> Option<u64> {
    // This one angers me a lot, iced pls read the file beforehand and cache it
    match handle.data() {
        Data::Bytes(raw) => Some(raw.len() as u64),
        Data::Path(path) => std::fs::metadata(path).map_or(None, |meta| Some(meta.len())),
        Data::Pixels {
            pixels,
            height: _,
            width: _,
        } => Some(pixels.len() as u64),
    }
}

pub struct ThumbnailCache {
    thumbnails: IndexMap<Uri, ImageHandle>,
    max_size: u64,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        const MAX_CACHE_SIZE: u64 = 1000 * 1000 * 100; // 100Mb
        Self::new(MAX_CACHE_SIZE)
    }
}

impl ThumbnailCache {
    pub fn new(max_size: u64) -> Self {
        Self {
            thumbnails: IndexMap::new(),
            max_size,
        }
    }

    pub fn put_thumbnail(&mut self, thumbnail_id: Uri, thumbnail: ImageHandle) {
        let thumbnail_size = match get_image_size_from_handle(&thumbnail) {
            Some(size) => size,
            None => return,
        };
        let cache_size = self.len();

        if cache_size + thumbnail_size > self.max_size {
            let mut current_size = 0;
            let mut remove_upto = 0;
            for (index, size) in self
                .thumbnails
                .values()
                .flat_map(|h| get_image_size_from_handle(h))
                .enumerate()
            {
                if current_size >= thumbnail_size {
                    remove_upto = index + 1;
                    break;
                }
                current_size += size;
            }
            for index in 0..remove_upto {
                self.thumbnails.shift_remove_index(index);
            }
        } else {
            self.thumbnails.insert(thumbnail_id, thumbnail);
        }
    }

    pub fn len(&self) -> u64 {
        self.thumbnails
            .values()
            .flat_map(|h| get_image_size_from_handle(h))
            .sum()
    }

    pub fn has_thumbnail(&self, thumbnail_id: &Uri) -> bool {
        self.thumbnails.contains_key(thumbnail_id)
    }

    pub fn get_thumbnail(&self, thumbnail_id: &Uri) -> Option<&ImageHandle> {
        self.thumbnails.get(thumbnail_id)
    }

    pub fn invalidate_thumbnail(&mut self, thumbnail_id: &Uri) {
        self.thumbnails.remove(thumbnail_id);
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
            match filetype {
                "image" => Image,
                "audio" => Audio,
                "video" => Video,
                _ => Other,
            }
        } else {
            Other
        }
    }
}

impl From<&str> for ContentType {
    fn from(other: &str) -> Self {
        ContentType::new(other)
    }
}
