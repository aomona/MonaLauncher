use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestVersions {
    pub release: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    pub url: String,
    pub sha1: String,
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
    pub arguments: Arguments,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadInfo {
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
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
}
