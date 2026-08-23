use std::error::Error;
use std::fmt;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};

const FABRIC_META_BASE_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
const MAX_META_RESPONSE_SIZE: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
pub enum FabricError {
    Http(reqwest::Error),
    InvalidBaseUrl,
    ResponseTooLarge,
    Json(serde_json::Error),
}

impl fmt::Display for FabricError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "Fabric Metaへの接続に失敗しました: {error}"),
            Self::InvalidBaseUrl => write!(formatter, "Fabric MetaのURL設定が正しくありません"),
            Self::ResponseTooLarge => write!(formatter, "Fabric Metaの応答が大きすぎます"),
            Self::Json(error) => write!(
                formatter,
                "Fabric Metaの応答を解釈できませんでした: {error}"
            ),
        }
    }
}

impl Error for FabricError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for FabricError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for FabricError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricLoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderComponent,
}

#[derive(Debug, Deserialize)]
struct LoaderComponent {
    version: String,
    stable: bool,
}

pub fn list_loader_versions(
    minecraft_version: &str,
) -> Result<Vec<FabricLoaderVersion>, FabricError> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("MonaLauncher/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let url = loader_versions_url(minecraft_version)?;
    let response = client.get(url).send()?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_META_RESPONSE_SIZE)
    {
        return Err(FabricError::ResponseTooLarge);
    }
    let bytes = response.bytes()?;
    if bytes.len() as u64 > MAX_META_RESPONSE_SIZE {
        return Err(FabricError::ResponseTooLarge);
    }
    let entries: Vec<LoaderEntry> = serde_json::from_slice(&bytes)?;

    Ok(entries
        .into_iter()
        .map(|entry| FabricLoaderVersion {
            version: entry.loader.version,
            stable: entry.loader.stable,
        })
        .collect())
}

fn loader_versions_url(minecraft_version: &str) -> Result<Url, FabricError> {
    let mut url = Url::parse(FABRIC_META_BASE_URL).map_err(|_| FabricError::InvalidBaseUrl)?;
    url.path_segments_mut()
        .map_err(|_| FabricError::InvalidBaseUrl)?
        .push(minecraft_version);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_minecraft_version_as_one_url_segment() {
        let url = loader_versions_url("../unsafe value").unwrap();

        assert_eq!(
            url.as_str(),
            "https://meta.fabricmc.net/v2/versions/loader/..%2Funsafe%20value"
        );
    }

    #[test]
    fn parses_loader_catalog_entries() {
        let entries: Vec<LoaderEntry> = serde_json::from_str(
            r#"[{
                "loader": { "version": "0.19.3", "stable": true },
                "intermediary": { "version": "1.21.8", "stable": true }
            }]"#,
        )
        .unwrap();

        assert_eq!(entries[0].loader.version, "0.19.3");
        assert!(entries[0].loader.stable);
    }
}
