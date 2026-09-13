//! Credential-bearing operations live outside the game process. The Java adapter is untrusted.
#[cfg(unix)]
pub mod channel;
pub mod chat;
pub mod protocol;
pub mod service;

#[cfg(all(test, unix))]
mod interop;
