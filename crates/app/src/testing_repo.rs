//! A fake `Repo`: records worktree adds/removes, commits, pushes; never touches disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use domain::{BranchName, Duration, Sha};

use crate::ports::{Repo, RepoError, RunError, RunOutput, Runner, Worktree};

/// Records worktree adds/removes; never touches disk.
#[derive(Debug, Default)]
pub struct FakeRepo {
    pub added: std::sync::Mutex<Vec<Worktree>>,
    pub removed: std::sync::Mutex<Vec<Worktree>>,
    /// Heads that `worktree_add` should reject as unknown.
    pub missing: Vec<Sha>,
    /// Heads whose rebase should conflict.
    pub conflicting: Vec<Sha>,
    /// What `rebase` returns as the new head for a given old head (identity if absent).
    pub rebased_to: BTreeMap<Sha, Sha>,
    pub fast_forwards: std::sync::Mutex<Vec<(BranchName, Sha)>>,
    pub pushes: std::sync::Mutex<Vec<(String, BranchName)>>,
    /// Make every push fail with `Unavailable`.
    pub push_fails: bool,
    /// What `commit_all` reports as HEAD (the fake never has real changes).
    pub commit_head: Option<Sha>,
    /// What `diff_stat` reports while a session runs.
    pub drift: crate::ports::DiffStat,
    /// What `diff_summary` reports for a contract.
    pub summary: crate::ports::DiffSummary,
    pub commits: std::sync::Mutex<Vec<String>>,
    pub rollbacks: std::sync::Mutex<Vec<(BranchName, Sha, Sha)>>,
}

#[async_trait]
impl Repo for FakeRepo {
    async fn diff_stat(&self, _worktree: &Worktree) -> Result<crate::ports::DiffStat, RepoError> {
        Ok(self.drift)
    }
    async fn diff_summary(
        &self,
        _base: &Sha,
        _head: &Sha,
    ) -> Result<crate::ports::DiffSummary, RepoError> {
        Ok(self.summary.clone())
    }

    async fn worktree_add(&self, branch: &BranchName, head: &Sha) -> Result<Worktree, RepoError> {
        if self.missing.contains(head) {
            return Err(RepoError::RefNotFound {
                rev: head.to_string(),
            });
        }
        let wt = Worktree {
            path: PathBuf::from(format!("/fake/wt/{branch}")),
            branch: branch.clone(),
            head: head.clone(),
        };
        self.added.lock().expect("test mutex").push(wt.clone());
        Ok(wt)
    }

    async fn worktree_remove(&self, worktree: Worktree) -> Result<(), RepoError> {
        self.removed.lock().expect("test mutex").push(worktree);
        Ok(())
    }

    async fn branch_worktree(
        &self,
        branch: &BranchName,
        from: &Sha,
    ) -> Result<Worktree, RepoError> {
        self.worktree_add(branch, from).await
    }

    async fn commit_all(&self, worktree: &Worktree, message: &str) -> Result<Sha, RepoError> {
        self.commits
            .lock()
            .expect("test mutex")
            .push(message.to_owned());
        Ok(self
            .commit_head
            .clone()
            .unwrap_or_else(|| worktree.head.clone()))
    }

    async fn rebase(&self, worktree: &Worktree, _onto: &BranchName) -> Result<Sha, RepoError> {
        if self.conflicting.contains(&worktree.head) {
            return Err(RepoError::Conflict {
                paths: vec![PathBuf::from("lib.sh")],
            });
        }
        Ok(self
            .rebased_to
            .get(&worktree.head)
            .cloned()
            .unwrap_or_else(|| worktree.head.clone()))
    }

    async fn fast_forward(&self, branch: &BranchName, to: &Sha) -> Result<(), RepoError> {
        self.fast_forwards
            .lock()
            .expect("test mutex")
            .push((branch.clone(), to.clone()));
        Ok(())
    }

    async fn head_of(&self, _branch: &BranchName) -> Result<Sha, RepoError> {
        Sha::try_new("0".repeat(40)).map_err(|e| RepoError::Rejected {
            op: crate::ports::GitOp::RevParse,
            detail: e.to_string(),
        })
    }

    async fn rollback(&self, branch: &BranchName, from: &Sha, to: &Sha) -> Result<(), RepoError> {
        self.rollbacks
            .lock()
            .expect("test mutex")
            .push((branch.clone(), from.clone(), to.clone()));
        Ok(())
    }

    async fn push(&self, remote: &str, branch: &BranchName) -> Result<(), RepoError> {
        if self.push_fails {
            return Err(RepoError::Unavailable {
                op: crate::ports::GitOp::Push,
                cause: crate::ports::Unavailable::Network,
                detail: "remote down".into(),
            });
        }
        self.pushes
            .lock()
            .expect("test mutex")
            .push((remote.to_owned(), branch.clone()));
        Ok(())
    }
}

/// Scripted command outcomes: exact command string → output. Unknown commands fail to spawn.
#[derive(Debug, Default)]
pub struct FakeRunner {
    pub script: BTreeMap<String, RunOutput>,
    pub calls: std::sync::Mutex<Vec<(PathBuf, String)>>,
}

impl FakeRunner {
    #[must_use]
    pub fn ok(stdout: &str) -> RunOutput {
        RunOutput {
            exit_code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
            timed_out: false,
        }
    }

    #[must_use]
    pub fn fail(code: i32, stderr: &str) -> RunOutput {
        RunOutput {
            exit_code: Some(code),
            stdout: String::new(),
            stderr: stderr.into(),
            timed_out: false,
        }
    }
}

#[async_trait]
impl Runner for FakeRunner {
    async fn run(
        &self,
        cwd: &Path,
        command: &str,
        _timeout: Duration,
    ) -> Result<RunOutput, RunError> {
        self.calls
            .lock()
            .expect("test mutex")
            .push((cwd.to_path_buf(), command.to_owned()));
        self.script.get(command).cloned().ok_or_else(|| RunError {
            command: command.to_owned(),
            cause: crate::ports::Unavailable::NotInstalled,
            detail: "unscripted".into(),
        })
    }
}
