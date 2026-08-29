//! Exhaustive tests for the task state machine (a sibling file to respect the size cap).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use super::*;
use crate::counts::{Attempts, Tokens};

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
            attempts: Attempts::new(2),
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
        tokens: Tokens::new(1000),
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
    assert_eq!(tr.task.usage.tokens, Tokens::new(1000));
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
        [Effect::CloseTaskBead { .. }, Effect::CloseVerifyBead { .. }]
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
    assert_eq!(tr.task.usage.attempts, Attempts::new(1));
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
                exceeded: BudgetExceeded::Attempts { used, limit }
            }
        } if used.get() == 2 && limit.get() == 2
    ));
    assert!(matches!(
        tr.effects.as_slice(),
        [Effect::AppendNote { .. }, Effect::OpenIncidentBead { .. }]
    ));
}

#[test]
fn lease_expiry_reopens_and_storms() {
    let mut task = fresh();
    for i in 0..MAX_LEASE_EXPIRIES.get() - 1 {
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
fn terminal_flags_and_state_names() {
    assert!(!TaskState::Open.is_terminal());
    let leased = fresh().apply(claim(0)).unwrap().task;
    assert!(!leased.state.is_terminal());
    assert_eq!(leased.state.name(), "leased");
    assert!(TaskState::Closed { merged: sha('c') }.is_terminal());
    assert!(
        TaskState::Incident {
            reason: IncidentReason::Manual {
                detail: String::new()
            }
        }
        .is_terminal()
    );
}

#[test]
fn lease_expiry_updates_usage_and_counter() {
    let task = fresh().apply(claim(0)).unwrap().task;
    let tr = task.apply(Event::LeaseExpired { now: t(90) }).unwrap();
    assert_eq!(tr.task.lease_expiries, Attempts::new(1));
    assert_eq!(tr.task.usage.wall_clock, Duration::from_seconds(90));
    assert_eq!(tr.task.state, TaskState::Open);
    assert!(
        matches!(tr.effects.as_slice(), [Effect::AppendNote { note, .. }] if note.contains("expired"))
    );
}

#[test]
fn verify_failure_and_merge_failure_accumulate_usage() {
    let tr = fresh()
        .apply(claim(0))
        .unwrap()
        .task
        .apply(submit(20))
        .unwrap();
    assert_eq!(tr.task.usage.wall_clock, Duration::from_seconds(20));
    let tr = tr
        .task
        .apply(Event::VerifyFailed { note: "n".into() })
        .unwrap();
    assert_eq!(
        tr.task.usage,
        Usage {
            tokens: Tokens::new(1000),
            wall_clock: Duration::from_seconds(20),
            attempts: Attempts::new(1)
        }
    );
    let task = Task {
        budget: Budget {
            attempts: Attempts::new(5),
            ..Budget::default()
        },
        ..fresh()
    };
    let tr = task
        .apply(claim(0))
        .unwrap()
        .task
        .apply(submit(5))
        .unwrap()
        .task
        .apply(Event::VerifyPassed)
        .unwrap()
        .task
        .apply(Event::MergeFailed { detail: "c".into() })
        .unwrap();
    assert_eq!(tr.task.usage.attempts, Attempts::new(1));
    assert_eq!(tr.task.usage.tokens, Tokens::new(1000));
}

#[test]
fn heartbeat_renews_from_now() {
    let task = fresh().apply(claim(0)).unwrap().task;
    let tr = task
        .apply(Event::Heartbeat {
            holder: agent("w1"),
            now: t(40),
        })
        .unwrap();
    assert!(matches!(
        &tr.task.state,
        TaskState::Leased { lease } if lease.expires == t(100) && lease.claimed_at == t(0)
    ));
}

#[test]
fn lease_storm_incident_keeps_usage_and_counter() {
    let mut task = fresh();
    for i in 0..MAX_LEASE_EXPIRIES.get() {
        let base = i64::from(i) * 100;
        task = task.apply(claim(base)).unwrap().task;
        task = task
            .apply(Event::LeaseExpired { now: t(base + 60) })
            .unwrap()
            .task;
    }
    assert_eq!(task.lease_expiries, MAX_LEASE_EXPIRIES);
    assert_eq!(
        task.usage.wall_clock,
        Duration::from_seconds(60 * u64::from(MAX_LEASE_EXPIRIES.get()))
    );
    assert!(matches!(
        task.state,
        TaskState::Incident {
            reason: IncidentReason::LeaseStorm { .. }
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
fn release_reopens_and_counts_attempt_then_escalates() {
    let task = fresh().apply(claim(0)).unwrap().task;
    let rel = |now: i64| Event::Release {
        holder: agent("w1"),
        now: t(now),
        note: "harness error".into(),
    };
    let tr = task.apply(rel(5)).unwrap();
    assert_eq!(tr.task.state, TaskState::Open);
    assert_eq!(tr.task.usage.attempts, Attempts::new(1));
    assert!(matches!(tr.effects.as_slice(), [Effect::AppendNote { .. }]));
    let tr = tr
        .task
        .apply(claim(10))
        .unwrap()
        .task
        .apply(rel(12))
        .unwrap();
    assert!(
        matches!(tr.task.state, TaskState::Incident { .. }),
        "attempts budget was 2"
    );
    assert!(fresh().apply(rel(0)).is_err(), "release needs a lease");
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
        tokens: Tokens::new(0),
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
            reason: IncidentReason::Manual {
                detail: String::new(),
            },
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
        reason: IncidentReason::Manual {
            detail: "stop".into(),
        },
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

#[test]
fn name_tables_match_the_types() {
    let a = sha('a');
    let states = [
        TaskState::Open,
        TaskState::Leased {
            lease: Lease::grant(agent("w"), t(0), Duration::from_seconds(1)),
        },
        TaskState::InVerify {
            branch: BranchName::try_new("b").unwrap(),
            head: a.clone(),
        },
        TaskState::Mergeable {
            branch: BranchName::try_new("b").unwrap(),
            head: a.clone(),
        },
        TaskState::Closed { merged: a },
        TaskState::Incident {
            reason: IncidentReason::Manual {
                detail: String::new(),
            },
        },
    ];
    assert_eq!(states.map(|s| s.name()), STATE_NAMES);
    let events = [
        claim(0),
        Event::Heartbeat {
            holder: agent("w"),
            now: t(0),
        },
        submit(0),
        Event::LeaseExpired { now: t(0) },
        Event::Release {
            holder: agent("w"),
            now: t(0),
            note: String::new(),
        },
        Event::VerifyPassed,
        Event::VerifyFailed {
            note: String::new(),
        },
        Event::Merged { merged: sha('c') },
        Event::MergeFailed {
            detail: String::new(),
        },
        Event::Escalate {
            reason: IncidentReason::Manual {
                detail: String::new(),
            },
        },
    ];
    assert_eq!(events.map(|e| e.name()), EVENT_NAMES);
}

#[test]
fn incident_reasons_explain_themselves() {
    let merge = IncidentReason::MergeConflict {
        detail: "conflicts in lib.sh".into(),
    };
    let text = merge.to_string();
    assert!(text.contains("could not land") && text.contains("lib.sh") && text.contains("reopens"));
    assert!(
        IncidentReason::LeaseStorm {
            expiries: Attempts::new(3)
        }
        .to_string()
        .contains("3 times")
    );
    assert!(
        IncidentReason::Manual { detail: "x".into() }
            .to_string()
            .contains("escalated by hand")
    );
    let budget = IncidentReason::Budget {
        exceeded: BudgetExceeded::Attempts {
            used: Attempts::new(3),
            limit: Attempts::new(3),
        },
    };
    assert!(budget.to_string().contains("budget exhausted"));
}
