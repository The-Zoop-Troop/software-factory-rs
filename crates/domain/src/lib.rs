//! Factory domain — the pure core.
//!
//! Bead conventions (ARCHITECTURE.md §3), the task state machine, budgets and
//! leases live here as types and total functions. No clock, no I/O, no async:
//! timestamps and IDs arrive as parameters.
#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::disallowed_methods
    )
)]

pub mod budget;
pub mod ids;
pub mod kind;
pub mod lease;
pub mod meta;
pub mod task;
pub mod time;

pub use budget::{Budget, BudgetExceeded, Usage};
pub use ids::{AgentId, BeadId, BranchName, Sha};
pub use kind::BeadKind;
pub use lease::Lease;
pub use meta::{BeadMeta, FactoryMeta, MergeMeta, MetaParseError, VerifyMeta};
pub use task::{Event, IllegalTransition, Task, TaskState, Transition};
pub use time::{Duration, Timestamp};
