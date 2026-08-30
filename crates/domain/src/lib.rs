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
pub mod counts;
pub mod effort;
pub mod ids;
pub mod kind;
pub mod lease;
pub mod meta;
pub mod nonempty;
pub mod plan;
pub mod remote;
pub mod task;
pub mod text;
pub mod time;

pub use budget::{Budget, BudgetExceeded, Usage};
pub use counts::{Attempts, MicroUsd, Priority, PriorityError, Tokens, Turns};
pub use effort::{Effort, UnknownEffort};
pub use ids::{AgentId, BeadId, BranchName, ClientId, RigName, Sha};
pub use kind::BeadKind;
pub use lease::Lease;
pub use meta::{
    BeadMeta, CrossRigNeed, FactoryMeta, MergeMeta, MetaParseError, NEEDS_META_KEY, VerifyMeta,
};
pub use nonempty::{EmptyError, NonEmpty};
pub use plan::{Plan, PlanDefaults, PlanError, PlannedTask, RawPlan, RawPlannedTask, TaskKey};
pub use remote::{
    Forbidden, Principal, RigBudget, RigBudgetExceeded, RigSpend, Scope, UnknownScope, require,
};
pub use task::{Event, IllegalTransition, Task, TaskState, Transition};
pub use text::{Title, VerifyCommand};
pub use time::{Duration, Timestamp};
