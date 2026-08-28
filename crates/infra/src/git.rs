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
    /// Both paths are made absolute up front: git resolves relative paths against its own
    /// `-C`/cwd, while the rest of the factory resolves them against the process cwd.
    #[must_use]
    pub fn new(repo: impl Into<PathBuf>, worktrees: impl Into<PathBuf>) -> Self {
        Self {
            repo: absolute(repo.into()),
            worktrees: absolute(worktrees.into()),
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

fn absolute(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&p)).unwrap_or(p)
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

    async fn rebase(&self, worktree: &Worktree, onto: &BranchName) -> Result<Sha, RepoError> {
        match Self::git_in(&worktree.path, &["rebase", onto.as_ref()]).await {
            Ok(_) => {}
            Err(RepoError::Rejected(msg))
                if msg.contains("CONFLICT") || msg.contains("could not apply") =>
            {
                let _ = Self::git_in(&worktree.path, &["rebase", "--abort"]).await;
                return Err(RepoError::Conflict(msg));
            }
            Err(e) => {
                let _ = Self::git_in(&worktree.path, &["rebase", "--abort"]).await;
                return Err(e);
            }
        }
        let out = Self::git_in(&worktree.path, &["rev-parse", "HEAD"]).await?;
        Sha::try_new(out).map_err(|e| RepoError::Rejected(format!("rev-parse output: {e}")))
    }

    async fn fast_forward(&self, branch: &BranchName, to: &Sha) -> Result<(), RepoError> {
        let current = self.rev_parse(branch.as_ref()).await?;
        if self
            .git(&["merge-base", "--is-ancestor", current.as_ref(), to.as_ref()])
            .await
            .is_err()
        {
            return Err(RepoError::NotFastForward {
                branch: branch.to_string(),
                to: to.clone(),
            });
        }
        // Compare-and-swap on the ref so a concurrent mover makes this fail rather than clobber.
        self.git(&[
            "update-ref",
            &format!("refs/heads/{branch}"),
            to.as_ref(),
            current.as_ref(),
        ])
        .await?;
        // If `branch` is checked out in the main worktree, bring its index/tree along.
        if let Ok(head) = self.git(&["symbolic-ref", "--short", "-q", "HEAD"]).await
            && head == branch.as_ref()
        {
            self.git(&["reset", "--hard", "-q", to.as_ref()]).await?;
        }
        Ok(())
    }

    async fn push(&self, remote: &str, branch: &BranchName) -> Result<(), RepoError> {
        let refspec = format!("{branch}:{branch}");
        self.git(&["push", "--quiet", remote, &refspec])
            .await
            .map(|_| ())
    }

    async fn head_of(&self, branch: &BranchName) -> Result<Sha, RepoError> {
        self.rev_parse(&format!("refs/heads/{branch}")).await
    }

    async fn branch_worktree(
        &self,
        branch: &BranchName,
        from: &Sha,
    ) -> Result<Worktree, RepoError> {
        let path = self.worktree_path(branch);
        tokio::fs::create_dir_all(&self.worktrees)
            .await
            .map_err(|e| RepoError::Unavailable(e.to_string()))?;
        if path.exists() {
            let _ = self
                .git(&["worktree", "remove", "--force", &path.to_string_lossy()])
                .await;
            let _ = tokio::fs::remove_dir_all(&path).await;
        }
        let _ = self.git(&["worktree", "prune"]).await;
        // `-B` resets a stale branch of the same name (a previous attempt) to the new base.
        self.git(&[
            "worktree",
            "add",
            "-B",
            branch.as_ref(),
            &path.to_string_lossy(),
            from.as_ref(),
        ])
        .await?;
        Ok(Worktree {
            path,
            branch: branch.clone(),
            head: from.clone(),
        })
    }

    async fn commit_all(&self, worktree: &Worktree, message: &str) -> Result<Sha, RepoError> {
        // Never let the harness's scratch files or our own markers into the commit.
        Self::git_in(
            &worktree.path,
            &["add", "-A", "--", ".", ":!.factory", ":!.beads"],
        )
        .await?;
        let dirty = Self::git_in(&worktree.path, &["diff", "--cached", "--quiet"])
            .await
            .is_err();
        if dirty {
            Self::git_in(&worktree.path, &["commit", "-q", "-m", message]).await?;
        }
        let out = Self::git_in(&worktree.path, &["rev-parse", "HEAD"]).await?;
        Sha::try_new(out).map_err(|e| RepoError::Rejected(format!("rev-parse output: {e}")))
    }
}
