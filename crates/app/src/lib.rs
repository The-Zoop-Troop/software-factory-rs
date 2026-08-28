//! Factory workflows and the ports (traits) they depend on.
//!
//! Depends on `domain` only. Ports are implemented in `infra`:
//! `BeadStore` (the `bd` CLI), `Repo` (git worktrees), `Harness` (LLM
//! agent runner), `Clock`.
#![forbid(unsafe_code)]
#![cfg_attr(any(test, feature = "testing"),
            allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing))]

pub use domain;
