use std::{fs, path::Path};

use super::{
    file_io::{path_is_link_or_reparse, write_atomic},
    installer::{load_instance, validate_instance_id, validate_instance_name},
    model::InstanceManifest,
    paths::MinecraftPaths,
};

/// Caller reserves both instance IDs and rejects a running source before copying.
pub fn duplicate_instance(
    paths: &MinecraftPaths,
    source_id: &str,
    new_id: &str,
    name: &str,
) -> Result<InstanceManifest, String> {
    validate_instance_id(new_id).map_err(|e| e.to_string())?;
    let name = validate_instance_name(name).map_err(|e| e.to_string())?;
    let mut instance = load_instance(paths, source_id).map_err(|e| e.to_string())?;
    if !instance.sandboxed
        || path_is_link_or_reparse(&paths.instances()).map_err(|e| e.to_string())?
    {
        return Err("安全でないインスタンスは複製できません".into());
    }
    let destination = paths.instance(new_id);
    // create_dir is exclusive: never overwrite or clean up an existing destination.
    fs::create_dir(&destination).map_err(|e| e.to_string())?;
    let result = (|| {
        let game = paths.instance_game_directory(new_id);
        fs::create_dir(&game).map_err(|e| e.to_string())?;
        let source_game = Path::new(&instance.game_directory);
        if source_game.exists() {
            for entry in fs::read_dir(source_game).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                // Regeneratable chat private keys must never be copied into a new instance.
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case("profilekeys"))
                {
                    continue;
                }
                copy_checked(&entry.path(), &game.join(entry.file_name()))?;
            }
        }
        // Shared Java/assets/libraries remain shared. Launch files, staging, and caches are rebuilt.
        for filename in ["fabric-profile.json", "modrinth-mods.json"] {
            let source = paths.instance(source_id).join(filename);
            match fs::symlink_metadata(&source) {
                Ok(metadata) if metadata.is_file() => {
                    copy_checked(&source, &destination.join(filename))?
                }
                Ok(_) => return Err(format!("複製元のファイル形式が不正です: {filename}")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        instance.id = new_id.to_owned();
        instance.name = name.to_owned();
        instance.game_directory = game.to_string_lossy().into_owned();
        // Publish last, so partial copies never appear in the instance list.
        write_atomic(
            &paths.instance_manifest(new_id),
            &serde_json::to_vec_pretty(&instance).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(instance)
    })();
    if result.is_err() {
        fs::remove_dir_all(&destination)
            .map_err(|e| format!("複製に失敗し、一時データも削除できませんでした: {e}"))?;
    }
    result
}

fn copy_checked(source: &Path, destination: &Path) -> Result<(), String> {
    crate::sandbox::game_files::validate_tree(source).map_err(|e| e.to_string())?;
    let mut pending = vec![(source.to_owned(), destination.to_owned())];
    while let Some((source, destination)) = pending.pop() {
        if path_is_link_or_reparse(&source).map_err(|e| e.to_string())? {
            return Err("リンクを含むインスタンスは複製できません".into());
        }
        let metadata = fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            fs::create_dir(&destination).map_err(|e| e.to_string())?;
            for entry in fs::read_dir(&source).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                pending.push((entry.path(), destination.join(entry.file_name())));
            }
        } else if metadata.is_file() {
            let mut input = fs::File::open(&source).map_err(|e| e.to_string())?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(|e| e.to_string())?;
            std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        } else {
            return Err("通常ファイル以外は複製できません".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> MinecraftPaths {
        let paths = MinecraftPaths::new(root.to_owned());
        let java = paths
            .runtimes()
            .join("temurin-21")
            .join("a".repeat(64))
            .join("runtime/bin")
            .join(crate::minecraft::model::java_executable_name());
        fs::create_dir_all(java.parent().unwrap()).unwrap();
        fs::write(&java, "synthetic runtime").unwrap();
        fs::create_dir_all(paths.instance_game_directory("source").join("saves/world")).unwrap();
        let manifest = serde_json::json!({
            "id": "source", "name": "Original", "versionId": "1.21.8",
            "javaPath": java, "gameDirectory": paths.instance_game_directory("source"),
            "demo": false, "sandboxed": true,
            "modLoader": {"type": "fabric", "version": "0.16.0"}
        });
        fs::write(
            paths.instance_manifest("source"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        paths
    }

    #[test]
    fn copies_independent_game_data_and_metadata_but_not_launch_files_or_keys() {
        let root = tempfile::tempdir().unwrap();
        let paths = fixture(root.path());
        let game = paths.instance_game_directory("source");
        fs::write(game.join("saves/world/level.dat"), "world").unwrap();
        fs::create_dir(game.join("mods")).unwrap();
        fs::write(game.join("mods/example.jar"), "mod").unwrap();
        fs::create_dir(game.join("profilekeys")).unwrap();
        fs::write(
            game.join("profilekeys/synthetic.json"),
            "synthetic private material",
        )
        .unwrap();
        fs::write(paths.instance_fabric_profile("source"), "fabric profile").unwrap();
        fs::write(paths.instance_mod_registry("source"), "mod registry").unwrap();
        fs::create_dir(paths.instance("source").join("launch")).unwrap();
        let mut original = load_instance(&paths, "source").unwrap();
        original.permissions.network = true;
        fs::write(
            paths.instance_manifest("source"),
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
        let copied = duplicate_instance(&paths, "source", "copy", "Original (複製)").unwrap();
        assert_eq!(copied.id, "copy");
        assert_eq!(copied.permissions, original.permissions);
        assert_eq!(copied.mod_loader, original.mod_loader);
        assert_eq!(copied.java_path, original.java_path);
        let copy_game = paths.instance_game_directory("copy");
        assert_eq!(
            fs::read(copy_game.join("saves/world/level.dat")).unwrap(),
            b"world"
        );
        assert_eq!(
            fs::read(copy_game.join("mods/example.jar")).unwrap(),
            b"mod"
        );
        fs::write(copy_game.join("saves/world/level.dat"), "changed").unwrap();
        assert_eq!(
            fs::read(game.join("saves/world/level.dat")).unwrap(),
            b"world"
        );
        assert!(!copy_game.join("profilekeys").exists());
        assert!(!paths.instance("copy").join("launch").exists());
        assert_eq!(
            fs::read(paths.instance_fabric_profile("copy")).unwrap(),
            b"fabric profile"
        );
        assert_eq!(
            fs::read(paths.instance_mod_registry("copy")).unwrap(),
            b"mod registry"
        );
        assert_eq!(
            load_instance(&paths, "copy").unwrap().game_directory,
            copied.game_directory
        );
        assert!(duplicate_instance(&paths, "source", "copy", "Overwrite").is_err());
        assert!(duplicate_instance(&paths, "source", "../escape", "Escape").is_err());
        assert!(duplicate_instance(&paths, "source", "source", "Overwrite source").is_err());
        assert_eq!(
            fs::read(copy_game.join("saves/world/level.dat")).unwrap(),
            b"changed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_links_and_removes_partial_destination() {
        let root = tempfile::tempdir().unwrap();
        let paths = fixture(root.path());
        let outside = root.path().join("outside");
        fs::write(&outside, "outside").unwrap();
        let alias = paths.instance_game_directory("source").join("alias");
        std::os::unix::fs::symlink(&outside, &alias).unwrap();
        assert!(duplicate_instance(&paths, "source", "copy", "Copy").is_err());
        assert!(!paths.instance("copy").exists());
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&outside, &alias).unwrap();
        assert!(duplicate_instance(&paths, "source", "copy", "Copy").is_err());
        assert!(!paths.instance("copy").exists());
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
    }
}
