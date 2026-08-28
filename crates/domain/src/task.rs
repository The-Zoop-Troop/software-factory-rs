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
use crate::ids::{AgentId, BeadId, BranchName, Sha};
use crate::lease::Lease;
use crate::time::{Duration, Timestamp};

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
        expiries: u32,
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
        tokens: u64,
    },
    LeaseExpired {
        now: Timestamp,
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
        detail: String,
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
    pub lease_expiries: u32,
    pub state: TaskState,
}

/// Maximum times a lease may expire before the task is treated as a lease storm.
pub const MAX_LEASE_EXPIRIES: u32 = 3;

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
            lease_expiries: 0,
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
                let expiries = self.lease_expiries.saturating_add(1);
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
                    effects: vec![Effect::CloseVerifyBead { verify }],
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

            // ---- escalate is allowed from every active state ---------------------------
            (
                TaskState::Open
                | TaskState::Leased { .. }
                | TaskState::InVerify { .. }
                | TaskState::Mergeable { .. },
                Event::Escalate { detail },
            ) => Ok(self.escalate(IncidentReason::Manual { detail })),

            // ---- everything else is illegal; listed exhaustively so new variants break the build
            (
                state @ TaskState::Open,
                event @ (Event::Heartbeat { .. }
                | Event::Submit { .. }
                | Event::LeaseExpired { .. }
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
                | Event::Merged { .. }
                | Event::MergeFailed { .. }),
            )
            | (
                state @ TaskState::Mergeable { .. },
                event @ (Event::Claim { .. }
                | Event::Heartbeat { .. }
                | Event::Submit { .. }
                | Event::LeaseExpired { .. }
                | Event::VerifyPassed
                | Event::VerifyFailed { .. }),
            )
            | (
                state @ (TaskState::Closed { .. } | TaskState::Incident { .. }),
                event @ (Event::Claim { .. }
                | Event::Heartbeat { .. }
                | Event::Submit { .. }
                | Event::LeaseExpired { .. }
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
mod tests {
    use super::*;

    fn id(s: &str) -> BeadId {
        BeadId::try_new(s).unwrap()
    }
    fn agent(s: &str) -> AgentId {
        AgentId::try_new(s).unwrap()
    }
    fn sha(c: char) -> Sha {
        Sha::try_new(core::iter::repeat_n(c, 40).collect::<String>()).unwrap()
    }
    fn t(secs: i64) -> Timestamp {
        Timestamp::from_unix_seconds(secs)
    }
    fn fresh() -> Task {
        Task::new(
            id("fac-1"),
            id("fac-2"),
            sha('a'),
            Budget {
                attempts: 2,
                ..Budget::default()
            },
        )
    }
    fn claim(now: i64) -> Event {
        Event::Claim {
            holder: agent("w1"),
            now: t(now),
            ttl: Duration::from_seconds(60),
        }
    }
    fn submit(now: i64) -> Event {
        Event::Submit {
            holder: agent("w1"),
            branch: BranchName::try_new("task/fac-1").unwrap(),
            head: sha('b'),
            now: t(now),
            tokens: 1000,
        }
    }

    #[test]
    fn happy_path() {
        let tr = fresh().apply(claim(0)).unwrap();
        assert_eq!(tr.task.state.name(), "leased");
        let tr = tr
            .task
            .apply(Event::Heartbeat {
                holder: agent("w1"),
                now: t(30),
            })
            .unwrap();
        let tr = tr.task.apply(submit(50)).unwrap();
        assert_eq!(tr.task.state.name(), "in_verify");
        assert_eq!(tr.task.usage.tokens, 1000);
        assert_eq!(tr.task.usage.wall_clock, Duration::from_seconds(50));
        let tr = tr.task.apply(Event::VerifyPassed).unwrap();
        assert_eq!(tr.task.state.name(), "mergeable");
        assert!(matches!(
            tr.effects.as_slice(),
            [Effect::OpenMergeBead { .. }]
        ));
        let tr = tr.task.apply(Event::Merged { merged: sha('c') }).unwrap();
        assert_eq!(tr.task.state.name(), "closed");
        assert!(matches!(
            tr.effects.as_slice(),
            [Effect::CloseVerifyBead { .. }]
        ));
        assert!(tr.task.state.is_terminal());
    }

    #[test]
    fn verify_failure_reopens_then_escalates() {
        let tr = fresh()
            .apply(claim(0))
            .unwrap()
            .task
            .apply(submit(1))
            .unwrap();
        let tr = tr
            .task
            .apply(Event::VerifyFailed {
                note: "boom".into(),
            })
            .unwrap();
        assert_eq!(tr.task.state, TaskState::Open);
        assert_eq!(tr.task.usage.attempts, 1);
        assert!(matches!(tr.effects.as_slice(), [Effect::AppendNote { .. }]));

        let tr = tr
            .task
            .apply(claim(2))
            .unwrap()
            .task
            .apply(submit(3))
            .unwrap();
        let tr = tr
            .task
            .apply(Event::VerifyFailed {
                note: "boom again".into(),
            })
            .unwrap();
        assert!(matches!(
            tr.task.state,
            TaskState::Incident {
                reason: IncidentReason::Budget {
                    exceeded: BudgetExceeded::Attempts { used: 2, limit: 2 }
                }
            }
        ));
        assert!(matches!(
            tr.effects.as_slice(),
            [Effect::AppendNote { .. }, Effect::OpenIncidentBead { .. }]
        ));
    }

    #[test]
    fn lease_expiry_reopens_and_storms() {
        let mut task = fresh();
        for i in 0..MAX_LEASE_EXPIRIES - 1 {
            let base = i64::from(i) * 1000;
            task = task.apply(claim(base)).unwrap().task;
            let tr = task
                .apply(Event::LeaseExpired { now: t(base + 60) })
                .unwrap();
            assert_eq!(tr.task.state, TaskState::Open);
            task = tr.task;
        }
        task = task.apply(claim(9000)).unwrap().task;
        let tr = task.apply(Event::LeaseExpired { now: t(9060) }).unwrap();
        assert!(matches!(
            tr.task.state,
            TaskState::Incident {
                reason: IncidentReason::LeaseStorm {
                    expiries: MAX_LEASE_EXPIRIES
                }
            }
        ));
    }

    #[test]
    fn lease_expiry_before_expiry_is_illegal() {
        let task = fresh().apply(claim(0)).unwrap().task;
        let err = task.apply(Event::LeaseExpired { now: t(10) }).unwrap_err();
        assert!(matches!(
            err,
            IllegalTransition::NotAllowed {
                state: "leased",
                event: "lease_expired",
                ..
            }
        ));
    }

    #[test]
    fn wrong_holder_cannot_submit_or_heartbeat() {
        let task = fresh().apply(claim(0)).unwrap().task;
        let bad = Event::Heartbeat {
            holder: agent("w2"),
            now: t(1),
        };
        assert!(matches!(
            task.clone().apply(bad),
            Err(IllegalTransition::NotHolder { .. })
        ));
        let bad = Event::Submit {
            holder: agent("w2"),
            branch: BranchName::try_new("task/fac-1").unwrap(),
            head: sha('b'),
            now: t(1),
            tokens: 0,
        };
        assert!(matches!(
            task.apply(bad),
            Err(IllegalTransition::NotHolder { .. })
        ));
    }

    #[test]
    fn heartbeat_after_expiry_is_rejected() {
        let task = fresh().apply(claim(0)).unwrap().task;
        let err = task
            .apply(Event::Heartbeat {
                holder: agent("w1"),
                now: t(60),
            })
            .unwrap_err();
        assert!(matches!(err, IllegalTransition::LeaseExpired { .. }));
    }

    #[test]
    fn merge_failure_reopens_with_note() {
        let tr = fresh()
            .apply(claim(0))
            .unwrap()
            .task
            .apply(submit(1))
            .unwrap()
            .task
            .apply(Event::VerifyPassed)
            .unwrap()
            .task
            .apply(Event::MergeFailed {
                detail: "conflict in lib.rs".into(),
            })
            .unwrap();
        assert_eq!(tr.task.state, TaskState::Open);
        assert!(
            matches!(tr.effects.as_slice(), [Effect::AppendNote { note, .. }] if note.contains("conflict"))
        );
    }

    #[test]
    fn terminal_states_reject_everything() {
        let closed = Task {
            state: TaskState::Closed { merged: sha('c') },
            ..fresh()
        };
        let incident = Task {
            state: TaskState::Incident {
                reason: IncidentReason::Manual { detail: "x".into() },
            },
            ..fresh()
        };
        let events = [
            claim(0),
            Event::Heartbeat {
                holder: agent("w1"),
                now: t(0),
            },
            submit(0),
            Event::LeaseExpired { now: t(0) },
            Event::VerifyPassed,
            Event::VerifyFailed {
                note: String::new(),
            },
            Event::Merged { merged: sha('d') },
            Event::MergeFailed {
                detail: String::new(),
            },
            Event::Escalate {
                detail: String::new(),
            },
        ];
        for e in events {
            assert!(
                closed.clone().apply(e.clone()).is_err(),
                "closed accepted {}",
                e.name()
            );
            assert!(
                incident.clone().apply(e.clone()).is_err(),
                "incident accepted {}",
                e.name()
            );
        }
    }

    #[test]
    fn illegal_pairs_in_active_states() {
        let open = fresh();
        assert!(open.clone().apply(Event::VerifyPassed).is_err());
        assert!(open.clone().apply(submit(0)).is_err());
        let leased = open.apply(claim(0)).unwrap().task;
        assert!(leased.clone().apply(claim(1)).is_err());
        assert!(leased.clone().apply(Event::VerifyPassed).is_err());
        let in_verify = leased.apply(submit(1)).unwrap().task;
        assert!(in_verify.clone().apply(claim(2)).is_err());
        assert!(
            in_verify
                .clone()
                .apply(Event::Merged { merged: sha('c') })
                .is_err()
        );
        let mergeable = in_verify.apply(Event::VerifyPassed).unwrap().task;
        assert!(
            mergeable
                .clone()
                .apply(Event::VerifyFailed {
                    note: String::new()
                })
                .is_err()
        );
        assert!(mergeable.apply(claim(3)).is_err());
    }

    #[test]
    fn escalate_from_any_active_state() {
        let e = || Event::Escalate {
            detail: "stop".into(),
        };
        let open = fresh();
        assert!(matches!(
            open.clone().apply(e()).unwrap().task.state,
            TaskState::Incident { .. }
        ));
        let leased = open.apply(claim(0)).unwrap().task;
        assert!(matches!(
            leased.clone().apply(e()).unwrap().task.state,
            TaskState::Incident { .. }
        ));
        let in_verify = leased.apply(submit(1)).unwrap().task;
        assert!(matches!(
            in_verify.clone().apply(e()).unwrap().task.state,
            TaskState::Incident { .. }
        ));
        let mergeable = in_verify.apply(Event::VerifyPassed).unwrap().task;
        assert!(matches!(
            mergeable.apply(e()).unwrap().task.state,
            TaskState::Incident { .. }
        ));
    }
}
