//! What applying an event yields: the next task and the effects the shell runs, or why the event
//! was refused.

use super::{IncidentReason, Task};
use crate::ids::{AgentId, BeadId, BranchName, Sha};
use crate::time::Timestamp;

/// Result of a successful transition: the new task plus what the shell should do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub task: Task,
    pub effects: Vec<Effect>,
}

/// Side effects the imperative shell must perform after a transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Create a `merge` bead for this branch.
    OpenMergeBead {
        task: BeadId,
        branch: BranchName,
        head: Sha,
    },
    /// Create an `incident` bead.
    OpenIncidentBead {
        task: BeadId,
        reason: IncidentReason,
    },
    /// Append a note to the task bead.
    AppendNote { task: BeadId, note: String },
    /// Close the task's own ledger issue (its metadata says `closed`; the issue must agree so
    /// dependents unblock). Must run before `CloseVerifyBead`, which is blocked by the task.
    CloseTaskBead { task: BeadId },
    /// Close the paired verify bead alongside the task.
    CloseVerifyBead { verify: BeadId },
    /// Close every open merge bead for the task, so the Integrator cannot land it.
    CloseMergeBeads { task: BeadId },
}

/// The event is not permitted in the current state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IllegalTransition {
    #[error("task {id}: event {event} not allowed in state {state}")]
    NotAllowed {
        id: BeadId,
        state: &'static str,
        event: &'static str,
    },
    #[error("task {id}: {actor} is not the lease holder ({holder})")]
    NotHolder {
        id: BeadId,
        actor: AgentId,
        holder: AgentId,
    },
    #[error("task {id}: lease already expired at {expires}")]
    LeaseExpired { id: BeadId, expires: Timestamp },
}
