#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(any(windows, target_os = "macos"))]
pub mod narrator_broker;
