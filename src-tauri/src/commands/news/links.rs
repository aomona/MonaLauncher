/// The patch index contains contentPath (a JSON document), not a browser article URL.
/// Use Minecraft's version-specific article routes, including older naming exceptions.
pub(super) fn patch_article_url(version: &str, release_type: &str) -> Option<String> {
    if version.len() > 80
        || version.is_empty()
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return None;
    }
    let slug = match version {
        "1.14" => "village---pillage-out-java-".into(),
        "1.13" => return Some("https://feedback.minecraft.net/hc/en-us/articles/360007323492-Minecraft-Java-Edition-1-13-Update-Aquatic".into()),
        // The original article was updated to include the second pre-release.
        "1.15.2-pre2" => "minecraft-1-15-2-pre-release-1".into(),
        _ if release_type == "release" && numeric_version(version) =>
            format!("minecraft-java-edition-{}", version.replace('.', "-")),
        _ if release_type == "snapshot" => {
            if let Some((base, number)) = version.split_once("-rc") {
                numbered_article(base, number, "release-candidate")?
            } else if let Some((base, number)) = version.split_once("-pre") {
                numbered_article(base, number, "pre-release")?
            } else if let Some((base, number)) = version.split_once("-snapshot-") {
                numbered_article(base, number, "snapshot")?
            } else if version.len() == 6 && version.as_bytes()[2] == b'w'
                && version[..2].bytes().all(|b| b.is_ascii_digit())
                && version[3..5].bytes().all(|b| b.is_ascii_digit())
                && version.as_bytes()[5].is_ascii_lowercase() {
                format!("minecraft-snapshot-{version}")
            } else { return None; }
        }
        _ => return None,
    };
    Some(format!("https://www.minecraft.net/en-us/article/{slug}"))
}

fn numeric_version(value: &str) -> bool {
    value.split('.').count() >= 2
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn numbered_article(base: &str, number: &str, kind: &str) -> Option<String> {
    let number = number.strip_prefix('-').unwrap_or(number);
    (numeric_version(base) && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("minecraft-{}-{kind}-{number}", base.replace('.', "-")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_current_and_legacy_version_routes() {
        for (version, kind, slug) in [
            (
                "26.3-rc-2",
                "snapshot",
                "minecraft-26-3-release-candidate-2",
            ),
            ("26.3-pre-3", "snapshot", "minecraft-26-3-pre-release-3"),
            ("26.3-snapshot-10", "snapshot", "minecraft-26-3-snapshot-10"),
            ("26.2", "release", "minecraft-java-edition-26-2"),
            (
                "1.21.11-rc3",
                "snapshot",
                "minecraft-1-21-11-release-candidate-3",
            ),
            ("25w46a", "snapshot", "minecraft-snapshot-25w46a"),
            ("1.15.2-pre2", "snapshot", "minecraft-1-15-2-pre-release-1"),
        ] {
            assert_eq!(
                patch_article_url(version, kind).unwrap(),
                format!("https://www.minecraft.net/en-us/article/{slug}")
            );
        }
        assert!(patch_article_url("../secret", "release").is_none());
        assert!(patch_article_url("https://evil.test", "snapshot").is_none());
        assert!(patch_article_url("unknown", "snapshot").is_none());
    }
}
