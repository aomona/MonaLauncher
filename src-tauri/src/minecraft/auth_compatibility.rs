//! Compatibility checks are not a trust boundary against game/Mod code. Secrets stay in Rust.
use super::{
    fabric::FabricProfile,
    file_io::read_bounded_file,
    model::{ModLoader, VersionMetadata},
    paths::MinecraftPaths,
};
use crate::auth::broker::service::UserAttributesSchema;
use sha2::{Digest, Sha256};

pub(super) struct Adapter {
    minecraft: &'static str,
    java: u32,
    authlib: &'static str,
    path: &'static str,
    sha1: &'static str,
    sha256: &'static str,
    size: u64,
    pub schema: UserAttributesSchema,
}
const ADAPTERS: [Adapter; 2] = [
    Adapter {
        minecraft: "1.21.8",
        java: 21,
        authlib: "com.mojang:authlib:6.0.58",
        path: "com/mojang/authlib/6.0.58/authlib-6.0.58.jar",
        sha1: "9261a6d53e629469fab20136b968bf5202d9c0f7",
        sha256: "7bea5444e83c8d343e11fb9e45939721f0db4321c5056ac846072e4a0bbe1321",
        size: 115810,
        schema: UserAttributesSchema::Authlib6,
    },
    Adapter {
        minecraft: "26.2",
        java: 25,
        authlib: "com.mojang:authlib:9.0.75",
        path: "com/mojang/authlib/9.0.75/authlib-9.0.75.jar",
        sha1: "d61056a234d5e4b272e09d59b0713f80d6c0b6af",
        sha256: "1f77e70240548b9cd233da0e12938bbbe5597ac9e94f6c6b07577faf1461a951",
        size: 145285,
        schema: UserAttributesSchema::Authlib9,
    },
];
const SUPPORTED_FABRIC: &str = "0.19.5";

pub(super) fn validate(
    version_id: &str,
    version: &VersionMetadata,
    java: u32,
    loader: &ModLoader,
    fabric: Option<&FabricProfile>,
) -> Result<&'static Adapter, String> {
    let adapter = ADAPTERS
        .iter()
        .find(|entry| entry.minecraft == version_id)
        .ok_or("認証仲介はMinecraft 1.21.8・26.2でのみ試験対応しています")?;
    if version.id != version_id || version.main_class != "net.minecraft.client.main.Main" {
        return Err("認証仲介に対応する公式Minecraftの起動構成と一致しません".into());
    }
    if java != adapter.java
        || version.java_version.as_ref().map(|v| v.major_version) != Some(adapter.java)
    {
        return Err(format!(
            "Minecraft {version_id}の認証仲介にはJava {}が必要です",
            adapter.java
        ));
    }
    let libraries: Vec<_> = version
        .libraries
        .iter()
        .filter(|library| artifact(&library.name, "com.mojang", "authlib"))
        .collect();
    if libraries.len() != 1
        || libraries[0].name != adapter.authlib
        || libraries[0].rules.is_some()
        || !libraries[0]
            .downloads
            .artifact
            .as_ref()
            .is_some_and(|file| {
                file.path.as_deref() == Some(adapter.path)
                    && file.sha1 == adapter.sha1
                    && file.size == adapter.size
            })
    {
        return Err("認証仲介に対応するauthlibの構成と一致しません。インスタンスを再インストールしてください".into());
    }
    match (loader, fabric) {
        (ModLoader::Vanilla, None) => {}
        (
            ModLoader::Fabric {
                version: loader_version,
            },
            Some(profile),
        ) if loader_version == SUPPORTED_FABRIC => {
            // Fabric jars precede the official classpath, so reject authlib replacement there.
            let loaders: Vec<_> = profile
                .libraries
                .iter()
                .filter(|library| artifact(&library.name, "net.fabricmc", "fabric-loader"))
                .collect();
            if profile.inherits_from != version_id
                || profile.main_class != "net.fabricmc.loader.impl.launch.knot.KnotClient"
                || loaders.len() != 1
                || loaders[0].name != "net.fabricmc:fabric-loader:0.19.5"
                || profile
                    .libraries
                    .iter()
                    .any(|library| artifact(&library.name, "com.mojang", "authlib"))
            {
                return Err("認証仲介に対応するFabricのライブラリ構成と一致しません".into());
            }
        }
        _ => return Err("認証仲介はVanillaまたはFabric 0.19.5でのみ試験対応しています".into()),
    }
    Ok(adapter)
}

fn artifact(coordinate: &str, group: &str, name: &str) -> bool {
    let mut fields = coordinate.split(':');
    fields.next() == Some(group) && fields.next() == Some(name)
}

pub(super) fn verify_authlib(paths: &MinecraftPaths, adapter: &Adapter) -> Result<(), String> {
    let bytes =
        read_bounded_file(&paths.libraries().join(adapter.path), adapter.size).map_err(|_| {
            "認証仲介用authlibを読み取れません。インスタンスを再インストールしてください"
        })?;
    if bytes.len() as u64 != adapter.size
        || format!("{:x}", Sha256::digest(&bytes)) != adapter.sha256
    {
        return Err(
            "認証仲介用authlibの検証に失敗しました。インスタンスを再インストールしてください"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn metadata(adapter: &Adapter) -> VersionMetadata {
        serde_json::from_value(json!({
            "id":adapter.minecraft, "type":"release", "mainClass":"net.minecraft.client.main.Main", "assets":"probe",
            "assetIndex":{"id":"probe","url":"https://example.invalid/index","sha1":"0".repeat(40)},
            "downloads":{"client":{"url":"https://example.invalid/client","sha1":"0".repeat(40),"size":0}},
            "javaVersion":{"majorVersion":adapter.java},
            "libraries":[{"name":adapter.authlib,"downloads":{"artifact":{"path":adapter.path,"sha1":adapter.sha1,"size":adapter.size,"url":"https://example.invalid/authlib"}}}]
        })).unwrap()
    }
    fn fabric(adapter: &Adapter) -> FabricProfile {
        serde_json::from_value(json!({"id":format!("fabric-loader-0.19.5-{}",adapter.minecraft),"inheritsFrom":adapter.minecraft,
            "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient", "libraries":[{"name":"net.fabricmc:fabric-loader:0.19.5"}]})).unwrap()
    }
    #[test]
    fn accepts_known_combinations_and_rejects_java_and_metadata_drift() {
        for adapter in &ADAPTERS {
            let original = metadata(adapter);
            assert!(validate(
                adapter.minecraft,
                &original,
                adapter.java,
                &ModLoader::Vanilla,
                None
            )
            .is_ok());
            assert!(validate(
                adapter.minecraft,
                &original,
                adapter.java + 1,
                &ModLoader::Vanilla,
                None
            )
            .is_err());
            assert!(validate(
                "unknown",
                &original,
                adapter.java,
                &ModLoader::Vanilla,
                None
            )
            .is_err());
            for mutation in 0..7 {
                let mut version = original.clone();
                match mutation {
                    0 => version.id = "different".into(),
                    1 => version.java_version = None,
                    2 => version.libraries.clear(),
                    3 => version.libraries.push(version.libraries[0].clone()),
                    4 => version.libraries[0].name = "com.mojang:authlib:unknown".into(),
                    5 => {
                        version.libraries[0]
                            .downloads
                            .artifact
                            .as_mut()
                            .unwrap()
                            .path = Some("alternate.jar".into())
                    }
                    _ => {
                        version.libraries[0]
                            .downloads
                            .artifact
                            .as_mut()
                            .unwrap()
                            .sha1 = "0".repeat(40)
                    }
                }
                assert!(validate(
                    adapter.minecraft,
                    &version,
                    adapter.java,
                    &ModLoader::Vanilla,
                    None
                )
                .is_err());
            }
        }
    }
    #[test]
    fn rejects_loader_drift_and_authlib_shadowing() {
        let adapter = &ADAPTERS[0];
        let version = metadata(adapter);
        let loader = ModLoader::Fabric {
            version: SUPPORTED_FABRIC.into(),
        };
        let profile = fabric(adapter);
        assert!(validate(
            adapter.minecraft,
            &version,
            adapter.java,
            &loader,
            Some(&profile)
        )
        .is_ok());
        assert!(validate(adapter.minecraft, &version, adapter.java, &loader, None).is_err());
        assert!(validate(
            adapter.minecraft,
            &version,
            adapter.java,
            &ModLoader::Fabric {
                version: "0.19.6".into()
            },
            Some(&profile)
        )
        .is_err());
        for replacement in [
            "com.mojang:authlib:9.0.75",
            "com.mojang:authlib:6.0.58:alternate",
            "net.fabricmc:fabric-loader:0.19.6",
        ] {
            let mut profile = profile.clone();
            profile
                .libraries
                .push(serde_json::from_value(json!({"name":replacement})).unwrap());
            assert!(validate(
                adapter.minecraft,
                &version,
                adapter.java,
                &loader,
                Some(&profile)
            )
            .is_err());
        }
    }
    #[test]
    fn rejects_missing_and_modified_jar_even_with_matching_metadata() {
        let temporary = tempfile::tempdir().unwrap();
        let paths = MinecraftPaths::new(temporary.path().to_owned());
        let adapter = &ADAPTERS[0];
        assert!(verify_authlib(&paths, adapter).is_err());
        let jar = paths.libraries().join(adapter.path);
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        std::fs::write(&jar, vec![0; adapter.size as usize]).unwrap();
        assert!(verify_authlib(&paths, adapter).is_err());
        std::fs::write(&jar, vec![0; adapter.size as usize + 1]).unwrap();
        assert!(verify_authlib(&paths, adapter).is_err());
    }
}
