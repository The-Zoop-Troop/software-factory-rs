//! Blocked-release loop tests for the task state machine (sibling file for the size cap).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use super::tests::{agent, claim, fresh, submit, t};
use super::*;
use crate::counts::Attempts;

#[test]
fn blocked_releases_escalate_as_a_release_loop() {
    let rel = |now: i64, blocked: bool| Event::Release {
        holder: agent("w1"),
        now: t(now),
        note: "released: blocked: the verify command cannot pass on this base".into(),
        blocked,
    };
    // First blocked release: back to open, streak at 1.
    let tr = fresh().apply(claim(0)).unwrap().task.apply(rel(5, true)).unwrap();
    assert_eq!(tr.task.state, TaskState::Open);
    assert_eq!(tr.task.blocked_releases, Attempts::new(1));
    // Second consecutive blocked release: a release-loop incident (before the budget fires,
    // so the operator sees the contract problem, not a generic budget message).
    let tr2 = tr
        .task
        .apply(claim(10))
        .unwrap()
        .task
        .apply(rel(15, true))
        .unwrap();
    assert_eq!(
        tr2.task.state,
        TaskState::Incident {
            reason: IncidentReason::ReleaseLoop {
                releases: Attempts::new(2),
                detail: "released: blocked: the verify command cannot pass on this base".into(),
            }
        }
    );
}

#[test]
fn a_plain_release_or_submission_resets_the_blocked_streak() {
    let rel = |now: i64, blocked: bool| Event::Release {
        holder: agent("w1"),
        now: t(now),
        note: "note".into(),
        blocked,
    };
    // blocked then non-blocked: the streak resets (whatever else the budget decides).
    let tr = fresh().apply(claim(0)).unwrap().task.apply(rel(5, true)).unwrap();
    let tr2 = tr
        .task
        .apply(claim(10))
        .unwrap()
        .task
        .apply(rel(15, false))
        .unwrap();
    assert_eq!(tr2.task.blocked_releases, Attempts::new(0));
    // blocked then a submission: streak gone before verification begins.
    let tr3 = fresh().apply(claim(0)).unwrap().task.apply(rel(5, true)).unwrap();
    let submitted = tr3
        .task
        .apply(claim(10))
        .unwrap()
        .task
        .apply(submit(15))
        .unwrap();
    assert_eq!(submitted.task.blocked_releases, Attempts::new(0));
}
