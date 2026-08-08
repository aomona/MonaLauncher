use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMetadata {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub main_class: String,
    pub assets: String,
    pub asset_index: AssetIndexReference,
    pub downloads: VersionDownloads,
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Arguments,
    pub minecraft_arguments: Option<String>,
    pub java_version: Option<JavaVersion>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionDownloads {
    pub client: DownloadInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexReference {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub downloads: LibraryDownloads,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadInfo>,
    #[serde(default)]
    pub classifiers: HashMap<String, DownloadInfo>,
}

impl Library {
    pub fn windows_native(&self) -> Option<&DownloadInfo> {
        let classifier = self.natives.as_ref()?.get("windows")?;
        let classifier = classifier.replace("${arch}", windows_native_architecture());
        self.downloads.classifiers.get(&classifier)
    }
}

fn windows_native_architecture() -> &'static str {
    if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadInfo {
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Arguments {
    pub game: Vec<Argument>,
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<RuleOs>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleOs {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceManifest {
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub java_path: String,
    pub game_directory: String,
    pub demo: bool,
    #[serde(default)]
    pub sandboxed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub message: String,
}

pub fn rules_allow(rules: Option<&[Rule]>, features: &HashMap<String, bool>) -> bool {
    let Some(rules) = rules else {
        return true;
    };

    let mut allowed = false;

    for rule in rules {
        if rule_matches(rule, features) {
            allowed = rule.action == "allow";
        }
    }

    allowed
}

fn rule_matches(rule: &Rule, features: &HashMap<String, bool>) -> bool {
    if let Some(os) = &rule.os {
        if os.name.as_deref().is_some_and(|name| name != "windows") {
            return false;
        }

        if os
            .arch
            .as_deref()
            .is_some_and(|arch| !architecture_matches(arch))
        {
            return false;
        }

        // OSバージョン正規表現を指定する旧バージョンは、MVPでは対象外にする。
        if os.version.is_some() {
            return false;
        }
    }

    if let Some(required_features) = &rule.features {
        for (name, required_value) in required_features {
            if features.get(name).copied().unwrap_or(false) != *required_value {
                return false;
            }
        }
    }

    true
}

fn architecture_matches(expected: &str) -> bool {
    matches!(
        (std::env::consts::ARCH, expected),
        ("x86_64", "x86_64") | ("x86", "x86") | ("aarch64", "arm64")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_items_without_rules() {
        assert!(rules_allow(None, &HashMap::new()));
    }

    #[test]
    fn maps_the_official_version_catalog_for_tauri() {
        let catalog: VersionManifest = serde_json::from_str(
            r#"{
                "latest": { "release": "1.21.8", "snapshot": "25w31a" },
                "versions": [{
                    "id": "1.21.8",
                    "type": "release",
                    "url": "https://example.invalid/1.21.8.json",
                    "sha1": "0123456789012345678901234567890123456789",
                    "releaseTime": "2025-07-17T12:00:00+00:00"
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(catalog.latest.release, "1.21.8");
        assert_eq!(catalog.versions[0].version_type, "release");
        let tauri_payload = serde_json::to_value(catalog).unwrap();
        assert_eq!(
            tauri_payload["versions"][0]["releaseTime"],
            "2025-07-17T12:00:00+00:00"
        );
    }

    #[test]
    fn applies_demo_feature_rule() {
        let rule = Rule {
            action: "allow".to_owned(),
            os: None,
            features: Some(HashMap::from([("is_demo_user".to_owned(), true)])),
        };

        assert!(!rules_allow(
            Some(std::slice::from_ref(&rule)),
            &HashMap::new()
        ));
        assert!(rules_allow(
            Some(&[rule]),
            &HashMap::from([("is_demo_user".to_owned(), true)]),
        ));
    }

    #[test]
    fn rejects_non_windows_rule() {
        let rule = Rule {
            action: "allow".to_owned(),
            os: Some(RuleOs {
                name: Some("linux".to_owned()),
                arch: None,
                version: None,
            }),
            features: None,
        };

        assert!(!rules_allow(Some(&[rule]), &HashMap::new()));
    }

    #[test]
    fn selects_windows_native_classifier() {
        let native = DownloadInfo {
            path: Some("native.jar".to_owned()),
            sha1: "hash".to_owned(),
            size: 1,
            url: "https://example.invalid/native.jar".to_owned(),
        };
        let library = Library {
            downloads: LibraryDownloads {
                artifact: None,
                classifiers: HashMap::from([("natives-windows-64".to_owned(), native)]),
            },
            rules: None,
            natives: Some(HashMap::from([(
                "windows".to_owned(),
                "natives-windows-${arch}".to_owned(),
            )])),
        };

        assert_eq!(
            library
                .windows_native()
                .and_then(|item| item.path.as_deref()),
            Some("native.jar")
        );
    }
}
