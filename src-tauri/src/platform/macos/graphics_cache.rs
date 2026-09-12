//! Metal chooses a Darwin cache path from the Java bundle identifier, not HOME/TMPDIR.
use std::{ffi::CStr, io, path::PathBuf};

pub fn java_metal_cache() -> io::Result<PathBuf> {
    // SAFETY: confstr reports the required buffer size; the second call receives that size.
    let length = unsafe { libc::confstr(libc::_CS_DARWIN_USER_CACHE_DIR, std::ptr::null_mut(), 0) };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut bytes = vec![0u8; length];
    // SAFETY: bytes owns length writable bytes, matching the size supplied to confstr.
    let written = unsafe {
        libc::confstr(
            libc::_CS_DARWIN_USER_CACHE_DIR,
            bytes.as_mut_ptr().cast(),
            length,
        )
    };
    if written == 0 || written > length {
        return Err(io::Error::other("Darwin cache path changed"));
    }
    let text = CStr::from_bytes_until_nul(&bytes).map_err(io::Error::other)?;
    use std::os::unix::ffi::OsStrExt;
    let parent = std::fs::canonicalize(std::ffi::OsStr::from_bytes(text.to_bytes()))?;
    let java = crate::sandbox::runtime_cache::owned_directory(&parent, "net.java.openjdk.java")
        .map_err(io::Error::other)?;
    let metal = crate::sandbox::runtime_cache::owned_directory(&java, "com.apple.metal")
        .map_err(io::Error::other)?;
    crate::sandbox::game_files::validate_tree(&metal).map_err(io::Error::other)?;
    Ok(metal)
}
