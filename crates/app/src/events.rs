//! The factory's structured event log (ARCHITECTURE.md §4.5). One record per state
//! transition or steward action; the observability pipeline for Phase 0.

use domain::{BeadId, Timestamp};

/// What happened.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum EventKind {
    /// A task changed state via `apply_event`.
    Transition {
        from: &'static str,
        event: &'static str,
        to: &'static str,
    },
    /// A worker claimed a task.
    Claimed { holder: String },
    /// A worker finished a session and submitted a branch.
    Submitted {
        holder: String,
        tokens: domain::Tokens,
        turns: domain::Turns,
        head: domain::Sha,
    },
    /// A worker gave a task back without submitting (errored session, no changes).
    Released { holder: String, detail: String },
    /// Verifier picked up an awaiting task and is running its checks (the other edge of `Verified`).
    VerifyStarted { verify_bead: BeadId },
    /// Verifier ran a verify bead against a task.
    Verified { passed: bool, verify_bead: BeadId },
    /// Verifier could not run the checks (environment); the task is an incident, no attempt charged.
    VerifyBlocked { verify_bead: BeadId, detail: String },
    /// A running session's worktree drift, sampled on each lease heartbeat.
    Progress {
        files: u32,
        insertions: u32,
        deletions: u32,
    },
    /// Integrator picked up a mergeable task and is rebasing it (the other edge of `Integrated`).
    IntegrateStarted { merge_bead: BeadId },
    /// Planner wrote a task; `needs` are the sibling tasks it waits for. With `Integrated` of each
    /// need this gives the moment a task became ready, with no Steward bookkeeping.
    TaskPlanned { epic: BeadId, needs: Vec<BeadId> },
    /// Integrator landed (or failed to land) a branch on main.
    Integrated {
        merge_bead: BeadId,
        landed: Option<domain::Sha>,
        rejection: Option<crate::integrator::LandRejection>,
    },
    /// Steward reopened a task whose lease expired.
    LeaseReaped,
    /// Steward escalated a task to an incident.
    Escalated { exceeded: domain::BudgetExceeded },
    /// Steward re-created a merge bead for a task left `mergeable` without one.
    MergeBeadRepaired,
    /// Steward closed an epic whose children are all closed.
    EpicClosed { children: usize },
    /// A sweep finished.
    SweepDone {
        reaped: usize,
        escalated: usize,
        epics_closed: usize,
    },
    /// A sweep step failed; recorded, not fatal.
    Error { detail: String },
    /// A remote client acted (or was refused) through the console. `actor` is `remote:<client>`.
    Remote { action: String, detail: String },
}

/// One line of the event log.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FactoryEvent {
    pub at: Timestamp,
    pub actor: String,
    pub bead: Option<BeadId>,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub kind: EventKind,
}
