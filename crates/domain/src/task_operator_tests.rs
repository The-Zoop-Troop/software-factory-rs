//! Operator events: cancel and retry go through the state machine like every other change.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use super::tests::{claim, fresh, submit};
use super::*;
use crate::counts::Attempts;

fn cancel() -> Event {
    Event::Cancel {
        by: "remote:ops".into(),
    }
}

fn incident() -> Task {
    fresh()
        .apply(Event::Escalate {
            reason: IncidentReason::Manual {
                detail: "stuck".into(),
            },
        })
        .unwrap()
        .task
}

#[test]
fn cancel_ends_every_unfinished_state_and_closes_its_merge_beads() {
    let leased = fresh().apply(claim(0)).unwrap().task;
    let in_verify = leased.clone().apply(submit(5)).unwrap().task;
    let mergeable = in_verify.clone().apply(Event::VerifyPassed).unwrap().task;
    for task in [fresh(), leased, in_verify, mergeable, incident()] {
        let from = task.state.name();
        let tr = task.apply(cancel()).unwrap();
        assert_eq!(
            tr.task.state,
            TaskState::Canceled {
                by: "remote:ops".into()
            },
            "from {from}"
        );
        assert!(tr.task.state.is_terminal());
        assert!(
            matches!(&tr.effects[0], Effect::CloseMergeBeads { task } if task == &tr.task.id),
            "from {from}: {:?}",
            tr.effects
        );
    }
}

#[test]
fn nothing_moves_a_canceled_task_and_a_landed_one_cannot_be_canceled() {
    let canceled = fresh().apply(cancel()).unwrap().task;
    for event in [
        claim(1),
        Event::VerifyPassed,
        cancel(),
        Event::OperatorRetry { note: "x".into() },
    ] {
        assert!(canceled.clone().apply(event).is_err());
    }
    let landed = fresh()
        .apply(claim(0))
        .unwrap()
        .task
        .apply(submit(5))
        .unwrap()
        .task
        .apply(Event::VerifyPassed)
        .unwrap()
        .task
        .apply(Event::Merged {
            merged: crate::ids::Sha::try_new("c".repeat(40)).unwrap(),
        })
        .unwrap()
        .task;
    assert!(landed.apply(cancel()).is_err());
}

#[test]
fn an_operator_retry_reopens_an_incident_with_a_fresh_budget() {
    let mut worn = incident();
    worn.usage = worn.usage.add_attempt().add_attempt();
    worn.lease_expiries = Attempts::new(2);
    worn.blocked_releases = Attempts::new(1);
    let tr = worn
        .apply(Event::OperatorRetry {
            note: "fixed the verify command".into(),
        })
        .unwrap();
    assert_eq!(tr.task.state, TaskState::Open);
    assert_eq!(tr.task.usage, crate::budget::Usage::default());
    assert_eq!(tr.task.lease_expiries, Attempts::new(0));
    assert_eq!(tr.task.blocked_releases, Attempts::new(0));
    assert!(matches!(
        &tr.effects[..],
        [Effect::AppendNote { note, .. }] if note == "incident resolved by operator: fixed the verify command"
    ));
}

#[test]
fn a_retry_is_refused_outside_an_incident() {
    let retry = || Event::OperatorRetry { note: "x".into() };
    assert!(fresh().apply(retry()).is_err());
    assert!(
        fresh()
            .apply(claim(0))
            .unwrap()
            .task
            .apply(retry())
            .is_err()
    );
}
