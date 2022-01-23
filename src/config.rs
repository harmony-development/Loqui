use client::{
    error::{ClientError, ClientResult},
    harmony_rust_sdk::api::profile::{GetAppDataRequest, SetAppDataRequest},
    Client,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub bg_image: BgImage,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BgImage {
    Default,
    None,
    Local(String),
    External(String),
}

impl Default for BgImage {
    fn default() -> Self {
        Self::Default
    }
}
