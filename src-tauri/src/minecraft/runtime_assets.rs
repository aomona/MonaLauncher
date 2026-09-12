//! An immutable per-instance asset view gives Minecraft its own writable `skins` sibling.
//! Asset objects are hard-linked where possible; the game cannot write either alias.
use super::model::{AssetIndex, VersionMetadata};
use crate::sandbox::runtime_cache::owned_directory;
use std::{fs, io, path::Path};

pub fn prepare(shared: &Path, view: &Path, version: &VersionMetadata) -> io::Result<()> {
    let index_id = &version.asset_index.id;
    if index_id.is_empty()
        || !index_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(io::Error::other("invalid asset index identifier"));
    }
    let source = shared.join("indexes").join(format!("{index_id}.json"));
    let index: AssetIndex = serde_json::from_slice(&fs::read(&source)?)?;
    let indexes = owned_directory(view, "indexes").map_err(io::Error::other)?;
    let objects = owned_directory(view, "objects").map_err(io::Error::other)?;
    // An index identifier can be re-published. Refresh this small file atomically.
    let index_destination = indexes.join(format!("{index_id}.json"));
    materialize(&source, &index_destination)?;
    if fs::read(&index_destination)? != fs::read(&source)? {
        super::file_io::write_atomic(&index_destination, &fs::read(&source)?)?;
    }
    for object in index.objects.values() {
        let hash = &object.hash;
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::Error::other("invalid asset object hash"));
        }
        let directory = owned_directory(&objects, &hash[..2]).map_err(io::Error::other)?;
        materialize(
            &shared.join("objects").join(&hash[..2]).join(hash),
            &directory.join(hash),
        )?;
    }
    Ok(())
}

fn materialize(source: &Path, destination: &Path) -> io::Result<()> {
    let source_meta = fs::symlink_metadata(source)?;
    if !source_meta.is_file() || source_meta.file_type().is_symlink() {
        return Err(io::Error::other("asset source is not a regular file"));
    }
    match fs::symlink_metadata(destination) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(io::Error::other("asset view contains a reparse point"));
                }
            }
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(io::Error::other("asset view contains an alias"));
            }
            // Content-addressed objects are immutable. An interrupted copy is recreated.
            if metadata.len() == source_meta.len() {
                return Ok(());
            }
            fs::remove_file(destination)?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    if fs::hard_link(source, destination).is_err() {
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).map_err(|e| io::Error::other(e.to_string()))?;
        let temporary = destination.with_extension(format!("{nonce:x?}.copy"));
        let result = fs::copy(source, &temporary).and_then(|_| fs::rename(&temporary, destination));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_view_is_reused_and_rejects_destination_symlinks() {
        let root = std::env::temp_dir().join(format!("mona-assets-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        let destination = root.join("view");
        fs::write(&source, b"trusted asset").unwrap();
        materialize(&source, &destination).unwrap();
        materialize(&source, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"trusted asset");
        #[cfg(unix)]
        {
            fs::remove_file(&destination).unwrap();
            std::os::unix::fs::symlink(&source, &destination).unwrap();
            assert!(materialize(&source, &destination).is_err());
            assert_eq!(fs::read(&source).unwrap(), b"trusted asset");
        }
        fs::remove_dir_all(root).unwrap();
    }
}
