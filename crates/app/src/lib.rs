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
pub mod events;
pub mod integrator;
pub mod planner;
pub mod ports;
pub mod steward;
pub mod transition;
pub mod verifier;
pub mod worker;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use bead::{Bead, BeadStatus, NewBead};
pub use domain;
pub use events::{EventKind, FactoryEvent};
pub use integrator::{IntegrateConfig, IntegrateReport, IntegratorError, integrate_once};
pub use planner::{PlanReport, PlannerError, plan};
pub use ports::{
    BeadStore, Clock, EventSink, Harness, HarnessError, HarnessOutcome, HarnessRequest, Repo,
    RepoError, RunError, RunOutput, Runner, StoreError, ToolPolicy, Worktree,
};
pub use steward::{StewardError, SweepReport, sweep};
pub use transition::{TransitionError, apply_event, load_task};
pub use verifier::{VerifierError, VerifyReport, verify_once};
pub use worker::{WorkReport, WorkerConfig, WorkerError, work_once};
