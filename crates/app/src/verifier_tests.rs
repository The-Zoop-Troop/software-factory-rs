//! Verifier tests: pass/fail/blocked outcomes over fakes, environment signatures, prepare step.

use domain::{AgentId, BeadId, BranchName, Budget, Duration, FactoryMeta, Sha, Timestamp, Usage};

use super::*;
use crate::testing::{FakeRepo, FakeRunner, FakeStore, FixedClock, MemorySink};
use domain::{Attempts, Tokens};

fn id(s: &str) -> BeadId {
    BeadId::try_new(s).unwrap()
}
fn main() -> BranchName {
    BranchName::try_new("main").unwrap()
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
                blocked_releases: Attempts::new(0),
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
        &main(),
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
    let kinds: Vec<_> = log.events().await.into_iter().map(|e| e.kind).collect();
    assert!(
        matches!(
            &kinds[..],
            [
                EventKind::VerifyStarted { .. },
                EventKind::Verified { passed: true, .. }
            ]
        ),
        "verify_started precedes verified: {kinds:?}"
    );
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
        &main(),
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
                blocked_releases: Attempts::new(0),
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
        &main(),
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
    let cmds = vec![VerifyCommand::try_new("sleep 99").unwrap()];
    let (passed, note) = summarize(&cmds, &[Ok(out)]);
    assert!(!passed);
    assert!(note.contains("timed out"));
}

#[test]
fn tail_respects_char_boundaries() {
    let s = "é".repeat(NOTE_TAIL);
    assert!(tail(&s).chars().all(|c| c == 'é'));
}

mod environment_tests {
    use super::super::*;
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
        // Interpreter missing-dependency aborts exit 1 but are the image's fault, not the task's.
        assert!(
            environmental(&[out(1, "ModuleNotFoundError: No module named 'yaml'")])
                .unwrap()
                .contains("no module named")
        );
        assert!(
            environmental(&[out(1, "Error: Cannot find module 'js-yaml'")])
                .unwrap()
                .contains("cannot find module")
        );
        assert!(
            environmental(&[out(1, "LoadError: cannot load such file -- yaml")])
                .unwrap()
                .contains("cannot load such file")
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

#[tokio::test]
async fn prepare_commands_come_from_the_integration_branch_not_the_candidate() {
    let store = store_in_verify().await;
    let root = std::env::temp_dir().join(format!("factory-verify-base-{}", std::process::id()));
    let candidate = root.join("task__fac-t/.factory");
    std::fs::create_dir_all(&candidate).unwrap();
    std::fs::write(
        candidate.join("runtime.toml"),
        "[verify]\nprepare = [\"echo chosen by the candidate\"]\n",
    )
    .unwrap();
    let mut repo = FakeRepo {
        worktree_root: Some(root.clone()),
        ..FakeRepo::default()
    };
    repo.base_files.insert(
        RUNTIME_TOML.into(),
        "[verify]\nprepare = [\"echo from the base\"]\n".into(),
    );
    let mut runner = FakeRunner::default();
    for cmd in ["echo from the base", "cargo test", "cargo clippy"] {
        runner.script.insert(cmd.into(), FakeRunner::ok(""));
    }
    let log = MemorySink::default();
    verify_once(
        &store,
        &repo,
        &runner,
        &FixedClock(Timestamp::from_unix_seconds(1)),
        &log,
        "v",
        &main(),
    )
    .await
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let ran: Vec<String> = runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|(_, c)| c.clone())
        .collect();
    assert_eq!(
        ran.first().map(String::as_str),
        Some("echo from the base"),
        "{ran:?}"
    );
    assert!(!ran.iter().any(|c| c.contains("candidate")), "{ran:?}");
}

mod prepare_tests {
    use super::super::prepare_for;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("factory-prepare-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(d.join(".factory"));
        d
    }

    /// The checkout's own runtime.toml stands in for the integration branch's here; the
    /// verify-level test below proves which one the Verifier actually reads.
    fn names(d: &std::path::Path) -> Vec<String> {
        let runtime = std::fs::read_to_string(d.join(".factory/runtime.toml")).ok();
        prepare_for(d, runtime.as_deref())
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn lockfiles_pick_a_default_and_runtime_toml_overrides_it() {
        assert!(
            names(&dir("none")).is_empty(),
            "nothing to prepare without a lockfile"
        );
        let npm = dir("npm");
        let _ = std::fs::write(npm.join("package-lock.json"), "{}");
        assert_eq!(names(&npm), ["npm ci"]);
        let go = dir("go");
        let _ = std::fs::write(go.join("go.sum"), "");
        assert_eq!(names(&go), ["go mod download"]);
        let declared = dir("declared");
        let _ = std::fs::write(declared.join("package-lock.json"), "{}");
        let _ = std::fs::write(
            declared.join(".factory/runtime.toml"),
            "[runtime]\nname = \"node\"\n[verify]\nprepare = [\"corepack enable\", \"pnpm install --frozen-lockfile\"]\n",
        );
        assert_eq!(
            names(&declared),
            ["corepack enable", "pnpm install --frozen-lockfile"]
        );
        let subs = dir("subs");
        let _ = std::fs::write(subs.join(".gitmodules"), "[submodule \"a\"]");
        let _ = std::fs::write(subs.join("package-lock.json"), "{}");
        assert_eq!(
            names(&subs),
            ["git submodule update --init --recursive", "npm ci"],
            "submodule init comes before the install"
        );
        let empty = dir("empty");
        let _ = std::fs::write(empty.join("package-lock.json"), "{}");
        let _ = std::fs::write(
            empty.join(".factory/runtime.toml"),
            "[verify]\nprepare = []\n",
        );
        assert!(
            names(&empty).is_empty(),
            "an explicit empty list disables the default"
        );
    }
}
