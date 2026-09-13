//! Credential-bearing operations live outside the game process. The Java adapter is untrusted.
#[cfg(unix)]
pub mod channel;
pub mod chat;
pub mod protocol;
pub mod service;

#[cfg(all(test, unix))]
mod interop;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod game_key_probe;

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod game_isolation_probe;
