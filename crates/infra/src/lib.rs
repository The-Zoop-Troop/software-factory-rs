//! Infrastructure adapters implementing the `app` ports.
//!
//! This is the only crate allowed to shell out (`bd`, `git`, `claude`) or
//! read the system clock.
#![forbid(unsafe_code)]
#![cfg_attr(
    any(test, feature = "testing"),
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::disallowed_methods
    )
)]

pub mod bd;
pub mod claude;
pub mod clock;
pub mod codex;
pub mod git;
pub mod jsonl;
pub mod opencode;
pub mod shell;

pub use app;

/// Classify a spawn/io failure from the OS into the ports' `Unavailable` cause.
#[must_use]
#[allow(
    clippy::wildcard_enum_match_arm,
    reason = "std::io::ErrorKind is #[non_exhaustive]; a catch-all is the only total match"
)]
pub fn classify_io(kind: std::io::ErrorKind) -> app::Unavailable {
    use app::Unavailable;
    use std::io::ErrorKind as K;
    match kind {
        K::NotFound | K::PermissionDenied => Unavailable::NotInstalled,
        K::ConnectionRefused
        | K::ConnectionReset
        | K::ConnectionAborted
        | K::NotConnected
        | K::BrokenPipe => Unavailable::Network,
        K::WouldBlock | K::TimedOut | K::Interrupted => Unavailable::Locked,
        _ => Unavailable::Io, // fp-allow: ErrorKind is non-exhaustive upstream
    }
}

pub use bd::BdCli;
pub use claude::ClaudeCli;
pub use clock::SystemClock;
pub use codex::CodexCli;
pub use git::GitCli;
pub use jsonl::JsonlSink;
pub use opencode::OpencodeServer;
pub use shell::ShellRunner;
