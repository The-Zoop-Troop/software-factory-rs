//! `Repo` over the `git` CLI. Worktrees live under a dedicated directory beside the clone.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use app::domain::{BranchName, Sha};
use app::{GitOp, Repo, RepoError, Worktree};
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
    pub async fn git(&self, op: GitOp, args: &[&str]) -> Result<String, RepoError> {
        Self::git_in(op, &self.repo, args).await
    }

    /// Run `git` in an arbitrary directory (e.g. a worktree).
    ///
    /// # Errors
    /// As `git`.
    pub async fn git_in(op: GitOp, cwd: &Path, args: &[&str]) -> Result<String, RepoError> {
        tracing::debug!(?cwd, ?args, "git");
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| RepoError::Unavailable {
                op,
                cause: crate::classify_io(e.kind()),
                detail: e.to_string(),
            })?;
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
        } else {
            Err(parse_git_stderr(op, &stderr))
        }
    }
}

/// Parse git's stderr once into a typed error. The only place its wording is read.
fn parse_git_stderr(op: GitOp, stderr: &str) -> RepoError {
    // fp-allow: boundary parse of CLI stderr, done once here
    let unknown_ref = [
        "not a valid object name",
        "unknown revision",
        "bad revision",
        "fatal: invalid reference",
        "Needed a single revision",
    ];
    if unknown_ref.iter().any(|k| stderr.contains(k)) {
        return RepoError::RefNotFound {
            rev: stderr.lines().next().unwrap_or_default().to_owned(),
        };
    }
    // fp-allow: boundary parse of git stderr, done once here
    if stderr.contains("CONFLICT") || stderr.contains("could not apply") {
        // fp-allow: boundary parse of git stderr
        return RepoError::Conflict {
            paths: conflict_paths(stderr),
        };
    }
    RepoError::Rejected {
        op,
        detail: stderr.to_owned(),
    }
}

/// `CONFLICT (content): Merge conflict in <path>` lines → paths.
fn conflict_paths(stderr: &str) -> Vec<PathBuf> {
    stderr
        .lines()
        .filter_map(|l| l.split("Merge conflict in ").nth(1))
        .map(|p| PathBuf::from(p.trim()))
        .collect()
}

impl GitCli {
    /// Resolve a ref to a full sha.
    ///
    /// # Errors
    /// `RefNotFound` if the ref doesn't resolve.
    pub async fn rev_parse(&self, rev: &str) -> Result<Sha, RepoError> {
        let out = self
            .git(
                GitOp::RevParse,
                &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
            )
            .await?;
        Sha::try_new(out).map_err(|e| RepoError::Rejected {
            op: GitOp::RevParse,
            detail: format!("rev-parse output: {e}"),
        })
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
            .map_err(|e| RepoError::Unavailable {
                op: GitOp::WorktreeAdd,
                cause: crate::classify_io(e.kind()),
                detail: e.to_string(),
            })?;
        // A stale worktree from a crashed run must not poison this one.
        if path.exists() {
            // fp-allow: best-effort cleanup of a stale worktree before recreating it
            let _ = self
                .git(
                    GitOp::WorktreeRemove,
                    &["worktree", "remove", "--force", &path.to_string_lossy()],
                )
                .await;
            // fp-allow: best-effort cleanup; `worktree add` below reports the real error
            let _ = tokio::fs::remove_dir_all(&path).await;
        }
        // fp-allow: prune is advisory; a failure here cannot make the next step worse
        let _ = self
            .git(GitOp::WorktreeRemove, &["worktree", "prune"])
            .await;
        self.git(
            GitOp::RevParse,
            &[
                "worktree",
                "add",
                "--detach",
                &path.to_string_lossy(),
                head.as_ref(),
            ],
        )
        .await?;
        Ok(Worktree {
            path,
            branch: branch.clone(),
            head: head.clone(),
        })
    }

    async fn worktree_remove(&self, worktree: Worktree) -> Result<(), RepoError> {
        self.git(
            GitOp::RevParse,
            &[
                "worktree",
                "remove",
                "--force",
                &worktree.path.to_string_lossy(),
            ],
        )
        .await?;
        Ok(())
    }

    async fn rebase(&self, worktree: &Worktree, onto: &BranchName) -> Result<Sha, RepoError> {
        if let Err(e) =
            Self::git_in(GitOp::Rebase, &worktree.path, &["rebase", onto.as_ref()]).await
        {
            // fp-allow: abort is best-effort cleanup; the typed error `e` is what the caller acts on
            let _ = Self::git_in(GitOp::Rebase, &worktree.path, &["rebase", "--abort"]).await;
            return Err(e);
        }
        let out = Self::git_in(GitOp::Commit, &worktree.path, &["rev-parse", "HEAD"]).await?;
        Sha::try_new(out).map_err(|e| RepoError::Rejected {
            op: GitOp::RevParse,
            detail: format!("rev-parse output: {e}"),
        })
    }

    async fn fast_forward(&self, branch: &BranchName, to: &Sha) -> Result<(), RepoError> {
        let current = self.rev_parse(branch.as_ref()).await?;
        if self
            .git(
                GitOp::FastForward,
                &["merge-base", "--is-ancestor", current.as_ref(), to.as_ref()],
            )
            .await
            .is_err()
        {
            return Err(RepoError::NotFastForward {
                branch: branch.clone(),
                to: to.clone(),
            });
        }
        // Compare-and-swap on the ref so a concurrent mover makes this fail rather than clobber.
        self.git(
            GitOp::FastForward,
            &[
                "update-ref",
                &format!("refs/heads/{branch}"),
                to.as_ref(),
                current.as_ref(),
            ],
        )
        .await?;
        // If `branch` is checked out in the main worktree, bring its index/tree along.
        if let Ok(head) = self
            .git(
                GitOp::FastForward,
                &["symbolic-ref", "--short", "-q", "HEAD"],
            )
            .await
            && head == branch.as_ref()
        {
            self.git(GitOp::FastForward, &["reset", "--hard", "-q", to.as_ref()])
                .await?;
        }
        Ok(())
    }

    async fn push(&self, remote: &str, branch: &BranchName) -> Result<(), RepoError> {
        let refspec = format!("{branch}:{branch}");
        self.git(GitOp::Push, &["push", "--quiet", remote, &refspec])
            .await
            .map(|_| ())
    }

    async fn rollback(&self, branch: &BranchName, from: &Sha, to: &Sha) -> Result<(), RepoError> {
        // Compare-and-swap: only move the ref if it is still at `from`.
        self.git(
            GitOp::FastForward,
            &[
                "update-ref",
                &format!("refs/heads/{branch}"),
                to.as_ref(),
                from.as_ref(),
            ],
        )
        .await?;
        if let Ok(head) = self
            .git(
                GitOp::FastForward,
                &["symbolic-ref", "--short", "-q", "HEAD"],
            )
            .await
            && head == branch.as_ref()
        {
            self.git(GitOp::FastForward, &["reset", "--hard", "-q", to.as_ref()])
                .await?;
        }
        Ok(())
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
            .map_err(|e| RepoError::Unavailable {
                op: GitOp::WorktreeAdd,
                cause: crate::classify_io(e.kind()),
                detail: e.to_string(),
            })?;
        if path.exists() {
            // fp-allow: best-effort cleanup of a stale worktree before recreating it
            let _ = self
                .git(
                    GitOp::WorktreeRemove,
                    &["worktree", "remove", "--force", &path.to_string_lossy()],
                )
                .await;
            // fp-allow: best-effort cleanup; `worktree add` below reports the real error
            let _ = tokio::fs::remove_dir_all(&path).await;
        }
        // fp-allow: prune is advisory; a failure here cannot make the next step worse
        let _ = self
            .git(GitOp::WorktreeRemove, &["worktree", "prune"])
            .await;
        // `-B` resets a stale branch of the same name (a previous attempt) to the new base.
        self.git(
            GitOp::RevParse,
            &[
                "worktree",
                "add",
                "-B",
                branch.as_ref(),
                &path.to_string_lossy(),
                from.as_ref(),
            ],
        )
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
            GitOp::Commit,
            &worktree.path,
            &["add", "-A", "--", ".", ":!.factory", ":!.beads"],
        )
        .await?;
        let dirty = Self::git_in(
            GitOp::Commit,
            &worktree.path,
            &["diff", "--cached", "--quiet"],
        )
        .await
        .is_err();
        if dirty {
            Self::git_in(
                GitOp::Commit,
                &worktree.path,
                &["commit", "-q", "-m", message],
            )
            .await?;
        }
        let out = Self::git_in(GitOp::Commit, &worktree.path, &["rev-parse", "HEAD"]).await?;
        Sha::try_new(out).map_err(|e| RepoError::Rejected {
            op: GitOp::RevParse,
            detail: format!("rev-parse output: {e}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_parses_into_each_variant() {
        assert!(matches!(
            parse_git_stderr(GitOp::RevParse, "fatal: Needed a single revision"),
            RepoError::RefNotFound { .. }
        ));
        let e = parse_git_stderr(
            GitOp::Rebase,
            "Auto-merging lib.sh\nCONFLICT (content): Merge conflict in lib.sh\nCONFLICT (content): Merge conflict in README.md\nerror: could not apply abc... msg",
        );
        assert!(
            matches!(&e, RepoError::Conflict { paths } if paths == &[PathBuf::from("lib.sh"), PathBuf::from("README.md")])
        );
        assert!(matches!(
            parse_git_stderr(
                GitOp::Push,
                "fatal: 'nowhere' does not appear to be a git repository"
            ),
            RepoError::Rejected {
                op: GitOp::Push,
                ..
            }
        ));
        assert!(conflict_paths("no conflicts here").is_empty());
    }
}
