//! The task state machine (ARCHITECTURE.md §3.3).
//!
//! ```text
//! open ──claim──▶ leased ──submit──▶ in_verify ──pass──▶ mergeable ──merged──▶ closed
//!                  │                    │
//!                  │ lease expired      │ fail (attempts < budget)
//!                  ▼                    ▼
//!                open  ◀──────────── open (+failure note)
//!                                       │ fail (attempts exhausted)
//!                                       ▼
//!                                   incident
//! ```
//!
//! `transition` is total over `(TaskState, Event)`: every pair yields either a new
//! state or an `IllegalTransition`. Adding a state or event must break the build.

use crate::budget::{Budget, BudgetExceeded, Usage};
use crate::counts::{Attempts, Tokens};
use crate::ids::{AgentId, BeadId, BranchName, Sha};
use crate::lease::Lease;
use crate::time::{Duration, Timestamp};
use core::fmt;

/// Where a task bead is in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "state", rename_all = "snake_case"))]
pub enum TaskState {
    /// Claimable (subject to `bd ready` dependency rules).
    Open,
    /// A worker holds it.
    Leased { lease: Lease },
    /// Worker pushed a branch; awaiting the Verifier.
    InVerify { branch: BranchName, head: Sha },
    /// Verification passed; awaiting the Integrator.
    Mergeable { branch: BranchName, head: Sha },
    /// Landed on main.
    Closed { merged: Sha },
    /// Needs a human or the Steward.
    Incident { reason: IncidentReason },
}

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
        }
    }
}

/// Something that happened to a task. Carries only facts, never decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Claim {
        holder: AgentId,
        now: Timestamp,
        ttl: Duration,
    },
    Heartbeat {
        holder: AgentId,
        now: Timestamp,
    },
    /// Worker committed and pushed; releases the lease.
    Submit {
        holder: AgentId,
        branch: BranchName,
        head: Sha,
        now: Timestamp,
        tokens: Tokens,
    },
    LeaseExpired {
        now: Timestamp,
    },
    /// Holder gives the task back without submitting (session errored, nothing produced).
    /// Counts as an attempt.
    Release {
        holder: AgentId,
        now: Timestamp,
        note: String,
    },
    VerifyPassed,
    VerifyFailed {
        note: String,
    },
    Merged {
        merged: Sha,
    },
    MergeFailed {
        detail: String,
    },
    Escalate {
        reason: IncidentReason,
    },
}

/// A task bead as the factory sees it: state plus the counters the state machine needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: BeadId,
    pub verify_bead: BeadId,
    pub base: Sha,
    pub budget: Budget,
    pub usage: Usage,
    pub lease_expiries: Attempts,
    pub state: TaskState,
}

/// Every state name, in lifecycle order (for generated docs and console rendering).
pub const STATE_NAMES: [&str; 6] = [
    "open",
    "leased",
    "in_verify",
    "mergeable",
    "closed",
    "incident",
];
/// Every event name (for generated docs).
pub const EVENT_NAMES: [&str; 10] = [
    "claim",
    "heartbeat",
    "submit",
    "lease_expired",
    "release",
    "verify_passed",
    "verify_failed",
    "merged",
    "merge_failed",
    "escalate",
];

/// Maximum times a lease may expire before the task is treated as a lease storm.
pub const MAX_LEASE_EXPIRIES: Attempts = Attempts::new(3);

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

impl TaskState {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Leased { .. } => "leased",
            Self::InVerify { .. } => "in_verify",
            Self::Mergeable { .. } => "mergeable",
            Self::Closed { .. } => "closed",
            Self::Incident { .. } => "incident",
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        match self {
            Self::Closed { .. } | Self::Incident { .. } => true,
            Self::Open | Self::Leased { .. } | Self::InVerify { .. } | Self::Mergeable { .. } => {
                false
            }
        }
    }
}

impl Event {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Claim { .. } => "claim",
            Self::Heartbeat { .. } => "heartbeat",
            Self::Submit { .. } => "submit",
            Self::LeaseExpired { .. } => "lease_expired",
            Self::Release { .. } => "release",
            Self::VerifyPassed => "verify_passed",
            Self::VerifyFailed { .. } => "verify_failed",
            Self::Merged { .. } => "merged",
            Self::MergeFailed { .. } => "merge_failed",
            Self::Escalate { .. } => "escalate",
        }
    }
}

impl Task {
    /// A fresh task in `Open` with default budget and no usage.
    #[must_use]
    pub fn new(id: BeadId, verify_bead: BeadId, base: Sha, budget: Budget) -> Self {
        Self {
            id,
            verify_bead,
            base,
            budget,
            usage: Usage::default(),
            lease_expiries: Attempts::new(0),
            state: TaskState::Open,
        }
    }

    /// Apply `event`, producing the next task and the effects the shell must run.
    ///
    /// # Errors
    /// `IllegalTransition` when the event is not valid in the current state.
    #[allow(
        clippy::too_many_lines,
        reason = "the full (state, event) table lives in one match so exhaustiveness is visible"
    )]
    pub fn apply(self, event: Event) -> Result<Transition, IllegalTransition> {
        let id = self.id.clone();
        let illegal = |state: &TaskState, event: &Event| IllegalTransition::NotAllowed {
            id: id.clone(),
            state: state.name(),
            event: event.name(),
        };

        match (self.state.clone(), event) {
            // ---- open -----------------------------------------------------------------
            (TaskState::Open, Event::Claim { holder, now, ttl }) => Ok(Transition {
                task: Task {
                    state: TaskState::Leased {
                        lease: Lease::grant(holder, now, ttl),
                    },
                    ..self
                },
                effects: vec![],
            }),

            // ---- leased ---------------------------------------------------------------
            (TaskState::Leased { lease }, Event::Heartbeat { holder, now }) => {
                Self::require_holder(&self.id, &lease, &holder)?;
                if lease.is_expired(now) {
                    return Err(IllegalTransition::LeaseExpired {
                        id: self.id,
                        expires: lease.expires,
                    });
                }
                let ttl = lease.expires.since(lease.claimed_at);
                Ok(Transition {
                    task: Task {
                        state: TaskState::Leased {
                            lease: lease.renew(now, ttl),
                        },
                        ..self
                    },
                    effects: vec![],
                })
            }
            (
                TaskState::Leased { lease },
                Event::Submit {
                    holder,
                    branch,
                    head,
                    now,
                    tokens,
                },
            ) => {
                Self::require_holder(&self.id, &lease, &holder)?;
                let usage = self
                    .usage
                    .add_tokens(tokens)
                    .add_wall_clock(now.since(lease.claimed_at));
                Ok(Transition {
                    task: Task {
                        usage,
                        state: TaskState::InVerify { branch, head },
                        ..self
                    },
                    effects: vec![],
                })
            }
            (TaskState::Leased { lease }, Event::LeaseExpired { now }) => {
                if !lease.is_expired(now) {
                    return Err(illegal(&self.state, &Event::LeaseExpired { now }));
                }
                let expiries = self.lease_expiries.incr();
                let usage = self.usage.add_wall_clock(now.since(lease.claimed_at));
                let note = format!(
                    "lease held by {} expired at {}",
                    lease.holder,
                    now.unix_seconds()
                );
                if expiries >= MAX_LEASE_EXPIRIES {
                    let t = Task {
                        usage,
                        lease_expiries: expiries,
                        ..self
                    };
                    return Ok(t.escalate(IncidentReason::LeaseStorm { expiries }));
                }
                Ok(Transition {
                    task: Task {
                        usage,
                        lease_expiries: expiries,
                        state: TaskState::Open,
                        ..self
                    },
                    effects: vec![Effect::AppendNote { task: id, note }],
                })
            }

            // ---- in_verify ------------------------------------------------------------
            (TaskState::InVerify { branch, head }, Event::VerifyPassed) => Ok(Transition {
                task: Task {
                    state: TaskState::Mergeable {
                        branch: branch.clone(),
                        head: head.clone(),
                    },
                    ..self
                },
                effects: vec![Effect::OpenMergeBead {
                    task: id,
                    branch,
                    head,
                }],
            }),
            (TaskState::InVerify { .. }, Event::VerifyFailed { note }) => {
                let usage = self.usage.add_attempt();
                let t = Task { usage, ..self };
                match t.budget.check(usage) {
                    Ok(()) => Ok(Transition {
                        task: Task {
                            state: TaskState::Open,
                            ..t
                        },
                        effects: vec![Effect::AppendNote { task: id, note }],
                    }),
                    Err(exceeded) => {
                        let mut tr = t.escalate(IncidentReason::Budget { exceeded });
                        tr.effects.insert(0, Effect::AppendNote { task: id, note });
                        Ok(tr)
                    }
                }
            }

            // ---- mergeable ------------------------------------------------------------
            (TaskState::Mergeable { .. }, Event::Merged { merged }) => {
                let verify = self.verify_bead.clone();
                Ok(Transition {
                    task: Task {
                        state: TaskState::Closed { merged },
                        ..self
                    },
                    effects: vec![
                        Effect::CloseTaskBead { task: id },
                        Effect::CloseVerifyBead { verify },
                    ],
                })
            }
            (TaskState::Mergeable { .. }, Event::MergeFailed { detail }) => {
                let usage = self.usage.add_attempt();
                let t = Task { usage, ..self };
                match t.budget.check(usage) {
                    Ok(()) => Ok(Transition {
                        task: Task {
                            state: TaskState::Open,
                            ..t
                        },
                        effects: vec![Effect::AppendNote {
                            task: id,
                            note: format!("merge failed: {detail}"),
                        }],
                    }),
                    Err(_) => Ok(t.escalate(IncidentReason::MergeConflict { detail })),
                }
            }

            (TaskState::Leased { lease }, Event::Release { holder, now, note }) => {
                Self::require_holder(&self.id, &lease, &holder)?;
                let usage = self
                    .usage
                    .add_wall_clock(now.since(lease.claimed_at))
                    .add_attempt();
                let t = Task { usage, ..self };
                match t.budget.check(usage) {
                    Ok(()) => Ok(Transition {
                        task: Task {
                            state: TaskState::Open,
                            ..t
                        },
                        effects: vec![Effect::AppendNote { task: id, note }],
                    }),
                    Err(exceeded) => {
                        let mut tr = t.escalate(IncidentReason::Budget { exceeded });
                        tr.effects.insert(0, Effect::AppendNote { task: id, note });
                        Ok(tr)
                    }
                }
            }

            // ---- escalate is allowed from every active state ---------------------------
            (
                TaskState::Open
                | TaskState::Leased { .. }
                | TaskState::InVerify { .. }
                | TaskState::Mergeable { .. },
                Event::Escalate { reason },
            ) => Ok(self.escalate(reason)),

            // ---- everything else is illegal; listed exhaustively so new variants break the build
            (
                state @ TaskState::Open,
                event @ (Event::Heartbeat { .. }
                | Event::Submit { .. }
                | Event::LeaseExpired { .. }
                | Event::Release { .. }
                | Event::VerifyPassed
                | Event::VerifyFailed { .. }
                | Event::Merged { .. }
                | Event::MergeFailed { .. }),
            )
            | (
                state @ TaskState::Leased { .. },
                event @ (Event::Claim { .. }
                | Event::VerifyPassed
                | Event::VerifyFailed { .. }
                | Event::Merged { .. }
                | Event::MergeFailed { .. }),
            )
            | (
                state @ TaskState::InVerify { .. },
                event @ (Event::Claim { .. }
                | Event::Heartbeat { .. }
                | Event::Submit { .. }
                | Event::LeaseExpired { .. }
                | Event::Release { .. }
                | Event::Merged { .. }
                | Event::MergeFailed { .. }),
            )
            | (
                state @ TaskState::Mergeable { .. },
                event @ (Event::Claim { .. }
                | Event::Heartbeat { .. }
                | Event::Submit { .. }
                | Event::LeaseExpired { .. }
                | Event::Release { .. }
                | Event::VerifyPassed
                | Event::VerifyFailed { .. }),
            )
            | (
                state @ (TaskState::Closed { .. } | TaskState::Incident { .. }),
                event @ (Event::Claim { .. }
                | Event::Heartbeat { .. }
                | Event::Submit { .. }
                | Event::LeaseExpired { .. }
                | Event::Release { .. }
                | Event::VerifyPassed
                | Event::VerifyFailed { .. }
                | Event::Merged { .. }
                | Event::MergeFailed { .. }
                | Event::Escalate { .. }),
            ) => Err(illegal(&state, &event)),
        }
    }

    fn escalate(self, reason: IncidentReason) -> Transition {
        let id = self.id.clone();
        Transition {
            task: Task {
                state: TaskState::Incident {
                    reason: reason.clone(),
                },
                ..self
            },
            effects: vec![Effect::OpenIncidentBead { task: id, reason }],
        }
    }

    fn require_holder(
        id: &BeadId,
        lease: &Lease,
        actor: &AgentId,
    ) -> Result<(), IllegalTransition> {
        if &lease.holder == actor {
            Ok(())
        } else {
            Err(IllegalTransition::NotHolder {
                id: id.clone(),
                actor: actor.clone(),
                holder: lease.holder.clone(),
            })
        }
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
