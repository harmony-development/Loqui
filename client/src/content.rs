use super::ClientError;
use harmony_rust_sdk::client::api::rest::FileId;
use std::path::{Path, PathBuf};

pub const MAX_THUMB_SIZE: u64 = 1000 * 500; // 500kb

pub const SESSIONS_DIR_NAME: &str = "sessions";
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
    latest_session_file: PathBuf,
    sessions_dir: PathBuf,
    log_file: PathBuf,
    content_dir: PathBuf,
}

impl Default for ContentStore {
    fn default() -> Self {
        let (sessions_dir, log_file, content_dir) =
            match directories_next::ProjectDirs::from("nodomain", "yusdacra", "crust") {
                Some(app_dirs) => (
                    app_dirs.data_dir().join(SESSIONS_DIR_NAME),
                    app_dirs.data_dir().join(LOG_FILENAME),
                    app_dirs.cache_dir().join(CONTENT_DIR_NAME),
                ),
                // Fallback to current working directory if no HOME is present
                None => (SESSIONS_DIR_NAME.into(), LOG_FILENAME.into(), CONTENT_DIR_NAME.into()),
            };

        Self {
            latest_session_file: sessions_dir.join("latest"),
            sessions_dir,
            log_file,
            content_dir,
        }
    }
}

impl ContentStore {
    pub fn session_path(&self, homeserver: &str, user_id: u64) -> PathBuf {
        self.sessions_dir()
            .join(format!("{}_{}", urlencoding::encode(&homeserver), user_id))
    }

    pub fn content_path(&self, id: &FileId) -> PathBuf {
        let normalized_id = urlencoding::encode(id.as_str());
        self.content_dir().join(normalized_id)
    }

    pub fn content_mimetype(&self, id: &FileId) -> String {
        infer::get_from_path(self.content_path(id))
            .ok()
            .flatten()
            .map(|filetype| filetype.mime_type().to_string())
            .unwrap_or_else(|| String::from("application/octet-stream"))
    }

    pub fn content_exists(&self, id: &FileId) -> bool {
        self.content_path(id).exists()
    }

    pub fn create_req_dirs(&self) -> Result<(), ClientError> {
        use std::fs::create_dir_all;

        create_dir_all(self.content_dir())?;
        create_dir_all(self.sessions_dir())?;
        create_dir_all(self.log_file().parent().unwrap_or(&Path::new(".")))?;

        Ok(())
    }

    pub fn latest_session_file(&self) -> &Path {
        self.latest_session_file.as_path()
    }

    pub fn content_dir(&self) -> &Path {
        self.content_dir.as_path()
    }

    pub fn sessions_dir(&self) -> &Path {
        self.sessions_dir.as_path()
    }

    pub fn log_file(&self) -> &Path {
        self.log_file.as_path()
    }
}
