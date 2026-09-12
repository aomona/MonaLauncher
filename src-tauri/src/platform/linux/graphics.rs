//! Minimal libdrm discovery metadata for the render nodes already granted to the game.
//! Reconstruct directories/links and bind individual attributes, never a device subtree.
use std::{collections::HashSet, fs, io, os::unix::fs::MetadataExt, path::Path, process::Command};

#[derive(Default)]
pub(super) struct Metadata {
    exposed: HashSet<std::path::PathBuf>,
}
impl Metadata {
    pub(super) fn expose(&mut self, command: &mut Command, render: &Path) -> io::Result<()> {
        let dev = fs::metadata(render)?.rdev();
        let char_path = std::path::PathBuf::from(format!(
            "/sys/dev/char/{}:{}",
            libc::major(dev),
            libc::minor(dev)
        ));
        // Software-only/older drivers may have no sysfs discovery metadata.
        if !char_path.exists() {
            return Ok(());
        }
        let node = fs::canonicalize(&char_path)?;
        let device = fs::canonicalize(node.join("device"))?;
        require_device_path(&device)?;
        self.device(command, &device)?;
        // libdrm follows virtio devices to their underlying PCI parent.
        if fs::read_link(device.join("subsystem"))?
            .file_name()
            .is_some_and(|n| n == "virtio")
        {
            let parent = device
                .parent()
                .ok_or_else(|| io::Error::other("missing virtio parent"))?;
            require_device_path(parent)?;
            self.device(command, parent)?;
        }
        // A compositor may advertise a primary dev_t. Publish its identification,
        // but never its /dev/dri/card* device, connectors, configuration or resources.
        for entry in fs::read_dir(device.join("drm"))? {
            let entry = entry?;
            if !drm_node_name(&entry.file_name().to_string_lossy()) {
                continue;
            }
            let path = fs::canonicalize(entry.path())?;
            if path.parent() != Some(device.join("drm").as_path()) {
                continue;
            }
            self.dir(command, &path);
            self.attributes(command, &path, &["dev", "uevent"])?;
            self.link(command, &device, &path.join("device"));
            self.link(
                command,
                Path::new("/sys/class/drm"),
                &path.join("subsystem"),
            );
            let number = fs::read_to_string(path.join("dev"))?;
            if !device_number(number.trim()) {
                return Err(io::Error::other("invalid DRM device number"));
            }
            self.link(
                command,
                &path,
                &Path::new("/sys/dev/char").join(number.trim()),
            );
            self.link(
                command,
                &path,
                &Path::new("/sys/class/drm").join(entry.file_name()),
            );
        }
        Ok(())
    }
    fn device(&mut self, command: &mut Command, device: &Path) -> io::Result<()> {
        self.dir(command, &device.join("drm"));
        self.attributes(
            command,
            device,
            &[
                "uevent",
                "vendor",
                "device",
                "subsystem_vendor",
                "subsystem_device",
                "revision",
            ],
        )?;
        // readlink is enough for libdrm's bus classification; the host bus tree stays hidden.
        let subsystem = fs::read_link(device.join("subsystem"))?;
        self.link(command, &subsystem, &device.join("subsystem"));
        Ok(())
    }
    fn attributes(&mut self, command: &mut Command, dir: &Path, names: &[&str]) -> io::Result<()> {
        for name in names {
            let path = dir.join(name);
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() => {
                    if self.exposed.insert(path.clone()) {
                        command.arg("--ro-bind").arg(&path).arg(&path);
                    }
                }
                Ok(_) => return Err(io::Error::other("unexpected non-file GPU attribute")),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    fn dir(&mut self, command: &mut Command, path: &Path) {
        if self.exposed.insert(path.to_owned()) {
            command.arg("--dir").arg(path);
        }
    }
    fn link(&mut self, command: &mut Command, target: &Path, path: &Path) {
        if self.exposed.insert(path.to_owned()) {
            command.arg("--symlink").arg(target).arg(path);
        }
    }
}
fn require_device_path(path: &Path) -> io::Result<()> {
    if !path.starts_with("/sys/devices") || path == Path::new("/sys/devices") {
        return Err(io::Error::other(
            "GPU metadata must resolve beneath /sys/devices",
        ));
    }
    Ok(())
}
fn drm_node_name(name: &str) -> bool {
    name.strip_prefix("renderD")
        .or_else(|| name.strip_prefix("card"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}
fn device_number(value: &str) -> bool {
    value.split_once(':').is_some_and(|(major, minor)| {
        [major, minor]
            .into_iter()
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_names_exclude_connectors_and_traversal() {
        assert!(drm_node_name("renderD128"));
        assert!(drm_node_name("card0"));
        for value in ["card0-DP-1", "card", "../card0", "renderD128/config"] {
            assert!(!drm_node_name(value));
        }
        assert!(device_number("226:128"));
        for value in ["226:0/../config", "226:", "226:0:1"] {
            assert!(!device_number(value));
        }
        assert!(require_device_path(Path::new("/sys/devices/pci0000:00/0000:00:02.0")).is_ok());
        assert!(require_device_path(Path::new("/etc")).is_err());
    }
}
