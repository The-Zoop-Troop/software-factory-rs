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
    /// Verifier ran a verify bead against a task.
    Verified { passed: bool, verify_bead: BeadId },
    /// Integrator landed (or failed to land) a branch on main.
    Integrated {
        merge_bead: BeadId,
        landed: Option<domain::Sha>,
        detail: Option<String>,
    },
    /// Steward reopened a task whose lease expired.
    LeaseReaped,
    /// Steward escalated a task to an incident.
    Escalated { detail: String },
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
