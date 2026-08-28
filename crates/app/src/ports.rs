//! Ports: the effects workflows are allowed to perform, as traits.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use domain::{BeadId, BeadKind, BranchName, Duration, FactoryMeta, Sha, Timestamp, VerifyMeta};

use crate::bead::{Bead, NewBead};
use crate::events::FactoryEvent;

/// Failures crossing the bead-store boundary, already translated from the adapter.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("bead {0} not found")]
    NotFound(BeadId),
    #[error("bead store rejected the request: {0}")]
    Rejected(String),
    #[error("could not decode bead store output: {0}")]
    Decode(String),
    #[error("bead store unavailable: {0}")]
    Unavailable(String),
}

/// The beads ledger. Implemented by the `bd` CLI adapter in `infra` and by an in-memory fake.
#[async_trait]
pub trait BeadStore: Send + Sync {
    /// # Errors
    /// `NotFound` if `id` does not exist; other variants for transport/decode failures.
    async fn show(&self, id: &BeadId) -> Result<Bead, StoreError>;

    /// Claimable beads of `kind` (dependency-aware, i.e. `bd ready`).
    ///
    /// # Errors
    /// Transport/decode failures.
    async fn ready(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError>;

    /// All non-closed beads of `kind`, regardless of readiness.
    ///
    /// # Errors
    /// Transport/decode failures.
    async fn list_active(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError>;

    /// Replace the factory metadata on a bead.
    ///
    /// # Errors
    /// `NotFound` or transport failures.
    async fn set_meta(&self, id: &BeadId, meta: &FactoryMeta) -> Result<(), StoreError>;

    /// Replace the verify metadata on a bead.
    ///
    /// # Errors
    /// `NotFound` or transport failures.
    async fn set_verify(&self, id: &BeadId, meta: &VerifyMeta) -> Result<(), StoreError>;

    /// Add a `blocks` edge: `dependent` NEEDS `blocker`.
    ///
    /// # Errors
    /// `NotFound`, `Rejected` (e.g. cycle), or transport failures.
    async fn add_needs(&self, dependent: &BeadId, blocker: &BeadId) -> Result<(), StoreError>;

    /// Append a note.
    ///
    /// # Errors
    /// `NotFound` or transport failures.
    async fn note(&self, id: &BeadId, text: &str) -> Result<(), StoreError>;

    /// # Errors
    /// `Rejected` if the store refuses the bead; transport failures.
    async fn create(&self, bead: NewBead) -> Result<BeadId, StoreError>;

    /// # Errors
    /// `NotFound` or transport failures.
    async fn close(&self, id: &BeadId, reason: &str) -> Result<(), StoreError>;

    /// Direct children of `id` (any status).
    ///
    /// # Errors
    /// Transport/decode failures.
    async fn children(&self, id: &BeadId) -> Result<Vec<Bead>, StoreError>;
}

/// Where factory events go. Recording must never fail the caller; sinks log their own trouble.
pub trait EventSink: Send + Sync {
    fn record(&self, event: &FactoryEvent);
}

/// Wall-clock source. The only place `now` comes from.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// Failures from the git adapter, already translated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RepoError {
    #[error("ref not found: {0}")]
    RefNotFound(String),
    #[error("rebase conflict: {0}")]
    Conflict(String),
    #[error("not a fast-forward: {branch} is not an ancestor of {to}")]
    NotFastForward { branch: String, to: Sha },
    #[error("git rejected the operation: {0}")]
    Rejected(String),
    #[error("git unavailable: {0}")]
    Unavailable(String),
}

/// A checked-out worktree. Removed by `Repo::worktree_remove`; never dropped silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: BranchName,
    pub head: Sha,
}

/// The project repository (a local clone with a bare "origin" the Integrator pushes to).
#[async_trait]
pub trait Repo: Send + Sync {
    /// Create a detached worktree at `head` (which must be the tip of `branch`).
    ///
    /// # Errors
    /// `RefNotFound` if `head` is unknown; `Rejected`/`Unavailable` otherwise.
    async fn worktree_add(&self, branch: &BranchName, head: &Sha) -> Result<Worktree, RepoError>;

    /// Remove a worktree created by `worktree_add`, discarding any changes.
    ///
    /// # Errors
    /// `Rejected`/`Unavailable`.
    async fn worktree_remove(&self, worktree: Worktree) -> Result<(), RepoError>;

    /// Rebase the worktree's detached HEAD onto `onto`; returns the new head.
    /// On conflict the rebase is aborted and the worktree left at its old head.
    ///
    /// # Errors
    /// `Conflict` if the rebase cannot apply cleanly.
    async fn rebase(&self, worktree: &Worktree, onto: &BranchName) -> Result<Sha, RepoError>;

    /// Move `branch` to `to`, only if that is a fast-forward.
    ///
    /// # Errors
    /// `NotFastForward` if `branch` is not an ancestor of `to`.
    async fn fast_forward(&self, branch: &BranchName, to: &Sha) -> Result<(), RepoError>;

    /// Current tip of `branch`.
    ///
    /// # Errors
    /// `RefNotFound` if the branch doesn't exist.
    async fn head_of(&self, branch: &BranchName) -> Result<Sha, RepoError>;

    /// Push `branch` to `remote`.
    ///
    /// # Errors
    /// `Rejected` if the remote refuses; `Unavailable` if unreachable.
    async fn push(&self, remote: &str, branch: &BranchName) -> Result<(), RepoError>;
}

/// Outcome of running a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// `None` when the process was killed by a signal or the timeout.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl RunOutput {
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }
}

/// Failure to even start a command (as opposed to the command failing).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not run `{command}`: {reason}")]
pub struct RunError {
    pub command: String,
    pub reason: String,
}

/// Runs shell commands in a directory with a timeout. The verify and build sandbox.
#[async_trait]
pub trait Runner: Send + Sync {
    /// # Errors
    /// `RunError` only if the process could not be spawned; a non-zero exit is an `Ok(RunOutput)`.
    async fn run(
        &self,
        cwd: &Path,
        command: &str,
        timeout: Duration,
    ) -> Result<RunOutput, RunError>;
}

/// What to ask the LLM harness to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessRequest {
    pub cwd: PathBuf,
    pub system_prompt: String,
    pub prompt: String,
    /// JSON Schema the final answer must satisfy; `None` for free text.
    pub schema: Option<serde_json::Value>,
    /// Whether the harness may edit files / run commands, or only think and answer.
    pub tools: ToolPolicy,
    pub max_turns: u32,
    pub timeout: Duration,
}

/// Tool access granted to a harness run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    /// No tools: a single structured answer (Planner v0).
    None,
    /// Read-only exploration of `cwd`.
    ReadOnly,
    /// Full YOLO inside the rig (Worker).
    Full,
}

/// What came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessOutcome {
    pub text: String,
    pub structured: Option<serde_json::Value>,
    pub tokens: u64,
    pub cost_micro_usd: u64,
    pub turns: u32,
    pub is_error: bool,
}

/// The harness could not run or returned garbage; a *model* error is `HarnessOutcome::is_error`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HarnessError {
    #[error("harness could not start: {0}")]
    Spawn(String),
    #[error("harness timed out after {0}s")]
    Timeout(u64),
    #[error("harness output undecodable: {0}")]
    Decode(String),
}

/// An LLM agent runner (Claude Code headless today; anything behind an A2A card tomorrow).
#[async_trait]
pub trait Harness: Send + Sync {
    /// # Errors
    /// `HarnessError` for infrastructure failure; model-level failure is in the outcome.
    async fn run(&self, req: HarnessRequest) -> Result<HarnessOutcome, HarnessError>;
}
