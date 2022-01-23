use std::path::Path;

pub const MAX_THUMB_SIZE: u64 = 1000 * 500; // 500kb

pub fn infer_type_from_bytes(data: &[u8]) -> String {
    infer::get(data)
        .map(|filetype| filetype.mime_type().to_string())
        .unwrap_or_else(|| String::from("application/octet-stream"))
}

pub fn get_filename<P: AsRef<Path>>(path: P) -> String {
    path.as_ref()
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("unknown"))
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
pub mod web {
    use gloo_storage::{LocalStorage, Storage};

    use crate::Session;

    pub fn get_latest_session() -> Option<Session> {
        <LocalStorage as Storage>::get("latest_session").ok()
    }

    pub fn put_session(session: Session) {
        let _ = <LocalStorage as Storage>::set("latest_session", session);
    }

    pub fn delete_latest_session() {
        <LocalStorage as Storage>::delete("latest_session")
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use crate::{error::ClientError, Session};
    use harmony_rust_sdk::client::api::rest::FileId;
    use std::path::{Path, PathBuf};

    lazy_static::lazy_static! {
        static ref STORE: ContentStore = ContentStore::default();
    }

    pub fn get_latest_session() -> Option<Session> {
        let session_raw = std::fs::read(STORE.latest_session_file()).ok()?;
        let session = toml::from_slice::<Session>(&session_raw)
            .map_err(|err| ClientError::Custom(err.to_string()))
            .ok()?;
        Some(session)
    }

    pub fn put_session(session: Session) {
        let serialized = toml::to_string_pretty(&session).expect("failed to serialize");
        let _ = std::fs::write(STORE.latest_session_file(), serialized.into_bytes());
    }

    pub fn delete_latest_session() {
        let _ = std::fs::remove_file(STORE.latest_session_file());
    }

    pub const SESSIONS_DIR_NAME: &str = "sessions";
    pub const LOG_FILENAME: &str = "log";
    pub const CONTENT_DIR_NAME: &str = "content";

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
                match directories_next::ProjectDirs::from("nodomain", "yusdacra", "loqui") {
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
        pub fn content_path(&self, id: &FileId) -> PathBuf {
            let id = id.to_string();
            let normalized_id = urlencoding::encode(id.as_str());
            self.content_dir().join(normalized_id.as_ref())
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
            create_dir_all(self.log_file().parent().unwrap_or_else(|| Path::new(".")))?;

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
}
