use std::error::Error;
use std::fmt;
use std::io::Read;
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};

const MODRINTH_API_BASE_URL: &str = "https://api.modrinth.com/v2";
const MODRINTH_ALLOWED_HOSTS: &[&str] = &["api.modrinth.com", "cdn.modrinth.com"];
const MAX_API_RESPONSE_SIZE: u64 = 4 * 1024 * 1024;
const SEARCH_LIMIT: u32 = 20;
const MAX_SEARCH_QUERY_LENGTH: usize = 100;
const MAX_VERSION_RESULTS: usize = 1024;
const MAX_VERSION_DEPENDENCIES: usize = 256;
const MAX_VERSION_FILES: usize = 256;

#[derive(Debug)]
pub enum ModrinthError {
    Http(reqwest::Error),
    Io(std::io::Error),
    InvalidBaseUrl,
    ResponseTooLarge,
    Json(serde_json::Error),
    ServiceStatus {
        status: StatusCode,
        message: Option<String>,
    },
    SearchQueryTooLong(usize),
    InvalidIdentifier(String),
    IdentifierMismatch {
        expected: String,
        actual: String,
    },
    InvalidResponseData(&'static str),
}

impl fmt::Display for ModrinthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(formatter, "Modrinthへの接続に失敗しました: {error}"),
            Self::Io(error) => write!(formatter, "Modrinthの応答を読み取れませんでした: {error}"),
            Self::InvalidBaseUrl => write!(formatter, "Modrinth APIのURL設定が正しくありません"),
            Self::ResponseTooLarge => write!(formatter, "Modrinth APIの応答が大きすぎます"),
            Self::Json(error) => {
                write!(formatter, "Modrinth APIの応答を解釈できませんでした: {error}")
            }
            Self::ServiceStatus { status, message } => {
                write!(formatter, "Modrinth APIがHTTP {status}を返しました")?;
                if let Some(message) = message {
                    write!(formatter, ": {message}")?;
                }
                Ok(())
            }
            Self::SearchQueryTooLong(length) => write!(
                formatter,
                "Mod検索キーワードは{MAX_SEARCH_QUERY_LENGTH}文字以内にしてください（現在{length}文字）"
            ),
            Self::InvalidIdentifier(identifier) => {
                write!(formatter, "Modrinth IDが正しくありません: {identifier}")
            }
            Self::IdentifierMismatch { expected, actual } => write!(
                formatter,
                "Modrinth APIのIDが一致しません: expected {expected}, got {actual}"
            ),
            Self::InvalidResponseData(field) => {
                write!(formatter, "Modrinth APIの応答フィールドが不正です: {field}")
            }
        }
    }
}

impl Error for ModrinthError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for ModrinthError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<std::io::Error> for ModrinthError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ModrinthError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct ModSearchResponse {
    pub hits: Vec<ModSearchHit>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct ModSearchHit {
    pub project_id: String,
    pub slug: Option<String>,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    pub date_modified: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub date_published: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub dependencies: Vec<ModrinthDependency>,
    pub files: Vec<ModrinthFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub file_name: Option<String>,
    pub dependency_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthFile {
    pub hashes: ModrinthFileHashes,
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub file_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthFileHashes {
    pub sha512: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthProject {
    pub id: String,
    pub title: String,
    pub description: String,
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServiceErrorResponse {
    description: Option<String>,
    error: Option<String>,
}

#[derive(Clone)]
pub struct ModrinthClient {
    client: Client,
}

impl ModrinthClient {
    pub fn new() -> Result<Self, ModrinthError> {
        let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if !modrinth_url_allowed(attempt.url()) {
                attempt.error("Modrinth redirect must use an approved HTTPS host")
            } else {
                attempt.follow()
            }
        });
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(redirect_policy)
            .user_agent(concat!(
                "aomona/MonaLauncher/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/aomona/MonaLauncher)"
            ))
            .build()?;
        Ok(Self { client })
    }

    pub fn search_mods(
        &self,
        query: &str,
        minecraft_version: &str,
        loader: &str,
        offset: u32,
    ) -> Result<ModSearchResponse, ModrinthError> {
        let query = query.trim();
        let query_length = query.chars().count();
        if query_length > MAX_SEARCH_QUERY_LENGTH {
            return Err(ModrinthError::SearchQueryTooLong(query_length));
        }
        let facets = serde_json::to_string(&[
            ["project_type:mod".to_owned()],
            [format!("versions:{minecraft_version}")],
            [format!("categories:{loader}")],
        ])?;
        let mut url = api_url(&["search"])?;
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("facets", &facets)
            .append_pair("index", "relevance")
            .append_pair("offset", &offset.to_string())
            .append_pair("limit", &SEARCH_LIMIT.to_string());
        let mut response: ModSearchResponse = self.get_json(url)?;
        if response.hits.len() > SEARCH_LIMIT as usize || response.limit > SEARCH_LIMIT {
            return Err(ModrinthError::InvalidResponseData("search.hits"));
        }
        for hit in &mut response.hits {
            validate_identifier(&hit.project_id)?;
            hit.title = sanitize_text(&hit.title, 120);
            hit.description = sanitize_text(&hit.description, 320);
            hit.author = sanitize_text(&hit.author, 64);
            hit.date_modified = sanitize_text(&hit.date_modified, 64);
            hit.slug = hit.slug.take().map(|slug| sanitize_text(&slug, 80));
        }
        Ok(response)
    }

    pub fn project_versions(
        &self,
        project_id: &str,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<Vec<ModrinthVersion>, ModrinthError> {
        validate_identifier(project_id)?;
        let mut url = api_url(&["project", project_id, "version"])?;
        let loaders = serde_json::to_string(&[loader])?;
        let game_versions = serde_json::to_string(&[minecraft_version])?;
        url.query_pairs_mut()
            .append_pair("loaders", &loaders)
            .append_pair("game_versions", &game_versions)
            .append_pair("include_changelog", "false");
        let mut versions: Vec<ModrinthVersion> = self.get_json(url)?;
        if versions.len() > MAX_VERSION_RESULTS {
            return Err(ModrinthError::InvalidResponseData("versions"));
        }
        for version in &mut versions {
            validate_version_response(version)?;
            if version.project_id != project_id {
                return Err(ModrinthError::IdentifierMismatch {
                    expected: project_id.to_owned(),
                    actual: version.project_id.clone(),
                });
            }
        }
        Ok(versions)
    }

    pub fn version(&self, version_id: &str) -> Result<ModrinthVersion, ModrinthError> {
        validate_identifier(version_id)?;
        let mut version: ModrinthVersion = self.get_json(api_url(&["version", version_id])?)?;
        if version.id != version_id {
            return Err(ModrinthError::IdentifierMismatch {
                expected: version_id.to_owned(),
                actual: version.id,
            });
        }
        validate_version_response(&mut version)?;
        Ok(version)
    }

    pub fn project(&self, project_id: &str) -> Result<ModrinthProject, ModrinthError> {
        validate_identifier(project_id)?;
        let mut project: ModrinthProject = self.get_json(api_url(&["project", project_id])?)?;
        if project.id != project_id {
            return Err(ModrinthError::IdentifierMismatch {
                expected: project_id.to_owned(),
                actual: project.id,
            });
        }
        project.title = sanitize_text(&project.title, 120);
        project.description = sanitize_text(&project.description, 320);
        project.slug = project.slug.map(|slug| sanitize_text(&slug, 80));
        Ok(project)
    }

    pub(crate) fn download(&self, url: Url) -> Result<Response, ModrinthError> {
        Ok(self.client.get(url).send()?.error_for_status()?)
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T, ModrinthError> {
        let response = self.client.get(url).send()?;
        let status = response.status();
        let bytes = read_bounded(response)?;
        if !status.is_success() {
            let message = serde_json::from_slice::<ServiceErrorResponse>(&bytes)
                .ok()
                .and_then(|error| error.description.or(error.error))
                .map(sanitize_service_message);
            return Err(ModrinthError::ServiceStatus { status, message });
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}

fn modrinth_url_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none_or(|port| port == 443)
        && url
            .host_str()
            .is_some_and(|host| MODRINTH_ALLOWED_HOSTS.contains(&host))
}

fn validate_version_response(version: &mut ModrinthVersion) -> Result<(), ModrinthError> {
    validate_identifier(&version.id)?;
    validate_identifier(&version.project_id)?;
    if version.dependencies.len() > MAX_VERSION_DEPENDENCIES {
        return Err(ModrinthError::InvalidResponseData("dependencies"));
    }
    if version.files.len() > MAX_VERSION_FILES {
        return Err(ModrinthError::InvalidResponseData("files"));
    }
    version.name = sanitize_text(&version.name, 120);
    version.version_number = sanitize_text(&version.version_number, 80);
    version.version_type = sanitize_text(&version.version_type, 16);
    version.date_published = sanitize_text(&version.date_published, 64);
    for dependency in &mut version.dependencies {
        if let Some(version_id) = &dependency.version_id {
            validate_identifier(version_id)?;
        }
        if let Some(project_id) = &dependency.project_id {
            validate_identifier(project_id)?;
        }
        dependency.file_name = dependency
            .file_name
            .take()
            .map(|name| sanitize_text(&name, 240));
        dependency.dependency_type = sanitize_text(&dependency.dependency_type, 32);
    }
    for file in &mut version.files {
        file.filename = sanitize_text(&file.filename, 240);
        if let Some(file_type) = &mut file.file_type {
            *file_type = sanitize_text(file_type, 32);
        }
    }
    if version.game_versions.len() > 256 || version.loaders.len() > 64 {
        return Err(ModrinthError::InvalidResponseData("compatibility"));
    }
    for item in version
        .game_versions
        .iter_mut()
        .chain(version.loaders.iter_mut())
    {
        *item = sanitize_text(item, 128);
    }
    Ok(())
}

fn api_url(segments: &[&str]) -> Result<Url, ModrinthError> {
    let mut url = Url::parse(MODRINTH_API_BASE_URL).map_err(|_| ModrinthError::InvalidBaseUrl)?;
    let mut path = url
        .path_segments_mut()
        .map_err(|_| ModrinthError::InvalidBaseUrl)?;
    for segment in segments {
        path.push(segment);
    }
    drop(path);
    Ok(url)
}

fn read_bounded(mut response: Response) -> Result<Vec<u8>, ModrinthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_API_RESPONSE_SIZE)
    {
        return Err(ModrinthError::ResponseTooLarge);
    }
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(MAX_API_RESPONSE_SIZE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_API_RESPONSE_SIZE {
        return Err(ModrinthError::ResponseTooLarge);
    }
    Ok(bytes)
}

pub(crate) fn validate_identifier(identifier: &str) -> Result<(), ModrinthError> {
    if identifier.len() != 8 || !identifier.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(ModrinthError::InvalidIdentifier(identifier.to_owned()));
    }
    Ok(())
}

fn sanitize_service_message(message: String) -> String {
    sanitize_text(&message, 256)
}

fn sanitize_text(value: &str, maximum: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(maximum)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_search_url_with_compatible_mod_facets() {
        let facets = serde_json::to_string(&[
            ["project_type:mod"],
            ["versions:1.21.8"],
            ["categories:fabric"],
        ])
        .unwrap();
        let mut url = api_url(&["search"]).unwrap();
        url.query_pairs_mut()
            .append_pair("query", "sodium options")
            .append_pair("facets", &facets);

        assert_eq!(url.host_str(), Some("api.modrinth.com"));
        let parameters = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            parameters.get("query").map(|value| value.as_ref()),
            Some("sodium options")
        );
        assert_eq!(
            parameters.get("facets").map(|value| value.as_ref()),
            Some(facets.as_str())
        );
    }

    #[test]
    fn encodes_identifiers_as_single_path_segments() {
        let url = api_url(&["project", "../unsafe", "version"]).unwrap();

        assert_eq!(
            url.as_str(),
            "https://api.modrinth.com/v2/project/..%2Funsafe/version"
        );
    }

    #[test]
    fn validates_stable_modrinth_identifiers() {
        assert!(validate_identifier("AANobbMI").is_ok());
        assert!(validate_identifier("../evil").is_err());
        assert!(validate_identifier("too-short").is_err());
    }

    #[test]
    fn bounds_remote_catalog_text() {
        assert_eq!(sanitize_text("safe\ntext", 20), "safetext");
        assert_eq!(sanitize_text(&"a".repeat(200), 120).len(), 120);
    }

    #[test]
    fn parses_search_and_version_responses() {
        let search: ModSearchResponse = serde_json::from_str(
            r#"{
                "hits": [{
                    "project_id": "AANobbMI",
                    "slug": "sodium",
                    "title": "Sodium",
                    "description": "Renderer optimization",
                    "author": "jellysquid3",
                    "downloads": 100,
                    "follows": 10,
                    "date_modified": "2026-01-01T00:00:00Z"
                }],
                "offset": 0,
                "limit": 20,
                "total_hits": 1
            }"#,
        )
        .unwrap();
        let version: ModrinthVersion = serde_json::from_str(
            r#"{
                "id": "7pwil2dy",
                "project_id": "AANobbMI",
                "name": "Sodium 0.7.3",
                "version_number": "0.7.3",
                "version_type": "release",
                "date_published": "2026-01-01T00:00:00Z",
                "game_versions": ["1.21.8"],
                "loaders": ["fabric"],
                "dependencies": [],
                "files": [{
                    "hashes": { "sha512": "00", "sha1": "00" },
                    "url": "https://cdn.modrinth.com/data/file.jar",
                    "filename": "file.jar",
                    "primary": true,
                    "size": 1,
                    "file_type": null
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(search.hits[0].project_id, "AANobbMI");
        assert_eq!(version.files[0].filename, "file.jar");
    }
}
