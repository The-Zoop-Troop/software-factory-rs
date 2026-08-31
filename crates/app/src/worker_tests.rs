//! Worker tests: the claim → session → commit → submit loop over fakes.
#![allow(clippy::disallowed_types, reason = "test doubles use a leaf std Mutex")]

use crate::transition::load_task;
use domain::{Budget, FactoryMeta, Sha, Timestamp, Usage};

use super::*;
use crate::testing::{FakeHarness, FakeRepo, FakeStore, FixedClock, MemorySink};

fn id(s: &str) -> BeadId {
    BeadId::try_new(s).unwrap()
}
fn sha(c: char) -> Sha {
    Sha::try_new(core::iter::repeat_n(c, 40).collect::<String>()).unwrap()
}
fn cfg() -> WorkerConfig {
    WorkerConfig {
        agent: AgentId::try_new("worker-1").unwrap(),
        main: BranchName::try_new("main").unwrap(),
        lease_ttl: Duration::from_seconds(30),
        max_turns: Turns::new(10),
        effort: None,
    }
}
fn harness_text(text: &str) -> FakeHarness {
    FakeHarness {
        outcome: Some(crate::ports::HarnessOutcome {
            text: text.into(),
            structured: None,
            tokens: Tokens::new(5000),
            cost_micro_usd: domain::MicroUsd::new(10),
            turns: Turns::new(7),
            is_error: false,
        }),
        requests: std::sync::Mutex::default(),
        yields: 0,
        blocked: None,
    }
}

async fn seeded() -> FakeStore {
    let store = FakeStore::default();
    store.seed_epic(id("fac-e"), &[]).await;
    store
        .seed_reference(id("fac-e.0"), id("fac-e"), "Use POSIX sh only.")
        .await;
    store
        .seed_task(
            id("fac-e.1"),
            FactoryMeta {
                verify_bead: id("fac-e.2"),
                base: sha('a'),
                budget: Budget {
                    wall_clock: Duration::from_minutes(10),
                    ..Budget::default()
                },
                usage: Usage::default(),
                lease_expiries: Attempts::new(0),
                state: TaskState::Open,
            },
        )
        .await;
    store.set_parent(&id("fac-e.1"), &id("fac-e")).await;
    store
        .note(&id("fac-e.1"), "verify FAILED: missing farewell()")
        .await
        .unwrap();
    store
        .seed_verify(id("fac-e.2"), id("fac-e.1"), &["sh tests/run.sh"])
        .await;
    store
}

#[tokio::test]
async fn a_long_session_reports_worktree_drift_on_each_heartbeat() {
    let store = seeded().await;
    let repo = FakeRepo {
        commit_head: Some(sha('b')),
        drift: crate::ports::DiffStat {
            files: 3,
            insertions: 40,
            deletions: 2,
        },
        ..FakeRepo::default()
    };
    let harness = FakeHarness {
        yields: 12,
        ..harness_text("done")
    };
    let log = MemorySink::default();
    work_once(
        &store,
        &repo,
        &harness,
        &FixedClock(Timestamp::from_unix_seconds(100)),
        &log,
        &cfg(),
    )
    .await
    .unwrap()
    .unwrap();
    let progress = log
        .events()
        .await
        .into_iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::Progress {
                    files: 3,
                    insertions: 40,
                    deletions: 2
                }
            )
        })
        .count();
    assert!(progress >= 1, "a heartbeat sampled the worktree");
}

#[tokio::test]
async fn a_resumed_branch_with_no_new_commits_is_submitted_not_released() {
    let store = seeded().await;
    store
        .note(&id("fac-e.1"), "resume-from: task/fac-e.1")
        .await
        .unwrap();
    // commit_all reports the same head the session started from: nothing new was committed.
    let repo = FakeRepo {
        commit_head: Some(sha('a')),
        ..FakeRepo::default()
    };
    let log = MemorySink::default();
    let report = work_once(
        &store,
        &repo,
        &harness_text("already done on the branch"),
        &FixedClock(Timestamp::from_unix_seconds(100)),
        &log,
        &cfg(),
    )
    .await
    .unwrap();
    assert!(report.is_some(), "submitted for verification");
    let task = load_task(&store, &id("fac-e.1")).await.unwrap();
    assert!(
        matches!(task.state, TaskState::InVerify { .. }),
        "{:?}",
        task.state
    );
}

#[tokio::test]
async fn two_workers_racing_one_task_claim_it_exactly_once() {
    let store = std::sync::Arc::new(seeded().await);
    let repo = FakeRepo {
        commit_head: Some(sha('b')),
        ..FakeRepo::default()
    };
    let log = MemorySink::default();
    let clock = FixedClock(Timestamp::from_unix_seconds(100));
    let mk = |n: &str| WorkerConfig {
        agent: domain::AgentId::try_new(n).expect("agent"),
        ..cfg()
    };
    let (cfg_a, cfg_b) = (mk("worker-a"), mk("worker-b"));
    let (harness_a, harness_b) = (harness_text("done"), harness_text("done"));
    let (a, b) = tokio::join!(
        work_once(store.as_ref(), &repo, &harness_a, &clock, &log, &cfg_a),
        work_once(store.as_ref(), &repo, &harness_b, &clock, &log, &cfg_b),
    );
    let reports = [a.unwrap(), b.unwrap()];
    assert_eq!(
        reports.iter().filter(|r| r.is_some()).count(),
        1,
        "exactly one wins: {reports:?}"
    );
    let claims = log
        .events()
        .await
        .into_iter()
        .filter(|e| matches!(e.kind, EventKind::Claimed { .. }))
        .count();
    assert_eq!(claims, 1, "one claimed event, not two");
}

#[tokio::test]
async fn a_blocked_session_is_released_with_its_reason_and_the_file_never_lands() {
    let store = seeded().await;
    let root = std::env::temp_dir().join(format!("factory-blocked-{}", std::process::id()));
    let repo = FakeRepo {
        commit_head: Some(sha('b')),
        worktree_root: Some(root.clone()),
        ..FakeRepo::default()
    };
    let harness = FakeHarness {
        blocked: Some("Need the OAuth client id; none in the repo.".into()),
        ..harness_text("stopping")
    };
    let log = MemorySink::default();
    let report = work_once(
        &store,
        &repo,
        &harness,
        &FixedClock(Timestamp::from_unix_seconds(100)),
        &log,
        &cfg(),
    )
    .await
    .unwrap();
    assert!(report.is_none(), "nothing submitted");
    let task = load_task(&store, &id("fac-e.1")).await.unwrap();
    assert!(matches!(task.state, TaskState::Open));
    let notes = store
        .show(&id("fac-e.1"))
        .await
        .unwrap()
        .notes
        .unwrap_or_default();
    assert!(
        notes.contains("released: blocked: Need the OAuth client id"),
        "{notes}"
    );
    assert!(
        !root
            .join("task__fac-e.1")
            .join("FACTORY_BLOCKED.md")
            .exists(),
        "file consumed"
    );
    let kinds: Vec<_> = log.events().await.into_iter().map(|e| e.kind).collect();
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, EventKind::Released { detail, .. } if detail.contains("OAuth")))
    );
}

#[tokio::test]
async fn claims_runs_commits_and_submits() {
    let store = seeded().await;
    let repo = FakeRepo {
        commit_head: Some(sha('b')),
        ..FakeRepo::default()
    };
    let harness = harness_text("done");
    let log = MemorySink::default();
    let report = work_once(
        &store,
        &repo,
        &harness,
        &FixedClock(Timestamp::from_unix_seconds(100)),
        &log,
        &cfg(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(report.task, id("fac-e.1"));
    assert_eq!(report.branch.as_ref(), "task/fac-e.1");
    assert_eq!(report.head, sha('b'));
    let task = load_task(&store, &id("fac-e.1")).await.unwrap();
    assert!(matches!(task.state, TaskState::InVerify { ref head, .. } if *head == sha('b')));
    assert_eq!(task.usage.tokens, Tokens::new(5000));
    assert_eq!(repo.commits.lock().unwrap().len(), 1);
    assert_eq!(repo.removed.lock().unwrap().len(), 1);

    let req = harness.requests.lock().unwrap()[0].clone();
    assert_eq!(req.tools, ToolPolicy::Full);
    assert!(
        req.prompt.contains("sh tests/run.sh"),
        "verify commands in packet"
    );
    assert!(
        req.prompt.contains("Use POSIX sh only."),
        "reference in packet"
    );
    assert!(
        req.prompt.contains("missing farewell()"),
        "prior notes in packet"
    );
    assert!(req.prompt.contains("attempt 1 of 3"));
    assert!(req.timeout <= Duration::from_minutes(10));

    let kinds: Vec<_> = log.events().await.into_iter().map(|e| e.kind).collect();
    assert!(matches!(kinds[0], EventKind::Claimed { .. }));
    assert!(matches!(
        kinds.last().unwrap(),
        EventKind::Submitted { tokens, turns, .. } if tokens.get() == 5000 && turns.get() == 7
    ));
}

#[tokio::test]
async fn nothing_ready_is_none() {
    let store = FakeStore::default();
    let out = work_once(
        &store,
        &FakeRepo::default(),
        &harness_text(""),
        &FixedClock(Timestamp::from_unix_seconds(0)),
        &MemorySink::default(),
        &cfg(),
    )
    .await
    .unwrap();
    assert_eq!(out, None);
}

#[tokio::test]
async fn skips_tasks_not_open() {
    let store = seeded().await;
    // Move it to leased by someone else; ready() in the fake still lists it.
    apply_event(
        &store,
        &id("fac-e.1"),
        Event::Claim {
            holder: AgentId::try_new("other").unwrap(),
            now: Timestamp::from_unix_seconds(0),
            ttl: Duration::from_seconds(99),
        },
    )
    .await
    .unwrap();
    let out = work_once(
        &store,
        &FakeRepo::default(),
        &harness_text(""),
        &FixedClock(Timestamp::from_unix_seconds(1)),
        &MemorySink::default(),
        &cfg(),
    )
    .await
    .unwrap();
    assert_eq!(out, None);
}
