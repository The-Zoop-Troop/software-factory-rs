//! Error tracks for the ports. Every variant carries what a caller can branch on; free text
//! appears only as `detail` evidence next to a typed classification, never as the classification.

use std::path::PathBuf;

use domain::{BeadId, BranchName, Duration, Sha};

/// Why a backend could not be reached or could not answer at all (as opposed to refusing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Unavailable {
    #[error("binary not installed or not executable")]
    NotInstalled,
    #[error("backend is locked or busy")]
    Locked,
    #[error("backend database error")]
    Database,
    #[error("i/o failure")]
    Io,
    #[error("network refused or dropped")]
    Network,
    #[error("did not become healthy in time")]
    NotHealthy,
    #[error("failed for an unclassified reason")]
    Unknown,
}

/// Ledger operations, for error context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum StoreOp {
    Show,
    Ready,
    List,
    Create,
    Update,
    Note,
    Close,
    Dep,
    Children,
}

/// Failures crossing the bead-store boundary, already translated from the adapter.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("bead {id} not found")]
    NotFound { id: BeadId },
    /// The ledger refused to close `id` while `by` are open. Close the blockers first.
    #[error("bead {id} is blocked by open beads {by:?}")]
    Blocked { id: BeadId, by: Vec<BeadId> },
    #[error("ledger rejected {op:?}: {detail}")]
    Rejected { op: StoreOp, detail: String },
    #[error("could not decode ledger output for {op:?} ({field}): {detail}")]
    Decode {
        op: StoreOp,
        field: &'static str,
        detail: String,
    },
    #[error("ledger unavailable during {op:?}: {cause}: {detail}")]
    Unavailable {
        op: StoreOp,
        cause: Unavailable,
        detail: String,
    },
}

impl StoreError {
    /// True when retrying later could succeed without any change on our side.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        match self {
            Self::Unavailable { cause, .. } => matches!(
                cause,
                Unavailable::Locked
                    | Unavailable::Database
                    | Unavailable::Io
                    | Unavailable::Network
                    | Unavailable::NotHealthy
            ),
            Self::NotFound { .. }
            | Self::Blocked { .. }
            | Self::Rejected { .. }
            | Self::Decode { .. } => false,
        }
    }
}

/// Git operations, for error context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GitOp {
    RevParse,
    WorktreeAdd,
    WorktreeRemove,
    Commit,
    Rebase,
    FastForward,
    Push,
    Status,
}

/// Failures from the git adapter, already translated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RepoError {
    #[error("ref not found: {rev}")]
    RefNotFound { rev: String },
    /// The rebase was aborted; the worktree is back at its old head.
    #[error("rebase conflict in {paths:?}")]
    Conflict { paths: Vec<PathBuf> },
    #[error("not a fast-forward: {branch} is not an ancestor of {to}")]
    NotFastForward { branch: BranchName, to: Sha },
    #[error("git rejected {op:?}: {detail}")]
    Rejected { op: GitOp, detail: String },
    #[error("git unavailable during {op:?}: {cause}: {detail}")]
    Unavailable {
        op: GitOp,
        cause: Unavailable,
        detail: String,
    },
}

impl RepoError {
    /// True when the branch is not at fault and a later attempt may land it unchanged.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        match self {
            Self::Unavailable { .. } | Self::NotFastForward { .. } => true,
            Self::RefNotFound { .. } | Self::Conflict { .. } | Self::Rejected { .. } => false,
        }
    }
}

/// Where in a harness run something failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum HarnessStage {
    Spawn,
    Health,
    Session,
    Prompt,
    Envelope,
}

/// The harness could not run or returned garbage; a *model* error is `HarnessOutcome::is_error`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HarnessError {
    #[error("harness `{bin}` could not start: {cause}: {detail}")]
    Spawn {
        bin: PathBuf,
        cause: Unavailable,
        detail: String,
    },
    #[error("harness timed out after {after:?} during {stage:?}")]
    Timeout {
        after: Duration,
        stage: HarnessStage,
    },
    #[error("harness HTTP {status} during {stage:?}: {detail}")]
    Http {
        stage: HarnessStage,
        status: u16,
        detail: String,
    },
    #[error("harness output undecodable during {stage:?}: {detail}")]
    Decode { stage: HarnessStage, detail: String },
    /// The worker's lease was lost mid-session; the session was abandoned.
    #[error("lease lost during session: {detail}")]
    LeaseLost { detail: String },
}

/// Failure to even start a command (as opposed to the command failing).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not run `{command}`: {cause}: {detail}")]
pub struct RunError {
    pub command: String,
    pub cause: Unavailable,
    pub detail: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn transient_classification() {
        let id = BeadId::try_new("fac-1").unwrap();
        assert!(
            StoreError::Unavailable {
                op: StoreOp::Show,
                cause: Unavailable::Locked,
                detail: String::new()
            }
            .is_transient()
        );
        assert!(
            !StoreError::Blocked {
                id: id.clone(),
                by: vec![]
            }
            .is_transient()
        );
        assert!(!StoreError::NotFound { id }.is_transient());
        let branch = BranchName::try_new("main").unwrap();
        let to = Sha::try_new("a".repeat(40)).unwrap();
        assert!(RepoError::NotFastForward { branch, to }.is_transient());
        assert!(!RepoError::Conflict { paths: vec![] }.is_transient());
    }
}
