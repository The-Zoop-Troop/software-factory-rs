//! `Repo` over the `git` CLI. Worktrees live under a dedicated directory beside the clone.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use app::domain::{BranchName, Sha};
use app::{Repo, RepoError, Worktree};
use async_trait::async_trait;
use tokio::process::Command;

/// A local git clone plus a directory for worktrees.
#[derive(Debug, Clone)]
pub struct GitCli {
    repo: PathBuf,
    worktrees: PathBuf,
}

impl GitCli {
    #[must_use]
    pub fn new(repo: impl Into<PathBuf>, worktrees: impl Into<PathBuf>) -> Self {
        Self {
            repo: repo.into(),
            worktrees: worktrees.into(),
        }
    }

    /// Run `git` in the clone; returns trimmed stdout.
    ///
    /// # Errors
    /// `Unavailable` if git can't start; `Rejected` (or `RefNotFound`) on non-zero exit.
    pub async fn git(&self, args: &[&str]) -> Result<String, RepoError> {
        Self::git_in(&self.repo, args).await
    }

    /// Run `git` in an arbitrary directory (e.g. a worktree).
    ///
    /// # Errors
    /// As `git`.
    pub async fn git_in(cwd: &Path, args: &[&str]) -> Result<String, RepoError> {
        tracing::debug!(?cwd, ?args, "git");
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| RepoError::Unavailable(e.to_string()))?;
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        } else if stderr.contains("not a valid object name")
            || stderr.contains("unknown revision")
            || stderr.contains("bad revision")
            || stderr.contains("fatal: invalid reference")
        {
            Err(RepoError::RefNotFound(stderr))
        } else {
            Err(RepoError::Rejected(stderr))
        }
    }

    /// Resolve a ref to a full sha.
    ///
    /// # Errors
    /// `RefNotFound` if the ref doesn't resolve.
    pub async fn rev_parse(&self, rev: &str) -> Result<Sha, RepoError> {
        let out = self
            .git(&["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
            .await?;
        Sha::try_new(out).map_err(|e| RepoError::Rejected(format!("rev-parse output: {e}")))
    }

    fn worktree_path(&self, branch: &BranchName) -> PathBuf {
        // Branch names contain '/', which would nest directories; flatten them.
        self.worktrees.join(branch.as_ref().replace('/', "__"))
    }
}

#[async_trait]
impl Repo for GitCli {
    async fn worktree_add(&self, branch: &BranchName, head: &Sha) -> Result<Worktree, RepoError> {
        let path = self.worktree_path(branch);
        tokio::fs::create_dir_all(&self.worktrees)
            .await
            .map_err(|e| RepoError::Unavailable(e.to_string()))?;
        // A stale worktree from a crashed run must not poison this one.
        if path.exists() {
            let _ = self
                .git(&["worktree", "remove", "--force", &path.to_string_lossy()])
                .await;
            let _ = tokio::fs::remove_dir_all(&path).await;
        }
        let _ = self.git(&["worktree", "prune"]).await;
        self.git(&[
            "worktree",
            "add",
            "--detach",
            &path.to_string_lossy(),
            head.as_ref(),
        ])
        .await?;
        Ok(Worktree {
            path,
            branch: branch.clone(),
            head: head.clone(),
        })
    }

    async fn worktree_remove(&self, worktree: Worktree) -> Result<(), RepoError> {
        self.git(&[
            "worktree",
            "remove",
            "--force",
            &worktree.path.to_string_lossy(),
        ])
        .await?;
        Ok(())
    }
}
