#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
pub mod narrator_broker;

#[cfg(target_os = "linux")]
pub mod linux;
