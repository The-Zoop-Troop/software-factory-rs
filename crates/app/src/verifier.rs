//! The Verifier (ARCHITECTURE.md §4.3). No LLM: check out the branch in a fresh worktree,
//! run the verify bead's commands verbatim, report the fact to the state machine.

use std::fmt::Write as _;

use domain::{BeadKind, Event, TaskState, VerifyMeta};

use crate::events::{EventKind, FactoryEvent};
use crate::ports::{BeadStore, Clock, EventSink, Repo, RunOutput, Runner, StoreError};
use crate::transition::{TransitionError, apply_event, load_task};

/// What one verification pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VerifyReport {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// Failure to list verify beads; per-bead failures are counted in the report.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerifierError {
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Maximum characters of command output kept on the bead note.
const NOTE_TAIL: usize = 4000;

/// Verify every verify bead whose paired task is `in_verify`.
///
/// # Errors
/// Only when the ledger cannot be listed.
#[tracing::instrument(skip_all, err)]
pub async fn verify_once(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    clock: &dyn Clock,
    log: &dyn EventSink,
    actor: &str,
) -> Result<VerifyReport, VerifierError> {
    let mut report = VerifyReport::default();
    for bead in store.list_active(BeadKind::Verify).await? {
        let Some(meta) = bead.verify.clone() else {
            report.skipped += 1;
            continue;
        };
        match verify_one(store, repo, runner, &meta).await {
            Ok(Some(passed)) => {
                if passed {
                    report.passed += 1;
                } else {
                    report.failed += 1;
                }
                log.record(&FactoryEvent {
                    at: clock.now(),
                    actor: actor.to_owned(),
                    bead: Some(meta.task.clone()),
                    kind: EventKind::Verified {
                        passed,
                        verify_bead: bead.id.clone(),
                    },
                });
            }
            Ok(None) => report.skipped += 1,
            Err(e) => {
                report.errors += 1;
                log.record(&FactoryEvent {
                    at: clock.now(),
                    actor: actor.to_owned(),
                    bead: Some(bead.id.clone()),
                    kind: EventKind::Error {
                        detail: e.to_string(),
                    },
                });
            }
        }
    }
    Ok(report)
}

/// `Some(true)` pass, `Some(false)` fail, `None` if the task wasn't awaiting verification.
async fn verify_one(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    meta: &VerifyMeta,
) -> Result<Option<bool>, TransitionError> {
    let task = load_task(store, &meta.task).await?;
    let TaskState::InVerify { branch, head } = &task.state else {
        return Ok(None);
    };
    let worktree = repo
        .worktree_add(branch, head)
        .await
        .map_err(|e| to_store(&e))?;
    let result = run_all(runner, &worktree.path, meta).await;
    // Remove the worktree before deciding, so a store failure can't leak a checkout.
    repo.worktree_remove(worktree)
        .await
        .map_err(|e| to_store(&e))?;

    let (passed, note) = summarize(&meta.commands, &result);
    let event = if passed {
        Event::VerifyPassed
    } else {
        Event::VerifyFailed { note }
    };
    apply_event(store, &meta.task, event).await?;
    Ok(Some(passed))
}

/// Run commands in order; stop at the first failure. `Err` means the command couldn't spawn.
async fn run_all(
    runner: &dyn Runner,
    cwd: &std::path::Path,
    meta: &VerifyMeta,
) -> Vec<Result<RunOutput, crate::ports::RunError>> {
    let mut results = Vec::with_capacity(meta.commands.len());
    for cmd in &meta.commands {
        let r = runner.run(cwd, cmd, meta.timeout).await;
        let stop = !matches!(&r, Ok(o) if o.succeeded());
        results.push(r);
        if stop {
            break;
        }
    }
    results
}

/// Pure: decide pass/fail and build the note that goes on the task bead.
fn summarize(
    commands: &[String],
    results: &[Result<RunOutput, crate::ports::RunError>],
) -> (bool, String) {
    let all_ran = results.len() == commands.len();
    let all_ok = results.iter().all(|r| matches!(r, Ok(o) if o.succeeded()));
    let passed = all_ran && all_ok;
    let mut note = String::from(if passed {
        "verify PASSED"
    } else {
        "verify FAILED"
    });
    for (cmd, r) in commands.iter().zip(results) {
        let _ = write!(note, "\n$ {cmd}\n");
        match r {
            Ok(o) => {
                let status = match (o.timed_out, o.exit_code) {
                    (true, _) => "timed out".to_owned(),
                    (false, Some(c)) => format!("exit {c}"),
                    (false, None) => "killed".to_owned(),
                };
                let _ = writeln!(note, "[{status}]");
                note.push_str(tail(&o.stdout));
                if !o.stderr.is_empty() {
                    note.push_str("\n--- stderr ---\n");
                    note.push_str(tail(&o.stderr));
                }
            }
            Err(e) => {
                let _ = write!(note, "[could not run: {}]", e.reason);
            }
        }
    }
    (passed, note)
}

fn tail(s: &str) -> &str {
    let start = s.len().saturating_sub(NOTE_TAIL);
    // Back up to a char boundary so we never slice inside a UTF-8 sequence.
    let start = (start..=s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());
    s.get(start..).unwrap_or("")
}

fn to_store(e: &crate::ports::RepoError) -> TransitionError {
    TransitionError::Store(StoreError::Unavailable(format!("repo: {e}")))
}

#[cfg(test)]
mod tests {
    use domain::{
        AgentId, BeadId, BranchName, Budget, Duration, FactoryMeta, Sha, Timestamp, Usage,
    };

    use super::*;
    use crate::testing::{FakeRepo, FakeRunner, FakeStore, FixedClock, MemorySink};

    fn id(s: &str) -> BeadId {
        BeadId::try_new(s).unwrap()
    }
    fn sha(c: char) -> Sha {
        Sha::try_new(core::iter::repeat_n(c, 40).collect::<String>()).unwrap()
    }

    async fn store_in_verify() -> FakeStore {
        let store = FakeStore::default();
        store
            .seed_task(
                id("fac-t"),
                FactoryMeta {
                    verify_bead: id("fac-v"),
                    base: sha('a'),
                    budget: Budget {
                        attempts: 1,
                        ..Budget::default()
                    },
                    usage: Usage::default(),
                    lease_expiries: 0,
                    state: TaskState::Open,
                },
            )
            .await;
        let now = Timestamp::from_unix_seconds(0);
        apply_event(
            &store,
            &id("fac-t"),
            Event::Claim {
                holder: AgentId::try_new("w").unwrap(),
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
                holder: AgentId::try_new("w").unwrap(),
                branch: BranchName::try_new("task/fac-t").unwrap(),
                head: sha('b'),
                now,
                tokens: 1,
            },
        )
        .await
        .unwrap();
        store
            .seed_verify(id("fac-v"), id("fac-t"), &["cargo test", "cargo clippy"])
            .await;
        store
    }

    #[tokio::test]
    async fn pass_opens_merge_bead_and_cleans_worktree() {
        let store = store_in_verify().await;
        let repo = FakeRepo::default();
        let mut runner = FakeRunner::default();
        runner
            .script
            .insert("cargo test".into(), FakeRunner::ok("ok"));
        runner
            .script
            .insert("cargo clippy".into(), FakeRunner::ok(""));
        let log = MemorySink::default();
        let report = verify_once(
            &store,
            &repo,
            &runner,
            &FixedClock(Timestamp::from_unix_seconds(1)),
            &log,
            "v",
        )
        .await
        .unwrap();
        assert_eq!(
            report,
            VerifyReport {
                passed: 1,
                ..VerifyReport::default()
            }
        );
        assert!(matches!(
            load_task(&store, &id("fac-t")).await.unwrap().state,
            TaskState::Mergeable { .. }
        ));
        let merges = store.list_active(BeadKind::Merge).await.unwrap();
        assert_eq!(merges.len(), 1);
        assert_eq!(merges[0].merge.as_ref().unwrap().task, id("fac-t"));
        assert_eq!(repo.added.lock().unwrap().len(), 1);
        assert_eq!(repo.removed.lock().unwrap().len(), 1);
        assert_eq!(runner.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn failure_stops_at_first_command_and_reopens_with_output() {
        let store = store_in_verify().await;
        let repo = FakeRepo::default();
        let mut runner = FakeRunner::default();
        runner.script.insert(
            "cargo test".into(),
            FakeRunner::fail(101, "test foo ... FAILED"),
        );
        let log = MemorySink::default();
        let report = verify_once(
            &store,
            &repo,
            &runner,
            &FixedClock(Timestamp::from_unix_seconds(1)),
            &log,
            "v",
        )
        .await
        .unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(
            runner.calls.lock().unwrap().len(),
            1,
            "clippy must not run after a failure"
        );
        // attempts budget was 1, so this failure is an incident
        assert!(matches!(
            load_task(&store, &id("fac-t")).await.unwrap().state,
            TaskState::Incident { .. }
        ));
        let notes = store.show(&id("fac-t")).await.unwrap().notes.unwrap();
        assert!(notes.contains("verify FAILED"));
        assert!(notes.contains("exit 101"));
        assert!(notes.contains("test foo ... FAILED"));
        assert_eq!(repo.removed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn skips_when_task_not_in_verify() {
        let store = FakeStore::default();
        store
            .seed_task(
                id("fac-t"),
                FactoryMeta {
                    verify_bead: id("fac-v"),
                    base: sha('a'),
                    budget: Budget::default(),
                    usage: Usage::default(),
                    lease_expiries: 0,
                    state: TaskState::Open,
                },
            )
            .await;
        store.seed_verify(id("fac-v"), id("fac-t"), &["true"]).await;
        let repo = FakeRepo::default();
        let runner = FakeRunner::default();
        let log = MemorySink::default();
        let report = verify_once(
            &store,
            &repo,
            &runner,
            &FixedClock(Timestamp::from_unix_seconds(1)),
            &log,
            "v",
        )
        .await
        .unwrap();
        assert_eq!(report.skipped, 1);
        assert!(repo.added.lock().unwrap().is_empty());
    }

    #[test]
    fn summarize_marks_timeout() {
        let out = RunOutput {
            exit_code: None,
            stdout: "partial".into(),
            stderr: String::new(),
            timed_out: true,
        };
        let (passed, note) = summarize(&["sleep 99".into()], &[Ok(out)]);
        assert!(!passed);
        assert!(note.contains("timed out"));
    }

    #[test]
    fn tail_respects_char_boundaries() {
        let s = "é".repeat(NOTE_TAIL);
        assert!(tail(&s).chars().all(|c| c == 'é'));
    }
}
