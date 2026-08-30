//! The Integrator v0 (ARCHITECTURE.md §4.4): land verified branches on `main` one at a
//! time — rebase, run the project's own check suite, fast-forward, push. The only
//! component that pushes to the remote. Batch-then-bisect is Phase 1.

use domain::{BeadId, BeadKind, BranchName, Duration, Event, MergeMeta, TaskState, VerifyCommand};

use crate::events::{EventKind, FactoryEvent};
use crate::ports::{BeadStore, Clock, EventSink, Repo, RepoError, RunError, Runner, StoreError};
use crate::transition::{TransitionError, apply_event, load_task};

/// How to integrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrateConfig {
    pub main: BranchName,
    /// Remote to push `main` to after landing; `None` for local-only rigs.
    pub remote: Option<String>,
    /// Project-wide checks run on the rebased head before it lands. Empty = trust the verify bead.
    pub checks: Vec<VerifyCommand>,
    pub check_timeout: Duration,
    /// Branches the factory must never integrate into or push (`main`, `master` by default).
    /// A rig whose integration branch is protected refuses to run at all.
    pub protected: Vec<BranchName>,
}

/// What one integration pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IntegrateReport {
    pub landed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// Failure to list merge beads; per-bead failures are counted in the report.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntegratorError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(
        "integration branch `{branch}` is protected; the factory never lands on it (set RIG_MAIN / --main to a feature branch, or change --protected deliberately)"
    )]
    ProtectedBranch { branch: BranchName },
}

/// Land every open merge bead whose task is `mergeable`, in listing order.
///
/// # Errors
/// Only when the ledger cannot be listed.
#[tracing::instrument(skip_all, err)]
pub async fn integrate_once(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    clock: &dyn Clock,
    log: &dyn EventSink,
    cfg: &IntegrateConfig,
    actor: &str,
) -> Result<IntegrateReport, IntegratorError> {
    if cfg.protected.contains(&cfg.main) {
        return Err(IntegratorError::ProtectedBranch {
            branch: cfg.main.clone(),
        });
    }
    let mut report = IntegrateReport::default();
    for bead in store.list_active(BeadKind::Merge).await? {
        let Some(meta) = bead.merge.clone() else {
            report.skipped += 1;
            continue;
        };
        let outcome = integrate_one(store, repo, runner, cfg, &bead.id, &meta).await;
        let kind = match &outcome {
            Ok(Some(Landed::Yes(sha))) => {
                report.landed += 1;
                EventKind::Integrated {
                    merge_bead: bead.id.clone(),
                    landed: Some(sha.clone()),
                    rejection: None,
                }
            }
            Ok(Some(Landed::No(rejection))) => {
                report.failed += 1;
                EventKind::Integrated {
                    merge_bead: bead.id.clone(),
                    landed: None,
                    rejection: Some(rejection.clone()),
                }
            }
            Ok(None) => {
                report.skipped += 1;
                continue;
            }
            Err(e) => {
                report.errors += 1;
                EventKind::Error {
                    detail: e.to_string(),
                }
            }
        };
        log.record(&FactoryEvent {
            at: clock.now(),
            actor: actor.to_owned(),
            bead: Some(meta.task.clone()),
            kind,
        });
    }
    Ok(report)
}

/// Why a branch could not land. Reopens the task with this as the note.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum LandRejection {
    Conflict {
        onto: BranchName,
        paths: Vec<std::path::PathBuf>,
    },
    CheckFailed {
        command: VerifyCommand,
        exit_code: Option<i32>,
        timed_out: bool,
        tail: String,
    },
}

impl std::fmt::Display for LandRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict { onto, paths } => {
                write!(f, "rebase onto {onto} conflicted in {paths:?}")
            }
            Self::CheckFailed {
                command,
                exit_code,
                timed_out,
                tail,
            } => {
                write!(
                    f,
                    "check `{command}` failed after rebase (exit {exit_code:?}{})",
                    if *timed_out { ", timed out" } else { "" }
                )?;
                if !tail.is_empty() {
                    write!(f, "\n{tail}")?;
                }
                Ok(())
            }
        }
    }
}

enum Landed {
    Yes(domain::Sha),
    No(LandRejection),
}

/// `None` if the task is no longer mergeable (stale merge bead — closed as such).
async fn integrate_one(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    cfg: &IntegrateConfig,
    merge_bead: &BeadId,
    meta: &MergeMeta,
) -> Result<Option<Landed>, TransitionError> {
    let task = load_task(store, &meta.task).await?;
    let TaskState::Mergeable { branch, head } = &task.state else {
        store
            .close(merge_bead, "task no longer mergeable; stale merge bead")
            .await?;
        return Ok(None);
    };
    if branch != &meta.branch || head != &meta.head {
        store
            .close(
                merge_bead,
                "merge bead does not match task's current branch/head",
            )
            .await?;
        return Ok(None);
    }

    let worktree = repo.worktree_add(branch, head).await?;
    let attempt = land(repo, runner, cfg, &worktree).await;
    repo.worktree_remove(worktree).await?;

    match attempt {
        Ok(new_head) => {
            apply_event(
                store,
                &meta.task,
                Event::Merged {
                    merged: new_head.clone(),
                },
            )
            .await?;
            store
                .close(merge_bead, &format!("landed on {} as {new_head}", cfg.main))
                .await?;
            Ok(Some(Landed::Yes(new_head)))
        }
        Err(LandError::Rejected(rejection)) => {
            let detail = rejection.to_string();
            apply_event(
                store,
                &meta.task,
                Event::MergeFailed {
                    detail: detail.clone(),
                },
            )
            .await?;
            store
                .close(merge_bead, "integration failed; task reopened")
                .await?;
            Ok(Some(Landed::No(rejection)))
        }
        // Infrastructure trouble (remote down, git broken): leave everything as it was and retry later.
        Err(LandError::Infra(e)) => Err(TransitionError::Repo(e)),
        Err(LandError::Runner(e)) => Err(TransitionError::Run(e)),
    }
}

enum LandError {
    /// The branch itself is at fault: conflict or failing checks. Reopens the task.
    Rejected(LandRejection),
    /// Our side is at fault. Nothing changes; try again next pass.
    Infra(RepoError),
    /// The check runner could not start.
    Runner(RunError),
}

/// Rebase → checks → fast-forward → push. Returns the sha now at the tip of `main`.
async fn land(
    repo: &dyn Repo,
    runner: &dyn Runner,
    cfg: &IntegrateConfig,
    worktree: &crate::ports::Worktree,
) -> Result<domain::Sha, LandError> {
    let new_head = match repo.rebase(worktree, &cfg.main).await {
        Ok(sha) => sha,
        Err(RepoError::Conflict { paths }) => {
            return Err(LandError::Rejected(LandRejection::Conflict {
                onto: cfg.main.clone(),
                paths,
            }));
        }
        Err(
            e @ (RepoError::RefNotFound { .. }
            | RepoError::NotFastForward { .. }
            | RepoError::Rejected { .. }
            | RepoError::Unavailable { .. }),
        ) => {
            return Err(LandError::Infra(e));
        }
    };

    for check in &cfg.checks {
        let out = runner
            .run(&worktree.path, check.as_ref(), cfg.check_timeout)
            .await
            .map_err(LandError::Runner)?;
        if !out.succeeded() {
            let tail = out
                .stderr
                .chars()
                .rev()
                .take(2000)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            return Err(LandError::Rejected(LandRejection::CheckFailed {
                command: check.clone(),
                exit_code: out.exit_code,
                timed_out: out.timed_out,
                tail,
            }));
        }
    }

    // Saga: fast-forward is the first step with a side effect; push is the second. If push
    // fails, compensate by moving `main` back so a retry starts from a clean state.
    let before = repo.head_of(&cfg.main).await.map_err(LandError::Infra)?;
    match repo.fast_forward(&cfg.main, &new_head).await {
        Ok(()) => {}
        // Someone else moved main between rebase and ff: not the branch's fault, retry next pass.
        Err(e) => return Err(LandError::Infra(e)),
    }
    if let Some(remote) = &cfg.remote
        && let Err(push_err) = repo.push(remote, &cfg.main).await
    {
        match repo.rollback(&cfg.main, &new_head, &before).await {
            Ok(()) => {
                tracing::warn!(error = %push_err, "push failed; main rolled back");
            }
            Err(rb) => {
                tracing::error!(error = %push_err, rollback = %rb, "push failed and rollback failed; main is ahead of the remote");
            }
        }
        return Err(LandError::Infra(push_err));
    }
    Ok(new_head)
}

#[cfg(test)]
mod tests {
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
}

#[cfg(test)]
mod protected_tests {
    use super::*;
    use crate::testing::{FakeRepo, FakeRunner, FakeStore, FixedClock, MemorySink};

    #[tokio::test]
    async fn a_protected_integration_branch_is_refused_before_anything_runs() {
        let store = FakeStore::default();
        let repo = FakeRepo::default();
        let runner = FakeRunner::default();
        let clock = FixedClock(domain::Timestamp::from_unix_seconds(0));
        let log = MemorySink::default();
        let main = BranchName::try_new("main").expect("branch");
        let cfg = IntegrateConfig {
            main: main.clone(),
            remote: Some("origin".into()),
            checks: vec![],
            check_timeout: Duration::from_minutes(1),
            protected: vec![main.clone(), BranchName::try_new("master").expect("branch")],
        };
        let err = integrate_once(&store, &repo, &runner, &clock, &log, &cfg, "i")
            .await
            .expect_err("refused");
        assert_eq!(err, IntegratorError::ProtectedBranch { branch: main });
        assert!(err.to_string().contains("protected"));
        assert!(log.events().await.is_empty());
    }
}
