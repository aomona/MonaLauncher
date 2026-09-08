use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MinecraftPaths {
    root: PathBuf,
}

impl MinecraftPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn instance(&self, instance_id: &str) -> PathBuf {
        self.instances().join(instance_id)
    }

    pub fn instance_manifest(&self, instance_id: &str) -> PathBuf {
        self.instance(instance_id).join("instance.json")
    }

    pub fn instance_game_directory(&self, instance_id: &str) -> PathBuf {
        self.instance(instance_id).join("game")
    }

    pub fn instance_fabric_profile(&self, instance_id: &str) -> PathBuf {
        self.instance(instance_id).join("fabric-profile.json")
    }

    pub fn instance_mods_directory(&self, instance_id: &str) -> PathBuf {
        self.instance_game_directory(instance_id).join("mods")
    }

    pub fn instance_mod_registry(&self, instance_id: &str) -> PathBuf {
        self.instance(instance_id).join("modrinth-mods.json")
    }

    pub fn instance_mod_staging(&self, instance_id: &str) -> PathBuf {
        self.instance(instance_id).join("modrinth-staging")
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn asset_indexes(&self) -> PathBuf {
        self.assets().join("indexes")
    }

    pub fn asset_objects(&self) -> PathBuf {
        self.assets().join("objects")
    }

    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn runtimes(&self) -> PathBuf {
        self.root.join("runtimes")
    }

    pub fn version_directory(&self, version_id: &str) -> PathBuf {
        self.versions().join(version_id)
    }

    pub fn version_json(&self, version_id: &str) -> PathBuf {
        self.version_directory(version_id)
            .join(format!("{version_id}.json"))
    }

    pub fn version_jar(&self, version_id: &str) -> PathBuf {
        self.version_directory(version_id)
            .join(format!("{version_id}.jar"))
    }
}
