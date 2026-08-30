//! The Verifier (ARCHITECTURE.md §4.3). No LLM: check out the branch in a fresh worktree,
//! run the verify bead's commands verbatim, report the fact to the state machine.

use std::fmt::Write as _;

use domain::{BeadKind, Event, NonEmpty, TaskState, VerifyCommand, VerifyMeta};

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
            Ok(Outcome::Blocked { reason: detail }) => {
                report.failed += 1;
                log.record(&FactoryEvent {
                    at: clock.now(),
                    actor: actor.to_owned(),
                    bead: Some(meta.task.clone()),
                    kind: EventKind::VerifyBlocked {
                        verify_bead: bead.id.clone(),
                        detail,
                    },
                });
            }
            Ok(outcome @ (Outcome::Passed | Outcome::Failed)) => {
                let passed = outcome == Outcome::Passed;
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
            Ok(Outcome::NotAwaiting) => report.skipped += 1,
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

/// What one verify run concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Passed,
    Failed,
    /// The rig could not run the checks; the task is an incident and no attempt was charged.
    Blocked {
        reason: String,
    },
    NotAwaiting,
}

async fn verify_one(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    meta: &VerifyMeta,
) -> Result<Outcome, TransitionError> {
    let task = load_task(store, &meta.task).await?;
    let TaskState::InVerify { branch, head } = &task.state else {
        return Ok(Outcome::NotAwaiting);
    };
    let worktree = repo.worktree_add(branch, head).await?;
    let result = run_all(runner, &worktree.path, meta).await;
    // Remove the worktree before deciding, so a store failure can't leak a checkout.
    repo.worktree_remove(worktree).await?;

    let (passed, note) = summarize(&meta.commands, &result);
    let blocked = environmental(&result);
    let (event, outcome) = match (blocked, passed) {
        (_, true) => (Event::VerifyPassed, Outcome::Passed),
        (Some(detail), false) => (
            Event::VerifyBlocked {
                note: format!("{detail}\n{note}"),
            },
            Outcome::Blocked { reason: detail },
        ),
        (None, false) => (Event::VerifyFailed { note }, Outcome::Failed),
    };
    apply_event(store, &meta.task, event).await?;
    Ok(outcome)
}

/// Run commands in order; stop at the first failure. `Err` means the command couldn't spawn.
async fn run_all(
    runner: &dyn Runner,
    cwd: &std::path::Path,
    meta: &VerifyMeta,
) -> Vec<Result<RunOutput, crate::ports::RunError>> {
    let mut results = Vec::with_capacity(meta.commands.len());
    for cmd in &meta.commands {
        let r = runner.run(cwd, cmd.as_ref(), meta.timeout).await;
        let stop = !matches!(&r, Ok(o) if o.succeeded());
        results.push(r);
        if stop {
            break;
        }
    }
    results
}

/// Signatures of a rig that cannot run the checks, as opposed to checks that fail.
const ENVIRONMENT_SIGNATURES: [&str; 9] = [
    "permission denied",
    "no space left on device",
    "read-only file system",
    "cannot execute binary file",
    "command not found",
    "could not resolve host",
    "connection refused",
    "network is unreachable",
    "temporary failure in name resolution",
];

/// Pure: was the first failure environmental? Exit 126/127 (not executable / not found) or a
/// known signature in the output. Returns the one-line reason to put on the incident.
#[must_use]
pub fn environmental(results: &[Result<RunOutput, crate::ports::RunError>]) -> Option<String> {
    let first_failure = results
        .iter()
        .find(|r| !matches!(r, Ok(o) if o.succeeded()))?;
    match first_failure {
        Err(e) => Some(format!("verify command could not run: {e}")),
        Ok(o) => {
            let text = format!("{}\n{}", o.stdout, o.stderr).to_ascii_lowercase();
            let by_code = matches!(o.exit_code, Some(126 | 127)).then(|| {
                format!(
                    "verify command exited {} (not executable / not found)",
                    o.exit_code.unwrap_or(0)
                )
            });
            by_code.or_else(|| {
                ENVIRONMENT_SIGNATURES
                    .iter()
                    .find(|sig| text.contains(*sig))
                    .map(|sig| format!("verify output contains `{sig}`"))
            })
        }
    }
}

/// Pure: decide pass/fail and build the note that goes on the task bead.
fn summarize(
    commands: &NonEmpty<VerifyCommand>,
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
                let _ = write!(note, "[could not run: {e}]");
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

#[cfg(test)]
mod tests {
    use domain::{
        AgentId, BeadId, BranchName, Budget, Duration, FactoryMeta, Sha, Timestamp, Usage,
    };

    use super::*;
    use crate::testing::{FakeRepo, FakeRunner, FakeStore, FixedClock, MemorySink};
    use domain::{Attempts, Tokens};

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
                        attempts: Attempts::new(1),
                        ..Budget::default()
                    },
                    usage: Usage::default(),
                    lease_expiries: Attempts::new(0),
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
                tokens: Tokens::new(1),
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
                    lease_expiries: Attempts::new(0),
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
        let cmds = NonEmpty::singleton(VerifyCommand::try_new("sleep 99").unwrap());
        let (passed, note) = summarize(&cmds, &[Ok(out)]);
        assert!(!passed);
        assert!(note.contains("timed out"));
    }

    #[test]
    fn tail_respects_char_boundaries() {
        let s = "é".repeat(NOTE_TAIL);
        assert!(tail(&s).chars().all(|c| c == 'é'));
    }
}

#[cfg(test)]
mod environment_tests {
    use super::*;
    use crate::testing::FakeRunner;

    #[allow(clippy::unnecessary_wraps, reason = "the classifier takes results")]
    fn out(code: i32, stderr: &str) -> Result<RunOutput, crate::ports::RunError> {
        Ok(RunOutput {
            exit_code: Some(code),
            ..FakeRunner::fail(code, stderr)
        })
    }

    #[test]
    fn classifies_environment_failures_and_leaves_real_failures_alone() {
        assert!(environmental(&[Ok(FakeRunner::ok("fine"))]).is_none());
        assert!(environmental(&[out(1, "assertion failed: expected 2 got 3")]).is_none());
        assert!(
            environmental(&[out(127, "sh: docker: not found")])
                .unwrap()
                .contains("127")
        );
        assert!(environmental(&[out(126, "")]).unwrap().contains("126"));
        assert!(
            environmental(&[out(1, "fork/exec /tmp/go-build/x.test: Permission denied")])
                .unwrap()
                .contains("permission denied")
        );
        assert!(
            environmental(&[out(2, "write /work: no space left on device")])
                .unwrap()
                .contains("no space")
        );
        assert!(
            environmental(&[
                Ok(FakeRunner::ok("ok")),
                out(1, "curl: (6) Could not resolve host: example.com")
            ])
            .unwrap()
            .contains("could not resolve")
        );
        let spawn = crate::ports::RunError {
            command: "sh".into(),
            cause: crate::Unavailable::NotInstalled,
            detail: "no such file".into(),
        };
        assert!(
            environmental(&[Err(spawn)])
                .unwrap()
                .contains("could not run")
        );
    }
}
