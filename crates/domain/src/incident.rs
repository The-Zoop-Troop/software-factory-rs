//! Why a task needs a human: incident reasons and their operator-facing messages.
//! Split from `task.rs` for the file-size taste cap; re-exported there, so
//! `domain::task::IncidentReason` keeps working.

use core::fmt;

use crate::budget::BudgetExceeded;
use crate::counts::Attempts;

/// Why a task became an incident.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum IncidentReason {
    Budget {
        exceeded: BudgetExceeded,
    },
    /// Reopened by lease expiry too many times without ever reaching verification.
    LeaseStorm {
        expiries: Attempts,
    },
    /// The Integrator could not land it.
    MergeConflict {
        detail: String,
    },
    /// A human or agent escalated explicitly.
    Manual {
        detail: String,
    },
    /// The rig could not run verification (no space, permission denied, missing tool,
    /// network). Not the task's fault: attempts are not charged and the branch is kept.
    Environment {
        detail: String,
    },
    /// Sessions keep declaring the task blocked and releasing without submitting: the
    /// contract likely cannot be satisfied as written (an impossible verify command, a
    /// scope rule that contradicts the goal).
    ReleaseLoop {
        releases: Attempts,
        detail: String,
    },
}

impl fmt::Display for IncidentReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Budget { exceeded } => write!(
                f,
                "budget exhausted: {exceeded}. The worker kept failing verification or ran out of \
                 time/tokens. Resolving reopens the task with fresh attempts; if the task itself \
                 is wrong, stop the epic and re-plan."
            ),
            Self::LeaseStorm { expiries } => write!(
                f,
                "lease expired {expiries} times without a submission: workers keep dying or \
                 stalling on this task. Check worker logs; resolving reopens it."
            ),
            Self::MergeConflict { detail } => write!(
                f,
                "the Integrator could not land this task's branch on main ({detail}). Another \
                 task changed the same files first. Resolving reopens the task so a worker redoes \
                 it on top of the current main; if the work is now redundant, stop the epic instead."
            ),
            Self::Manual { detail } => write!(f, "escalated by hand: {detail}"),
            Self::Environment { detail } => write!(
                f,
                "the rig could not run the verification ({detail}). This is an environment \
                 problem, not the task's: fix the rig (image, volume, network), then resume from \
                 the task's branch or retry."
            ),
            Self::ReleaseLoop { releases, detail } => write!(
                f,
                "released without submitting {releases} times, each session declaring itself \
                 blocked ({detail}). The task's contract likely cannot be satisfied as written: \
                 retry with guidance that unblocks it, or re-plan the epic."
            ),
        }
    }
}
