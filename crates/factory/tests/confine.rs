//! `factory confine` on this kernel: a confined child writes its worktree and its own branch, and
//! cannot write the event log, the ledger files or the integration branch. Needs Landlock (ABI 3+),
//! which the rig's hosts and CI's runners have; without it this test fails rather than skipping,
//! because a skipped proof is no proof.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use infra::{ChildEnv, Confine};

/// A rig's layout in a temp dir: ledger dir with `.beads` and the event log, a clone whose
/// `main` is a loose ref, and a linked worktree on `task/x`.
struct Rig {
    root: PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("factory-confine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let rig = root.join("rig");
        std::fs::create_dir_all(rig.join(".beads")).unwrap();
        std::fs::create_dir_all(rig.join(".factory/worktrees")).unwrap();
        std::fs::write(rig.join(".beads/issues.db"), "ledger").unwrap();
        std::fs::write(
            rig.join(".factory/events.jsonl"),
            "{\"kind\":\"claimed\"}\n",
        )
        .unwrap();
        let sh = |cwd: &Path, script: &str| {
            let out = Command::new("sh")
                .arg("-c")
                .arg(script)
                .current_dir(cwd)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{script}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        sh(
            &rig,
            "git init -q -b main repo && cd repo && git config user.email t@t && git config user.name t \
             && echo base > README.md && git add . && git commit -qm base \
             && git worktree add -q -b task/x ../.factory/worktrees/task__x",
        );
        Self { root }
    }

    fn rig(&self) -> PathBuf {
        self.root.join("rig")
    }

    fn worktree(&self) -> PathBuf {
        self.rig().join(".factory/worktrees/task__x")
    }

    /// Run `script` in the worktree, confined as a Worker session would be.
    fn confined(&self, script: &str) -> (bool, String) {
        // The fixture lives under /tmp, which sessions may write, so the session's temp dir is
        // its own here; in a rig the ledger is not beneath /tmp.
        let home = self.root.join("home");
        let tmp = self.root.join("tmp");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        let env = ChildEnv::from_vars(
            [
                ("HOME".to_owned(), home.display().to_string()),
                ("TMPDIR".to_owned(), tmp.display().to_string()),
                ("PATH".to_owned(), "/usr/local/bin:/usr/bin:/bin".to_owned()),
            ],
            &[],
        );
        let confine =
            Confine::from_env(PathBuf::from(env!("CARGO_BIN_EXE_factory")), &env).unwrap();
        let mut cmd = Command::new(&confine.helper);
        cmd.arg("confine");
        for dir in confine.writable_for(&self.worktree()) {
            cmd.arg("--write").arg(dir);
        }
        let out = cmd
            .args(["--", "/bin/sh", "-c", script])
            .current_dir(self.worktree())
            .env_clear()
            .env("HOME", &home)
            .env("TMPDIR", &tmp)
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_confined_session_can_commit_to_its_own_branch() {
    let rig = Rig::new("a_confined_session_can_c");
    let (ok, out) = rig.confined(
        "echo change > feature.txt && git add feature.txt && git commit -qm work && git rev-parse HEAD",
    );
    assert!(ok, "{out}");
    let (ok, out) =
        rig.confined("mkdir -p $HOME/.cache && echo x > $HOME/.cache/x && echo y > $TMPDIR/ok");
    assert!(ok, "home and the temp dir stay writable: {out}");
}

#[test]
fn a_confined_session_cannot_touch_the_ledger_the_event_log_or_main() {
    let rig = Rig::new("a_confined_session_canno");
    let r = rig.rig();
    let attempts = [
        (
            "append to the event log",
            format!("echo forged >> {}/.factory/events.jsonl", r.display()),
        ),
        (
            "truncate the event log",
            format!(": > {}/.factory/events.jsonl", r.display()),
        ),
        (
            "delete the event log",
            format!("rm {}/.factory/events.jsonl", r.display()),
        ),
        (
            "edit the ledger files",
            format!("echo x > {}/.beads/issues.db", r.display()),
        ),
        (
            "add a ledger file",
            format!("touch {}/.beads/new", r.display()),
        ),
        (
            "move main with git",
            "git update-ref refs/heads/main HEAD~0".to_owned(),
        ),
        (
            "write main's ref file",
            format!("echo 0000 > {}/repo/.git/refs/heads/main", r.display()),
        ),
        (
            "change the repo's config",
            "git config --local core.hooksPath /tmp".to_owned(),
        ),
        (
            "write the main checkout",
            format!("echo x > {}/repo/README.md", r.display()),
        ),
    ];
    for (what, script) in attempts {
        let (ok, out) = rig.confined(&script);
        assert!(
            !ok,
            "{what} should fail under confinement but succeeded: {out}"
        );
    }
    let events = std::fs::read_to_string(r.join(".factory/events.jsonl")).unwrap();
    assert_eq!(
        events, "{\"kind\":\"claimed\"}\n",
        "the event log is unchanged"
    );
    assert_eq!(
        std::fs::read_to_string(r.join(".beads/issues.db")).unwrap(),
        "ledger"
    );
}

#[test]
fn the_same_attempts_succeed_unconfined() {
    // The control: without confinement the planted layout really is writable, so the test above
    // fails for the right reason.
    let rig = Rig::new("the_same_attempts_succee");
    let r = rig.rig();
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "echo forged >> {0}/.factory/events.jsonl && echo x > {0}/.beads/issues.db",
            r.display()
        ))
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn confine_without_a_program_is_an_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(["confine", "--write", "/tmp"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
