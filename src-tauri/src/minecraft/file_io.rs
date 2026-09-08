use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Reads a regular, non-link file without allowing an unbounded allocation.
pub fn read_bounded_file(path: &Path, maximum: u64) -> std::io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "refusing to read a non-regular or linked file: {}",
                path.display()
            ),
        ));
    }
    if metadata.len() > maximum {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("file exceeds the {maximum} byte limit: {}", path.display()),
        ));
    }

    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    File::open(path)?
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("file exceeds the {maximum} byte limit: {}", path.display()),
        ));
    }
    Ok(bytes)
}

/// Writes a complete file and atomically publishes it at `path`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("file has no parent directory: {}", path.display()),
        )
    })?;
    fs::create_dir_all(parent)?;

    let temporary = create_temporary_file_path(path);
    let result = (|| {
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        output.write_all(bytes)?;
        output.flush()?;
        output.sync_all()?;
        drop(output);
        replace_file_atomic(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn create_temporary_file_path(path: &Path) -> PathBuf {
    let counter = TEMPORARY_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = path
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_else(|| std::ffi::OsString::from("data"));
    name.push(format!(".{}.{}.tmp", std::process::id(), counter));
    path.with_file_name(name)
}

/// Atomically publishes an already-written file, replacing the destination if it exists.
#[cfg(windows)]
pub fn replace_file_atomic(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    // SAFETY: both UTF-16 buffers are NUL terminated and remain alive for the call.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| std::io::Error::last_os_error())
}

#[cfg(not(windows))]
pub fn replace_file_atomic(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

pub fn path_is_link_or_reparse(path: &Path) -> std::io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(metadata.file_type().is_symlink() || is_reparse_point(&metadata))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "monalauncher-file-io-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn atomically_replaces_existing_content() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("state.json");
        write_atomic(&target, b"old").unwrap();
        write_atomic(&target, b"new state").unwrap();

        assert_eq!(read_bounded_file(&target, 32).unwrap(), b"new state");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_files_over_the_limit() {
        let directory = temporary_directory();
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("large.bin");
        fs::write(&target, b"12345").unwrap();

        assert_eq!(
            read_bounded_file(&target, 4).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
