//! The Verifier (ARCHITECTURE.md §4.3). No LLM: check out the branch in a fresh worktree,
//! run the verify bead's commands verbatim, report the fact to the state machine.

use std::fmt::Write as _;

use domain::{BeadKind, Event, TaskState, VerifyCommand, VerifyMeta};

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
        match verify_one(store, repo, runner, clock, log, actor, &bead.id, &meta).await {
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

#[allow(
    clippy::too_many_arguments,
    reason = "ports plus the stage identity; nothing outlives the call"
)]
async fn verify_one(
    store: &dyn BeadStore,
    repo: &dyn Repo,
    runner: &dyn Runner,
    clock: &dyn Clock,
    log: &dyn EventSink,
    actor: &str,
    verify_bead: &domain::BeadId,
    meta: &VerifyMeta,
) -> Result<Outcome, TransitionError> {
    let task = load_task(store, &meta.task).await?;
    let TaskState::InVerify { branch, head } = &task.state else {
        return Ok(Outcome::NotAwaiting);
    };
    log.record(&FactoryEvent {
        at: clock.now(),
        actor: actor.to_owned(),
        bead: Some(meta.task.clone()),
        kind: EventKind::VerifyStarted {
            verify_bead: verify_bead.clone(),
        },
    });
    let worktree = repo.worktree_add(branch, head).await?;
    // The checkout is fresh: dependencies are installed by the prepare step, never by the plan.
    let commands: Vec<VerifyCommand> = prepare_for(&worktree.path)
        .into_iter()
        .chain(meta.commands.iter().cloned())
        .collect();
    let result = run_all(runner, &worktree.path, &commands, meta.timeout).await;
    // Remove the worktree before deciding, so a store failure can't leak a checkout.
    repo.worktree_remove(worktree).await?;

    let (passed, note) = summarize(&commands, &result);
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
/// Commands that make a fresh checkout verifiable, before the plan's own: `[verify] prepare`
/// in `.factory/runtime.toml`, or a default from the lockfile present (`npm ci`, frozen pnpm /
/// yarn installs, `go mod download`). Nothing for Rust: cargo fetches on its own.
#[must_use]
pub fn prepare_for(worktree: &std::path::Path) -> Vec<VerifyCommand> {
    #[derive(serde::Deserialize, Default)]
    struct Spec {
        #[serde(default)]
        verify: Verify,
    }
    #[derive(serde::Deserialize, Default)]
    struct Verify {
        #[serde(default)]
        prepare: Option<Vec<String>>,
    }
    let declared = std::fs::read_to_string(worktree.join(".factory/runtime.toml"))
        .ok()
        .and_then(|t| toml::from_str::<Spec>(&t).ok())
        .and_then(|s| s.verify.prepare);
    let submodules = worktree
        .join(".gitmodules")
        .exists()
        .then(|| "git submodule update --init --recursive".to_owned());
    let defaults = || {
        [
            ("package-lock.json", "npm ci"),
            ("pnpm-lock.yaml", "pnpm install --frozen-lockfile"),
            ("yarn.lock", "yarn install --frozen-lockfile"),
            ("go.sum", "go mod download"),
        ]
        .into_iter()
        .filter(|(lock, _)| worktree.join(lock).exists())
        .map(|(_, cmd)| cmd.to_owned())
        .take(1)
        .collect::<Vec<_>>()
    };
    submodules
        .into_iter()
        .chain(declared.unwrap_or_else(defaults))
        .filter_map(|c| VerifyCommand::try_new(&c).ok())
        .collect()
}

async fn run_all(
    runner: &dyn Runner,
    cwd: &std::path::Path,
    commands: &[VerifyCommand],
    timeout: domain::Duration,
) -> Vec<Result<RunOutput, crate::ports::RunError>> {
    let mut results = Vec::with_capacity(commands.len());
    for cmd in commands {
        let r = runner.run(cwd, cmd.as_ref(), timeout).await;
        let stop = !matches!(&r, Ok(o) if o.succeeded());
        results.push(r);
        if stop {
            break;
        }
    }
    results
}

/// Signatures of a rig that cannot run the checks, as opposed to checks that fail.
/// Includes interpreter missing-dependency aborts (Python/Node/Ruby): those exit 1, not
/// 126/127, and mean the runtime image lacks a module the verify command needs.
const ENVIRONMENT_SIGNATURES: [&str; 13] = [
    "permission denied",
    "no space left on device",
    "read-only file system",
    "cannot execute binary file",
    "command not found",
    "could not find `protoc`",
    "could not resolve host",
    "connection refused",
    "network is unreachable",
    "temporary failure in name resolution",
    "no module named",
    "cannot find module",
    "cannot load such file",
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
    commands: &[VerifyCommand],
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
#[path = "verifier_tests.rs"]
mod tests;
