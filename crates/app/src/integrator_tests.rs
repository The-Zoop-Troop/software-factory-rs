//! Integrator tests: rebase, checks, fast-forward, push, and every rejection path over fakes.

use domain::{AgentId, Attempts, Budget, FactoryMeta, Sha, Timestamp, Usage};

use super::*;
use crate::testing::{FakeRepo, FakeRunner, FakeStore, FixedClock, MemorySink};
use domain::Tokens;

fn id(s: &str) -> BeadId {
    BeadId::try_new(s).unwrap()
}
fn sha(c: char) -> Sha {
    Sha::try_new(core::iter::repeat_n(c, 40).collect::<String>()).unwrap()
}
fn cfg(checks: &[&str], remote: Option<&str>) -> IntegrateConfig {
    IntegrateConfig {
        main: BranchName::try_new("main").unwrap(),
        remote: remote.map(str::to_owned),
        checks: checks
            .iter()
            .map(|c| VerifyCommand::try_new(*c).expect("test command"))
            .collect(),
        check_timeout: Duration::from_seconds(10),
        protected: vec![],
    }
}

/// A task in `mergeable` at branch task/fac-t @ 'b', with a merge bead.
async fn store_mergeable(attempts: Attempts) -> FakeStore {
    let store = FakeStore::default();
    store
        .seed_task(
            id("fac-t"),
            FactoryMeta {
                verify_bead: id("fac-v"),
                base: sha('a'),
                budget: Budget {
                    attempts,
                    ..Budget::default()
                },
                usage: Usage::default(),
                lease_expiries: Attempts::new(0),
                blocked_releases: Attempts::new(0),
                state: TaskState::Open,
            },
        )
        .await;
    store.seed_plain(id("fac-v"), "verify").await;
    let now = Timestamp::from_unix_seconds(0);
    let w = AgentId::try_new("w").unwrap();
    apply_event(
        &store,
        &id("fac-t"),
        Event::Claim {
            holder: w.clone(),
            now,
            ttl: Duration::from_seconds(9),
        },
    )
    .await
    .unwrap();
    apply_event(
        &store,
        &id("fac-t"),
        Event::Submit {
            holder: w,
            branch: BranchName::try_new("task/fac-t").unwrap(),
            head: sha('b'),
            now,
            tokens: Tokens::new(1),
        },
    )
    .await
    .unwrap();
    apply_event(&store, &id("fac-t"), Event::VerifyPassed)
        .await
        .unwrap(); // creates the merge bead
    store
}

#[tokio::test]
async fn lands_rebased_head_runs_checks_pushes_and_closes() {
    let store = store_mergeable(Attempts::new(3)).await;
    let mut repo = FakeRepo::default();
    repo.rebased_to.insert(sha('b'), sha('c'));
    let mut runner = FakeRunner::default();
    runner
        .script
        .insert("cargo test".into(), FakeRunner::ok(""));
    let log = MemorySink::default();
    let report = integrate_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(5)),
        &log,
        &cfg(&["cargo test"], Some("origin")),
        "i",
    )
    .await
    .unwrap();
    assert_eq!(
        report,
        IntegrateReport {
            landed: 1,
            ..IntegrateReport::default()
        }
    );
    assert_eq!(
        load_task(&store, &id("fac-t")).await.unwrap().state,
        TaskState::Closed { merged: sha('c') }
    );
    assert_eq!(
        *repo.fast_forwards.lock().unwrap(),
        vec![(BranchName::try_new("main").unwrap(), sha('c'))]
    );
    assert_eq!(repo.pushes.lock().unwrap().len(), 1);
    let kinds: Vec<_> = log.events().await.into_iter().map(|e| e.kind).collect();
    assert!(
        matches!(
            &kinds[..],
            [
                EventKind::IntegrateStarted { .. },
                EventKind::Integrated {
                    landed: Some(_),
                    ..
                }
            ]
        ),
        "integrate_started precedes integrated: {kinds:?}"
    );
    assert!(
        store.list_active(BeadKind::Merge).await.unwrap().is_empty(),
        "merge bead closed"
    );
    assert_eq!(
        store.show(&id("fac-v")).await.unwrap().status,
        crate::BeadStatus::Closed,
        "verify bead closed"
    );
    assert_eq!(repo.removed.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn conflict_reopens_task_with_detail_and_closes_merge_bead() {
    let store = store_mergeable(Attempts::new(3)).await;
    let mut repo = FakeRepo::default();
    repo.conflicting.push(sha('b'));
    let runner = FakeRunner::default();
    let log = MemorySink::default();
    let report = integrate_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(5)),
        &log,
        &cfg(&[], None),
        "i",
    )
    .await
    .unwrap();
    assert_eq!(report.failed, 1);
    let task = load_task(&store, &id("fac-t")).await.unwrap();
    assert_eq!(task.state, TaskState::Open);
    assert_eq!(task.usage.attempts, Attempts::new(1));
    assert!(
        store
            .show(&id("fac-t"))
            .await
            .unwrap()
            .notes
            .unwrap()
            .contains("conflicted in")
    );
    assert!(store.list_active(BeadKind::Merge).await.unwrap().is_empty());
    assert!(repo.fast_forwards.lock().unwrap().is_empty());
}

#[tokio::test]
async fn failing_check_rejects_without_touching_main() {
    let store = store_mergeable(Attempts::new(3)).await;
    let repo = FakeRepo::default();
    let mut runner = FakeRunner::default();
    runner
        .script
        .insert("cargo test".into(), FakeRunner::fail(1, "test x FAILED"));
    let log = MemorySink::default();
    let report = integrate_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(5)),
        &log,
        &cfg(&["cargo test"], None),
        "i",
    )
    .await
    .unwrap();
    assert_eq!(report.failed, 1);
    assert!(repo.fast_forwards.lock().unwrap().is_empty());
    assert!(
        store
            .show(&id("fac-t"))
            .await
            .unwrap()
            .notes
            .unwrap()
            .contains("test x FAILED")
    );
}

#[tokio::test]
async fn push_failure_is_infra_error_and_leaves_task_mergeable() {
    let store = store_mergeable(Attempts::new(3)).await;
    let repo = FakeRepo {
        push_fails: true,
        ..FakeRepo::default()
    };
    let runner = FakeRunner::default();
    let log = MemorySink::default();
    let report = integrate_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(5)),
        &log,
        &cfg(&[], Some("origin")),
        "i",
    )
    .await
    .unwrap();
    assert_eq!(report.errors, 1);
    assert!(matches!(
        load_task(&store, &id("fac-t")).await.unwrap().state,
        TaskState::Mergeable { .. }
    ));
    // Compensation ran: main was moved back to where it was before the fast-forward.
    {
        let rollbacks = repo.rollbacks.lock().unwrap();
        assert_eq!(rollbacks.len(), 1);
        assert_eq!(rollbacks[0].1, sha('b'), "from the landed head");
    }
    assert_eq!(
        store.list_active(BeadKind::Merge).await.unwrap().len(),
        1,
        "merge bead kept for retry"
    );
}

#[tokio::test]
async fn stale_merge_bead_is_closed() {
    let store = FakeStore::default();
    store
        .seed_task(
            id("fac-t"),
            FactoryMeta {
                verify_bead: id("fac-v"),
                base: sha('a'),
                budget: Budget::default(),
                usage: Usage::default(),
                lease_expiries: Attempts::new(0),
                blocked_releases: Attempts::new(0),
                state: TaskState::Open,
            },
        )
        .await;
    store
        .seed_merge(id("fac-m"), id("fac-t"), "task/fac-t", sha('b'))
        .await;
    let repo = FakeRepo::default();
    let runner = FakeRunner::default();
    let log = MemorySink::default();
    let report = integrate_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(5)),
        &log,
        &cfg(&[], None),
        "i",
    )
    .await
    .unwrap();
    assert_eq!(report.skipped, 1);
    assert!(store.list_active(BeadKind::Merge).await.unwrap().is_empty());
    assert!(repo.added.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_remote_ahead_is_fast_forwarded_before_landing_and_divergence_is_rejected() {
    let store = store_mergeable(Attempts::new(3)).await;
    let repo = FakeRepo {
        rebased_to: std::collections::BTreeMap::from([(sha('b'), sha('c'))]),
        remote_head: Some(sha('e')),
        ..FakeRepo::default()
    };
    let runner = FakeRunner::default();
    let log = MemorySink::default();
    let report = integrate_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(9)),
        &log,
        &cfg(&[], Some("origin")),
        "i",
    )
    .await
    .unwrap();
    assert_eq!(report.landed, 1);
    let syncs = repo.syncs.lock().unwrap().clone();
    assert_eq!(syncs.len(), 1, "fetched from the remote before rebasing");
    assert_eq!(syncs[0].0, "origin");

    let store = store_mergeable(Attempts::new(3)).await;
    let repo = FakeRepo {
        remote_head: Some(sha('e')),
        remote_diverged: true,
        ..FakeRepo::default()
    };
    let report = integrate_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(9)),
        &log,
        &cfg(&[], Some("origin")),
        "i",
    )
    .await
    .unwrap();
    assert_eq!((report.landed, report.failed), (0, 1));
    assert!(
        repo.pushes.lock().unwrap().is_empty(),
        "nothing pushed over a diverged branch"
    );
    let task = load_task(&store, &id("fac-t")).await.unwrap();
    assert!(
        matches!(task.state, TaskState::Open),
        "task reopened with the reason"
    );
    let notes = store
        .show(&id("fac-t"))
        .await
        .unwrap()
        .notes
        .unwrap_or_default();
    assert!(notes.contains("diverged"), "{notes}");
}
