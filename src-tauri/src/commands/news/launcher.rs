use super::{Feed, NewsEntry, Source, MAX_BYTES};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::Url;
use roxmltree::{Document, Node, ParsingOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const CONTENT_NS: &str = "http://purl.org/rss/1.0/modules/content/";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";

pub(super) fn site_url() -> Result<Url, String> {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../../../news.config.json"))
            .map_err(|e| e.to_string())?;
    let value = option_env!("NEWS_SITE_URL")
        .filter(|value| !value.is_empty())
        .or_else(|| config["siteUrl"].as_str())
        .ok_or("ニュースのsiteUrlが設定されていません。")?;
    let mut url = Url::parse(value).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "ニュースのsiteUrlは認証情報・query・fragmentのないHTTPS URLが必要です。".into(),
        );
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

#[derive(Deserialize, Serialize)]
struct Article {
    id: String,
    title: String,
    summary: String,
    date: String,
    url: String,
    content: String,
    author: Option<String>,
}

fn field(node: Node<'_, '_>, namespace: Option<&str>, name: &str) -> Result<String, String> {
    let mut matches = node.children().filter(|child| {
        child.is_element()
            && child.tag_name().name() == name
            && child.tag_name().namespace() == namespace
    });
    let element = matches
        .next()
        .ok_or_else(|| format!("{name}がありません。"))?;
    if matches.next().is_some() || element.children().any(|child| child.is_element()) {
        return Err(format!("{name}の形式が不正です。"));
    }
    let value: String = element
        .children()
        .filter(|child| child.is_text())
        .filter_map(|child| child.text())
        .collect();
    if value.trim().is_empty() {
        return Err(format!("{name}が空です。"));
    }
    Ok(value)
}

pub(super) fn normalize(value: &serde_json::Value) -> Result<NewsEntry, String> {
    let article: Article = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let base = site_url()?.join("news/").map_err(|e| e.to_string())?;
    let url = Url::parse(&article.url).map_err(|e| e.to_string())?;
    if url.origin() != base.origin()
        || !url.path().starts_with(base.path())
        || !url.username().is_empty()
        || url.password().is_some()
        || article.id.trim().is_empty()
        || article.title.trim().is_empty()
        || article.summary.trim().is_empty()
        || article.content.trim().is_empty()
    {
        return Err("記事のURLまたは必須項目が不正です。".into());
    }
    let date = super::normalize_date(&article.date).ok_or("記事の日付が不正です。")?;
    Ok(NewsEntry {
        id: format!("launcher:{}", article.id),
        title: article.title,
        summary: article.summary,
        category: "MonaLauncher".into(),
        date,
        kind: Source::MonaLauncher,
        article_url: url.to_string(),
        image_url: None,
        content_html: Some(article.content),
        author: article.author,
    })
}

pub(super) fn parse(bytes: &[u8]) -> Result<Feed, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("RSSのサイズが上限を超えています。".into());
    }
    let xml = std::str::from_utf8(bytes).map_err(|_| "RSSがUTF-8ではありません。")?;
    let doc = Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .map_err(|e| format!("RSSを解析できませんでした: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("rss") || root.attribute("version") != Some("2.0") {
        return Err("RSS 2.0形式ではありません。".into());
    }
    let channel = root
        .children()
        .find(|node| node.has_tag_name("channel"))
        .ok_or("RSS channelがありません。")?;
    let mut entries = Vec::new();
    let mut ids = HashSet::new();
    for (index, item) in channel
        .children()
        .filter(|node| node.has_tag_name("item"))
        .enumerate()
    {
        let parse_item = || -> Result<serde_json::Value, String> {
            if index >= 2000 {
                return Err("記事数が上限を超えています。".into());
            }
            let date = DateTime::parse_from_rfc2822(&field(item, None, "pubDate")?)
                .map_err(|_| "pubDateが不正です。")?
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true);
            let article = Article {
                id: field(item, None, "guid")?,
                title: field(item, None, "title")?,
                summary: field(item, None, "description")?,
                date,
                url: field(item, None, "link")?,
                content: field(item, Some(CONTENT_NS), "encoded")?,
                author: field(item, Some(DC_NS), "creator").ok(),
            };
            let value = serde_json::to_value(article).map_err(|e| e.to_string())?;
            normalize(&value)?;
            Ok(value)
        };
        let value = parse_item().map_err(|e| format!("RSS item {}: {e}", index + 1))?;
        if !ids.insert(value["id"].as_str().unwrap_or_default().to_owned()) {
            return Err(format!("RSS item {}: guidが重複しています。", index + 1));
        }
        entries.push(value);
    }
    Ok(Feed {
        version: 1,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::news::{accept_fetched, combine, Cache};

    fn rss(body: &str) -> String {
        format!(
            r#"<rss version="2.0" xmlns:content="{CONTENT_NS}" xmlns:dc="{DC_NS}"><channel>{body}</channel></rss>"#
        )
    }

    fn item() -> String {
        format!(
            r#"<item><title><![CDATA[日本語 & 更新]]></title><guid>example</guid><link>{}news/example/</link><description>説明 &amp; 要約</description><pubDate>Sat, 12 Sep 2026 09:00:00 +0900</pubDate><dc:creator>著者</dc:creator><content:encoded><![CDATA[<p>日本語の<strong>本文</strong></p>]]></content:encoded></item>"#,
            site_url().unwrap()
        )
    }

    #[test]
    fn reads_rss_html_and_caches_it_for_offline_reading() {
        let feed = parse(rss(&item()).as_bytes()).unwrap();
        let cache = Cache {
            feed,
            fetched_at: 42,
        };
        let path =
            std::env::temp_dir().join(format!("mona-launcher-rss-{}.json", std::process::id()));
        let fresh = accept_fetched(Source::MonaLauncher, Some(&path), Ok(cache)).unwrap();
        assert_eq!(fresh.entries[0].date, "2026-09-12T00:00:00.000Z");
        assert_eq!(fresh.entries[0].title, "日本語 & 更新");
        assert_eq!(fresh.entries[0].summary, "説明 & 要約");
        assert_eq!(fresh.entries[0].author.as_deref(), Some("著者"));
        let cached =
            accept_fetched(Source::MonaLauncher, Some(&path), Err("offline".into())).unwrap();
        assert!(cached.cached);
        assert_eq!(
            cached.entries[0].content_html,
            fresh.entries[0].content_html
        );
        let merged = combine([Ok(cached), Err("Mojang unavailable".into())]).unwrap();
        assert_eq!(merged.entries.len(), 1);
        assert!(merged.warning.unwrap().contains("Mojang unavailable"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_invalid_xml_entities_metadata_and_article_destinations() {
        let valid = rss(&item());
        for invalid in [
            "<rss>".to_string(),
            format!("<!DOCTYPE rss [<!ENTITY x SYSTEM 'file:///etc/passwd'>]>{valid}"),
            valid.replace("09:00:00 +0900", "invalid"),
            valid.replace("<title>", "<missing>"),
            valid.replace(&site_url().unwrap().to_string(), "https://evil.test/"),
            valid.replace(CONTENT_NS, "https://evil.test/content"),
            rss(&format!("{}{}", item(), item())),
        ] {
            assert!(parse(invalid.as_bytes()).is_err(), "{invalid}");
        }
        assert!(parse(rss("").as_bytes()).unwrap().entries.is_empty());
    }

    #[test]
    #[ignore = "run after pnpm generate:feed to check the actual generated RSS"]
    fn generated_launcher_feed() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/rss.xml");
        let feed = parse(&std::fs::read(path).unwrap()).unwrap();
        assert!(!feed.entries.is_empty());
        for value in feed.entries {
            let article = normalize(&value).unwrap();
            assert!(article.content_html.is_some());
        }
    }

    #[test]
    #[ignore = "requires the published MonaLauncher RSS service"]
    fn live_launcher_feed() {
        let cache = super::super::fetch(Source::MonaLauncher).unwrap();
        let feed = super::super::normalize(&cache, Source::MonaLauncher).unwrap();
        assert!(feed.warning.is_none());
        eprintln!("Live MonaLauncher RSS: {} articles", feed.entries.len());
    }
}
