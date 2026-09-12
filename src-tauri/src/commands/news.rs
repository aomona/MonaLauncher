mod links;

use crate::minecraft::file_io::{read_bounded_file, write_atomic};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Manager;

const BASE_URL: &str = "https://launchercontent.mojang.com/v2/";
const MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    News,
    JavaPatchNotes,
}

impl Source {
    fn file(self) -> &'static str {
        match self {
            Self::News => "news.json",
            Self::JavaPatchNotes => "javaPatchNotes.json",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::News => "Minecraftニュース",
            Self::JavaPatchNotes => "Javaパッチノート",
        }
    }
    fn cache_file(self) -> &'static str {
        match self {
            Self::News => "mojang-v2-news.json",
            Self::JavaPatchNotes => "mojang-v2-java-patch-notes.json",
        }
    }
}

#[derive(Deserialize, Serialize)]
struct Feed {
    version: u32,
    entries: Vec<serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    feed: Feed,
    fetched_at: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNews {
    id: String,
    title: String,
    text: String,
    category: String,
    date: String,
    read_more_link: String,
    news_page_image: Option<RawImage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPatch {
    id: String,
    title: String,
    version: String,
    #[serde(rename = "type")]
    release_type: String,
    date: String,
    short_text: String,
    image: Option<RawImage>,
}

#[derive(Deserialize)]
struct RawImage {
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsEntry {
    id: String,
    title: String,
    summary: String,
    category: String,
    date: String,
    kind: Source,
    article_url: String,
    image_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsFeed {
    entries: Vec<NewsEntry>,
    fetched_at: u64,
    warning: Option<String>,
    cached: bool,
}

fn allowed_url(value: &str, image: bool) -> Option<String> {
    let base = Url::parse(BASE_URL).ok()?;
    let url = if image {
        base.join(value)
    } else {
        Url::parse(value)
    }
    .ok()?;
    let hosts: &[&str] = if image {
        &["launchercontent.mojang.com"]
    } else {
        &[
            "www.minecraft.net",
            "minecraft.net",
            "feedback.minecraft.net",
            "aka.ms",
            "www.youtube.com",
        ]
    };
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && hosts.contains(&url.host_str()?))
    .then(|| url.to_string())
}

fn normalize_date(value: &str) -> Option<String> {
    // Normalize both date-only news and RFC3339 patch timestamps before combining feeds.
    let date = if value.len() == 10 {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .ok()?
            .and_hms_opt(0, 0, 0)?
            .and_utc()
    } else {
        DateTime::parse_from_rfc3339(value)
            .ok()?
            .with_timezone(&Utc)
    };
    Some(date.to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn normalize(cache: &Cache, source: Source) -> Result<NewsFeed, String> {
    if cache.feed.version != 1 || cache.feed.entries.len() > 2000 {
        return Err("ニュース配信の形式に対応していません。".into());
    }
    let mut ids = HashSet::new();
    let mut entries: Vec<_> = cache
        .feed
        .entries
        .iter()
        .filter_map(|value| {
            let mut entry = match source {
                Source::News => {
                    let raw: RawNews = serde_json::from_value(value.clone()).ok()?;
                    NewsEntry {
                        id: format!("news:{}", raw.id),
                        title: raw.title,
                        summary: raw.text,
                        category: raw.category,
                        date: normalize_date(&raw.date)?,
                        kind: source,
                        article_url: allowed_url(&raw.read_more_link, false)?,
                        image_url: raw
                            .news_page_image
                            .and_then(|image| allowed_url(&image.url, true)),
                    }
                }
                Source::JavaPatchNotes => {
                    let raw: RawPatch = serde_json::from_value(value.clone()).ok()?;
                    NewsEntry {
                        id: format!("patch:{}", raw.id),
                        title: raw.title,
                        summary: raw.short_text,
                        category: format!("Java Patch Notes · {}", raw.release_type),
                        date: normalize_date(&raw.date)?,
                        kind: source,
                        article_url: links::patch_article_url(&raw.version, &raw.release_type)?,
                        image_url: raw.image.and_then(|image| allowed_url(&image.url, true)),
                    }
                }
            };
            if entry.title.trim().is_empty()
                || entry.id.ends_with(':')
                || !ids.insert(entry.id.clone())
            {
                return None;
            }
            // Some patch summaries use numeric references without a terminating semicolon.
            entry.summary = entry
                .summary
                .replace("&#38;", "&")
                .replace("&#38 ", "& ")
                .replace("&#x26;", "&")
                .replace("&amp;", "&");
            Some(entry)
        })
        .collect();
    if entries.is_empty() && !cache.feed.entries.is_empty() {
        return Err("ニュース配信に表示できる記事がありません。".into());
    }
    entries.sort_by(|a, b| b.date.cmp(&a.date));
    let warning = (entries.len() != cache.feed.entries.len())
        .then(|| format!("{}: 形式を確認できない記事を除外しました。", source.name()));
    Ok(NewsFeed {
        entries,
        fetched_at: cache.fetched_at,
        warning,
        cached: false,
    })
}

fn fetch(source: Source) -> Result<Cache, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(format!("{BASE_URL}{}", source.file()))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|_| "取得できませんでした。通信状態を確認して再試行してください。".to_string())?;
    let mut bytes = Vec::new();
    response
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("配信サイズが上限を超えています。".into());
    }
    let feed = serde_json::from_slice(&bytes)
        .map_err(|_| "配信の形式を読み取れませんでした。".to_string())?;
    Ok(Cache {
        feed,
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64,
    })
}

fn read_cache(path: &Path, source: Source) -> Option<NewsFeed> {
    let bytes = read_bounded_file(path, MAX_BYTES).ok()?;
    let cache = serde_json::from_slice(&bytes).ok()?;
    let mut result = normalize(&cache, source).ok()?;
    result.cached = true;
    Some(result)
}

fn append_warning(feed: &mut NewsFeed, warning: String) {
    feed.warning = Some(match feed.warning.take() {
        Some(previous) => format!("{previous}\n{warning}"),
        None => warning,
    });
}

fn accept_fetched(
    source: Source,
    path: Option<&Path>,
    fresh: Result<Cache, String>,
) -> Result<NewsFeed, String> {
    match fresh.and_then(|cache| normalize(&cache, source).map(|result| (cache, result))) {
        Ok((cache, mut result)) => {
            let saved = path
                .and_then(|path| {
                    let bytes = serde_json::to_vec(&cache).ok()?;
                    write_atomic(path, &bytes).ok()
                })
                .is_some();
            if !saved {
                append_warning(
                    &mut result,
                    format!(
                        "{}: 取得しましたが、オフライン用に保存できませんでした。",
                        source.name()
                    ),
                );
            }
            Ok(result)
        }
        Err(error) => {
            let message = format!("{}: {error}", source.name());
            if let Some(mut cached) = path.and_then(|path| read_cache(path, source)) {
                append_warning(
                    &mut cached,
                    format!("{message} 保存済みの記事を表示しています。"),
                );
                Ok(cached)
            } else {
                Err(message)
            }
        }
    }
}

fn combine(results: [Result<NewsFeed, String>; 2]) -> Result<NewsFeed, String> {
    let mut feeds = Vec::new();
    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok(feed) => feeds.push(feed),
            Err(error) => errors.push(error),
        }
    }
    if feeds.is_empty() {
        return Err(errors.join("\n"));
    }
    let mut merged = NewsFeed {
        entries: Vec::new(),
        fetched_at: u64::MAX,
        warning: None,
        cached: false,
    };
    for feed in feeds {
        merged.fetched_at = merged.fetched_at.min(feed.fetched_at);
        merged.cached |= feed.cached;
        merged.entries.extend(feed.entries);
        if let Some(warning) = feed.warning {
            append_warning(&mut merged, warning);
        }
    }
    for error in errors {
        append_warning(&mut merged, error);
    }
    merged.entries.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(merged)
}

#[tauri::command]
pub async fn cached_minecraft_news(app: tauri::AppHandle) -> Result<Option<NewsFeed>, String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let results = [Source::News, Source::JavaPatchNotes].map(|source| {
            read_cache(&dir.join(source.cache_file()), source)
                .ok_or_else(|| format!("{}は未取得です。", source.name()))
        });
        combine(results).ok()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_minecraft_news(app: tauri::AppHandle) -> Result<NewsFeed, String> {
    let dir = app.path().app_cache_dir().ok();
    let start = |source: Source| {
        let path = dir.as_ref().map(|dir| dir.join(source.cache_file()));
        tauri::async_runtime::spawn_blocking(move || {
            accept_fetched(source, path.as_deref(), fetch(source))
        })
    };
    // Start both requests before awaiting either; one unavailable feed must not hide the other.
    let news = start(Source::News);
    let patches = start(Source::JavaPatchNotes);
    combine([
        news.await
            .map_err(|e| e.to_string())
            .and_then(|result| result),
        patches
            .await
            .map_err(|e| e.to_string())
            .and_then(|result| result),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn news(id: &str, date: &str) -> serde_json::Value {
        json!({"id":id,"title":"News","text":"<script>plain text</script>","category":"Minecraft",
            "date":date,"readMoreLink":"https://www.minecraft.net/article/test","newsPageImage":{"url":"/v2/images/test.jpg"}})
    }
    fn patch(id: &str, date: &str) -> serde_json::Value {
        json!({"id":id,"title":"Minecraft 26.3 Release Candidate 2","shortText":"Blocks &#38 items","version":"26.3-rc-2",
            "type":"snapshot","date":date,"image":{"url":"/v2/images/patch.jpg"},"contentPath":"javaPatchNotes/test.json"})
    }
    fn cache(entries: Vec<serde_json::Value>) -> Cache {
        Cache {
            fetched_at: 42,
            feed: Feed {
                version: 1,
                entries,
            },
        }
    }

    #[test]
    fn resolves_v2_images_and_rejects_unsafe_destinations() {
        assert_eq!(
            allowed_url("/v2/images/test.jpg", true).unwrap(),
            "https://launchercontent.mojang.com/v2/images/test.jpg"
        );
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://www.minecraft.net.evil.test/a",
            "https://user@www.minecraft.net/a",
            "https://www.minecraft.net:444/a",
            "http://www.minecraft.net/a",
        ] {
            assert!(allowed_url(url, false).is_none(), "{url}");
        }
        assert!(allowed_url("//evil.test/image.png", true).is_none());
    }

    #[test]
    fn merges_both_schemas_in_chronological_order_with_distinct_ids() {
        let news = normalize(
            &cache(vec![
                news("same", "2026-09-05"),
                news("older", "2024-01-16"),
                news("bad", "2024-02-31"),
            ]),
            Source::News,
        )
        .unwrap();
        let patches = normalize(
            &cache(vec![patch("same", "2026-09-11T12:32:17.471Z")]),
            Source::JavaPatchNotes,
        )
        .unwrap();
        let merged = combine([Ok(news), Ok(patches)]).unwrap();
        assert_eq!(merged.entries.len(), 3);
        assert_eq!(merged.entries[0].id, "patch:same");
        assert_eq!(merged.entries[1].id, "news:same");
        assert_eq!(merged.entries[0].summary, "Blocks & items");
        assert_eq!(merged.entries[1].summary, "<script>plain text</script>");
        assert!(merged.warning.is_some());
        assert_eq!(
            normalize_date("2026-09-11T14:32:17.471+02:00").unwrap(),
            "2026-09-11T12:32:17.471Z"
        );
    }

    #[test]
    fn deduplicates_and_rejects_unreadable_feeds() {
        let result = normalize(
            &cache(vec![news("one", "2024-01-16"), news("one", "2024-01-16")]),
            Source::News,
        )
        .unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(result.warning.is_some());
        assert!(normalize(&cache(vec![json!({})]), Source::News).is_err());
        assert!(normalize(&cache(vec![]), Source::News)
            .unwrap()
            .entries
            .is_empty());
    }

    #[test]
    fn partial_failure_preserves_the_other_feed_and_uses_its_own_cache() {
        let path =
            std::env::temp_dir().join(format!("mona-v2-news-test-{}.json", std::process::id()));
        let cached = cache(vec![patch("saved", "2026-09-01T14:50:12.860Z")]);
        write_atomic(&path, &serde_json::to_vec(&cached).unwrap()).unwrap();
        let patches =
            accept_fetched(Source::JavaPatchNotes, Some(&path), Err("Offline".into())).unwrap();
        assert!(patches.cached);
        assert_eq!(patches.fetched_at, 42);
        let latest_news = normalize(&cache(vec![news("new", "2026-09-05")]), Source::News).unwrap();
        let merged = combine([Ok(latest_news), Ok(patches)]).unwrap();
        assert_eq!(merged.entries.len(), 2);
        assert!(merged.cached);
        assert!(merged.warning.unwrap().contains("保存済み"));
        write_atomic(&path, b"invalid json").unwrap();
        assert!(read_cache(&path, Source::JavaPatchNotes).is_none());
        std::fs::remove_file(path).unwrap();
        let only_news = normalize(&cache(vec![news("only", "2026-09-05")]), Source::News).unwrap();
        assert_eq!(
            combine([Ok(only_news), Err("patch unavailable".into())])
                .unwrap()
                .entries
                .len(),
            1
        );
        assert!(combine([Err("news offline".into()), Err("patch offline".into())]).is_err());
    }

    #[test]
    #[ignore = "requires the live Mojang v2 services"]
    fn live_mojang_feeds() {
        let results = [Source::News, Source::JavaPatchNotes].map(|source| {
            let result = normalize(&fetch(source).unwrap(), source).unwrap();
            eprintln!(
                "{}: {} entries; first: {} ({})",
                source.name(),
                result.entries.len(),
                result.entries[0].title,
                result.entries[0].date
            );
            assert!(result.warning.is_none(), "{:?}", result.warning);
            Ok(result)
        });
        let result = combine(results).unwrap();
        assert!(result
            .entries
            .iter()
            .any(|entry| entry.kind == Source::News));
        assert!(result
            .entries
            .iter()
            .any(|entry| entry.kind == Source::JavaPatchNotes));
    }
}
