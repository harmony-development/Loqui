use std::path::PathBuf;

use client::{
    content,
    error::{ClientError, ClientResult},
    harmony_rust_sdk::api::{
        exports::prost::bytes::Bytes,
        profile::{GetAppDataRequest, SetAppDataRequest},
        rest::FileId,
    },
    Client,
};
use serde::{Deserialize, Serialize};

/// Application instance specific config. AKA not synced across different loqui
/// instances (web, other desktops etc.).
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct LocalConfig {
    /// Scale factor (pixels per point).
    #[serde(default)]
    pub scale_factor: f32,
    /// Background image for this user.
    #[serde(default)]
    pub bg_image: BgImage,
}

impl LocalConfig {
    pub fn load() -> Self {
        content::get_local_config::<Self>("config").unwrap_or_default()
    }

    pub fn store(&self) {
        content::set_local_config("config", self)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BgImage {
    /// Show the harmony lotus.
    Default,
    /// Show nothing.
    None,
    /// Show a local image.
    Local(PathBuf),
    /// Fetch and show an external image.
    External(String),
}

impl BgImage {
    pub async fn load(self) -> ClientResult<()> {
        let res = match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Local(path) => tokio::task::spawn_blocking(move || std::fs::read(path))
                .await
                .expect("task panicked")
                .map_err(|err| err.to_string())
                .map(Bytes::from),
            Self::External(url) => {
                (async {
                    let resp = reqwest::get(url).await.map_err(|err| err.to_string())?;
                    resp.bytes().await.map_err(|err| err.to_string())
                })
                .await
            }
            _ => return Ok(()),
        };

        match res {
            Ok(data) => {
                crate::image_cache::op::decode_image(data, FileId::Id(String::new()), "bg_image".to_string());
                Ok(())
            }
            Err(err) => Err(ClientError::Custom(err)),
        }
    }
}

impl Default for BgImage {
    fn default() -> Self {
        Self::Default
    }
}

/// Synced config across all loqui instances for a user.
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct Config {
    /// Keywords that will trigger a mention
    #[serde(default)]
    pub mention_keywords: Vec<String>,
}

impl Config {
    pub async fn load(client: &Client) -> ClientResult<Config> {
        let req = GetAppDataRequest::new("loqui".to_string());
        let raw = client.inner().call(req).await?.app_data;

        if raw.is_empty() {
            return Ok(Config::default());
        }

        let structured: Self = serde_json::from_slice(&raw)
            .map_err(|err| ClientError::Custom(format!("failed to deserialize config: {}", err)))?;
        Ok(structured)
    }

    pub async fn store(&self, client: &Client) -> ClientResult<()> {
        let serialized = serde_json::to_vec(self).expect("must be valid config");
        let req = SetAppDataRequest::new("loqui".to_string(), serialized);
        client.inner().call(req).await?;
        Ok(())
    }
}
