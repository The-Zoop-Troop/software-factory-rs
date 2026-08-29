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
pub mod console;
pub mod errors;
pub mod events;
pub mod integrator;
pub mod mcp;
pub mod plan_queue;
pub mod planner;
pub mod ports;
pub mod remote;
pub mod rigs;
pub mod steward;
pub mod transition;
pub mod verifier;
pub mod worker;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use bead::{Bead, BeadStatus, NewBead};
pub use console::{EpicSummary, LedgerSummary, inbox, ledger_summary, resolve};
pub use domain;
pub use errors::{
    GitOp, HarnessError, HarnessStage, RepoError, RunError, StoreError, StoreOp, Unavailable,
};
pub use events::{EventKind, FactoryEvent};
pub use integrator::{IntegrateConfig, IntegrateReport, IntegratorError, integrate_once};
pub use mcp::{McpConfig, McpConfigError, McpServer};
pub use plan_queue::{QueuedOutcome, plan_outcome, plan_queued_once, plan_request};
pub use planner::{PlanReport, PlannerError, plan};
pub use ports::{
    BeadStore, Clock, EventSink, Harness, HarnessOutcome, HarnessRequest, Repo, RunOutput, Runner,
    ToolPolicy, Worktree,
};
pub use remote::chat::{
    A2aApi, ChatCommand, ChatParseError, ClientError, HELP, Seen, handle, notifications,
    parse_command, render_inbox, render_tasks, with_vanished,
};
pub use remote::service::{
    RemoteError, Sent, cancel_task, events_after, get_task, list_tasks, list_tasks_with_vanished,
    send_message, spend,
};
pub use remote::{
    Authenticator, EventRecord, EventTail, PlanSubmitter, Rig, RigRegistry, SubmitError, TailError,
};
pub use rigs::{
    HostDocker, HostError, HostRegistry, HostRig, RegistryError, RigHealth, backup, backup_path,
    doctor as rig_doctor,
};
pub use steward::{StewardError, SweepReport, sweep};
pub use transition::{TransitionError, apply_event, load_task};
pub use verifier::{VerifierError, VerifyReport, verify_once};
pub use worker::{WorkReport, WorkerConfig, WorkerError, work_once};
