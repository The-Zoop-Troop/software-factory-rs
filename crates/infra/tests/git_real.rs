//! `GitCli` against a real git repository in a temp dir: worktrees, commit, rebase, ff, push.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use infra::GitCli;
use infra::app::domain::{BranchName, Sha};
use infra::app::{Repo, RepoError};

fn sh(dir: &Path, cmd: &str) -> String {
    let out = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{cmd}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A clone with one commit on main and a bare origin.
fn fixture() -> (PathBuf, PathBuf, GitCli) {
    let base = std::env::temp_dir().join(format!(
        "factory-git-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    let clone = base.join("clone");
    std::fs::create_dir_all(&base).unwrap();
    sh(
        &base,
        "git init -q --bare -b main origin.git && git init -q -b main clone",
    );
    sh(
        &clone,
        "git config user.email t@t && git config user.name t && git remote add origin ../origin.git && echo base > README.md && git add . && git commit -qm base && git push -q origin main",
    );
    let git = GitCli::new(&clone, base.join("wt"));
    (base, clone, git)
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn b(s: &str) -> BranchName {
    BranchName::try_new(s).unwrap()
}

#[tokio::test]
async fn branch_commit_rebase_ff_push_roundtrip() {
    let (_base, clone, git) = fixture();
    let main = git.head_of(&b("main")).await.unwrap();
    assert!(git.head_of(&b("nope")).await.is_err());

    // Worker: new branch from main, edit, commit.
    let wt = git.branch_worktree(&b("task/one"), &main).await.unwrap();
    std::fs::write(wt.path.join("one.txt"), "1").unwrap();
    let head = git.commit_all(&wt, "task one").await.unwrap();
    assert_ne!(head, main);
    assert_eq!(
        git.commit_all(&wt, "nothing").await.unwrap(),
        head,
        "no changes → same head"
    );
    git.worktree_remove(wt).await.unwrap();

    // main moves on independently (non-conflicting).
    sh(
        &clone,
        "echo other > other.txt && git add . && git commit -qm other",
    );
    let main_before = git.head_of(&b("main")).await.unwrap();

    // Integrator: detached worktree at the branch head, rebase, checks, ff, push.
    let wt = git.worktree_add(&b("task/one"), &head).await.unwrap();
    let rebased = git.rebase(&wt, &b("main")).await.unwrap();
    assert_ne!(rebased, head);
    git.worktree_remove(wt).await.unwrap();
    git.fast_forward(&b("main"), &rebased).await.unwrap();
    assert_eq!(git.head_of(&b("main")).await.unwrap(), rebased);
    assert!(
        clone.join("one.txt").exists(),
        "checked-out main was reset to the new head"
    );
    git.push("origin", &b("main")).await.unwrap();
    assert_eq!(
        sh(&clone, "git -C ../origin.git rev-parse main"),
        rebased.as_ref()
    );

    // Compensation: roll main back to the pre-landing head (CAS on the landed head), then refuse
    // to roll back again because main is no longer at that head.
    git.rollback(&b("main"), &rebased, &main_before)
        .await
        .unwrap();
    assert_eq!(git.head_of(&b("main")).await.unwrap(), main_before);
    assert!(
        !clone.join("one.txt").exists(),
        "checked-out main followed the rollback"
    );
    assert!(matches!(
        git.rollback(&b("main"), &rebased, &main_before).await,
        Err(RepoError::Rejected { .. })
    ));
    git.fast_forward(&b("main"), &rebased).await.unwrap();

    // Not a fast-forward: main cannot move back to the old base.
    assert!(matches!(
        git.fast_forward(&b("main"), &main).await,
        Err(RepoError::NotFastForward { .. })
    ));
    // Unknown head.
    assert!(matches!(
        git.worktree_add(&b("x"), &Sha::try_new("0".repeat(40)).unwrap())
            .await,
        Err(RepoError::RefNotFound { .. })
    ));
    // Push to a missing remote.
    assert!(git.push("nowhere", &b("main")).await.is_err());
}

#[tokio::test]
async fn rebase_conflict_is_reported_and_aborted() {
    let (_base, clone, git) = fixture();
    let main = git.head_of(&b("main")).await.unwrap();
    let wt = git
        .branch_worktree(&b("task/conflict"), &main)
        .await
        .unwrap();
    std::fs::write(wt.path.join("README.md"), "branch").unwrap();
    let head = git.commit_all(&wt, "conflict").await.unwrap();
    git.worktree_remove(wt).await.unwrap();
    sh(
        &clone,
        "echo main-moved > README.md && git add . && git commit -qm move",
    );
    let wt = git.worktree_add(&b("task/conflict"), &head).await.unwrap();
    assert!(matches!(
        git.rebase(&wt, &b("main")).await,
        Err(RepoError::Conflict { .. })
    ));
    // Aborted: the worktree is back at its head, and re-adding over a stale path works.
    assert_eq!(sh(&wt.path, "git rev-parse HEAD"), head.as_ref());
    let again = git.worktree_add(&b("task/conflict"), &head).await.unwrap();
    git.worktree_remove(again).await.unwrap();
}
