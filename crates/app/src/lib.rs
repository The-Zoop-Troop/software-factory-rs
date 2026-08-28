//! Factory workflows and the ports (traits) they depend on.
//!
//! Depends on `domain` only. Ports are implemented in `infra`:
//! `BeadStore` (the `bd` CLI), `Clock`. Later: `Repo` (git worktrees) and
//! `Harness` (LLM agent runner).
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

pub mod bead;
pub mod ports;
pub mod transition;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use bead::{Bead, BeadStatus, NewBead};
pub use domain;
pub use ports::{BeadStore, Clock, StoreError};
pub use transition::{TransitionError, apply_event, load_task};
