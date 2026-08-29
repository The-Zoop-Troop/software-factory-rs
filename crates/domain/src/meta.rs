//! Boundary: the JSON stored in a bead's `metadata.fac` field ⇄ `Task`.
//!
//! Serde decodes into `RawFactoryMeta`; `TryFrom` turns it into a `FactoryMeta`
//! whose fields are already-valid domain types. Nothing else in the factory reads
//! bead metadata directly.

use crate::budget::{Budget, Usage};
use crate::ids::{BeadId, Sha};
use crate::task::{Task, TaskState};

/// Key under a bead's metadata object where the factory keeps a task's fields.
pub const META_KEY: &str = "fac";
/// Key for a verify bead's fields.
pub const VERIFY_META_KEY: &str = "fac_verify";
/// Key for a merge bead's fields.
pub const MERGE_META_KEY: &str = "fac_merge";

/// Schema version of the metadata blob; bump on incompatible change.
pub const META_VERSION: u32 = 1;

/// Typed view of everything the factory stores on a task bead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(try_from = "RawFactoryMeta", into = "RawFactoryMeta")
)]
pub struct FactoryMeta {
    pub verify_bead: BeadId,
    pub base: Sha,
    pub budget: Budget,
    pub usage: Usage,
    pub lease_expiries: u32,
    pub state: TaskState,
}

/// Wire shape. Public only so serde can name it; construct `FactoryMeta` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawFactoryMeta {
    pub version: u32,
    pub verify_bead: String,
    pub base: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub budget: Option<Budget>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub usage: Usage,
    #[cfg_attr(feature = "serde", serde(default))]
    pub lease_expiries: u32,
    pub state: TaskState,
}

/// Why a metadata blob could not become a `FactoryMeta`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetaParseError {
    #[error("unsupported factory metadata version {found} (expected {expected})")]
    Version { found: u32, expected: u32 },
    #[error("invalid verify_bead id: {0}")]
    VerifyBead(String),
    #[error("invalid base sha: {0}")]
    Base(String),
}

impl TryFrom<RawFactoryMeta> for FactoryMeta {
    type Error = MetaParseError;

    fn try_from(raw: RawFactoryMeta) -> Result<Self, Self::Error> {
        if raw.version != META_VERSION {
            return Err(MetaParseError::Version {
                found: raw.version,
                expected: META_VERSION,
            });
        }
        Ok(Self {
            verify_bead: BeadId::try_new(raw.verify_bead)
                .map_err(|e| MetaParseError::VerifyBead(e.to_string()))?,
            base: Sha::try_new(raw.base).map_err(|e| MetaParseError::Base(e.to_string()))?,
            budget: raw.budget.unwrap_or_default(),
            usage: raw.usage,
            lease_expiries: raw.lease_expiries,
            state: raw.state,
        })
    }
}

impl From<FactoryMeta> for RawFactoryMeta {
    fn from(m: FactoryMeta) -> Self {
        Self {
            version: META_VERSION,
            verify_bead: m.verify_bead.into_inner(),
            base: m.base.into_inner(),
            budget: Some(m.budget),
            usage: m.usage,
            lease_expiries: m.lease_expiries,
            state: m.state,
        }
    }
}

impl FactoryMeta {
    /// Attach the bead id to make a `Task`.
    #[must_use]
    pub fn into_task(self, id: BeadId) -> Task {
        Task {
            id,
            verify_bead: self.verify_bead,
            base: self.base,
            budget: self.budget,
            usage: self.usage,
            lease_expiries: self.lease_expiries,
            state: self.state,
        }
    }
}

/// What a verify bead stores: which task it checks and how.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(try_from = "RawVerifyMeta", into = "RawVerifyMeta")
)]
pub struct VerifyMeta {
    pub task: BeadId,
    /// Shell commands run in order inside a fresh worktree; the first non-zero exit fails.
    pub commands: Vec<String>,
    pub timeout: crate::time::Duration,
}

/// Wire shape of `VerifyMeta`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawVerifyMeta {
    pub version: u32,
    pub task: String,
    pub commands: Vec<String>,
    #[cfg_attr(feature = "serde", serde(default = "default_timeout"))]
    pub timeout: crate::time::Duration,
}

fn default_timeout() -> crate::time::Duration {
    crate::time::Duration::from_minutes(20)
}

/// Why a verify blob could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerifyMetaParseError {
    #[error("unsupported verify metadata version {found} (expected {expected})")]
    Version { found: u32, expected: u32 },
    #[error("invalid task id: {0}")]
    Task(String),
    #[error("verify bead has no commands")]
    NoCommands,
}

impl TryFrom<RawVerifyMeta> for VerifyMeta {
    type Error = VerifyMetaParseError;

    fn try_from(raw: RawVerifyMeta) -> Result<Self, Self::Error> {
        if raw.version != META_VERSION {
            return Err(VerifyMetaParseError::Version {
                found: raw.version,
                expected: META_VERSION,
            });
        }
        if raw.commands.iter().all(|c| c.trim().is_empty()) {
            return Err(VerifyMetaParseError::NoCommands);
        }
        Ok(Self {
            task: BeadId::try_new(raw.task)
                .map_err(|e| VerifyMetaParseError::Task(e.to_string()))?,
            commands: raw
                .commands
                .into_iter()
                .filter(|c| !c.trim().is_empty())
                .collect(),
            timeout: raw.timeout,
        })
    }
}

impl From<VerifyMeta> for RawVerifyMeta {
    fn from(m: VerifyMeta) -> Self {
        Self {
            version: META_VERSION,
            task: m.task.into_inner(),
            commands: m.commands,
            timeout: m.timeout,
        }
    }
}

/// What a merge bead stores: the branch the Integrator should land and for which task.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(try_from = "RawMergeMeta", into = "RawMergeMeta")
)]
pub struct MergeMeta {
    pub task: BeadId,
    pub branch: crate::ids::BranchName,
    pub head: Sha,
}

/// Wire shape of `MergeMeta`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawMergeMeta {
    pub version: u32,
    pub task: String,
    pub branch: String,
    pub head: String,
}

/// Why a merge blob could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MergeMetaParseError {
    #[error("unsupported merge metadata version {found} (expected {expected})")]
    Version { found: u32, expected: u32 },
    #[error("invalid task id: {0}")]
    Task(String),
    #[error("invalid branch: {0}")]
    Branch(String),
    #[error("invalid head sha: {0}")]
    Head(String),
}

impl TryFrom<RawMergeMeta> for MergeMeta {
    type Error = MergeMetaParseError;

    fn try_from(raw: RawMergeMeta) -> Result<Self, Self::Error> {
        if raw.version != META_VERSION {
            return Err(MergeMetaParseError::Version {
                found: raw.version,
                expected: META_VERSION,
            });
        }
        Ok(Self {
            task: BeadId::try_new(raw.task)
                .map_err(|e| MergeMetaParseError::Task(e.to_string()))?,
            branch: crate::ids::BranchName::try_new(raw.branch)
                .map_err(|e| MergeMetaParseError::Branch(e.to_string()))?,
            head: Sha::try_new(raw.head).map_err(|e| MergeMetaParseError::Head(e.to_string()))?,
        })
    }
}

impl From<MergeMeta> for RawMergeMeta {
    fn from(m: MergeMeta) -> Self {
        Self {
            version: META_VERSION,
            task: m.task.into_inner(),
            branch: m.branch.into_inner(),
            head: m.head.into_inner(),
        }
    }
}

/// Any factory metadata a bead can carry, keyed by bead kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeadMeta {
    Task(FactoryMeta),
    Verify(VerifyMeta),
    Merge(MergeMeta),
}

impl BeadMeta {
    /// The metadata key this variant is stored under.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            Self::Task(_) => META_KEY,
            Self::Verify(_) => VERIFY_META_KEY,
            Self::Merge(_) => MERGE_META_KEY,
        }
    }
}

impl From<Task> for FactoryMeta {
    fn from(t: Task) -> Self {
        Self {
            verify_bead: t.verify_bead,
            base: t.base,
            budget: t.budget,
            usage: t.usage,
            lease_expiries: t.lease_expiries,
            state: t.state,
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;
    use crate::ids::AgentId;
    use crate::lease::Lease;
    use crate::time::{Duration, Timestamp};

    #[test]
    fn json_roundtrip_leased() {
        let meta = FactoryMeta {
            verify_bead: BeadId::try_new("fac-2").unwrap(),
            base: Sha::try_new("a".repeat(40)).unwrap(),
            budget: Budget::default(),
            usage: Usage::default().add_tokens(5),
            lease_expiries: 1,
            state: TaskState::Leased {
                lease: Lease::grant(
                    AgentId::try_new("w1").unwrap(),
                    Timestamp::from_unix_seconds(10),
                    Duration::from_seconds(60),
                ),
            },
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"state\":\"leased\""));
        let back: FactoryMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn rejects_bad_version_and_sha() {
        let bad = r#"{"version":99,"verify_bead":"fac-2","base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","state":{"state":"open"}}"#;
        let err = serde_json::from_str::<FactoryMeta>(bad).unwrap_err();
        assert!(err.to_string().contains("version"));
        let bad = r#"{"version":1,"verify_bead":"fac-2","base":"nope","state":{"state":"open"}}"#;
        assert!(serde_json::from_str::<FactoryMeta>(bad).is_err());
    }

    #[test]
    fn verify_meta_rejects_empty_commands() {
        let bad = r#"{"version":1,"task":"fac-1","commands":["  "]}"#;
        assert!(serde_json::from_str::<VerifyMeta>(bad).is_err());
        let ok = r#"{"version":1,"task":"fac-1","commands":["  ","cargo test",""]}"#;
        let m: VerifyMeta = serde_json::from_str(ok).unwrap();
        assert_eq!(m.timeout, crate::time::Duration::from_minutes(20));
        assert_eq!(m.commands, vec!["cargo test"], "blank commands are dropped");
    }

    #[test]
    fn merge_meta_roundtrip() {
        let m = MergeMeta {
            task: BeadId::try_new("fac-1").unwrap(),
            branch: crate::ids::BranchName::try_new("task/fac-1").unwrap(),
            head: Sha::try_new("b".repeat(40)).unwrap(),
        };
        let back: MergeMeta = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn verify_and_merge_meta_reject_bad_versions_and_ids() {
        assert!(
            serde_json::from_str::<VerifyMeta>(r#"{"version":9,"task":"fac-1","commands":["x"]}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<VerifyMeta>(r#"{"version":1,"task":"","commands":["x"]}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<MergeMeta>(
                r#"{"version":9,"task":"fac-1","branch":"b","head":"x"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<MergeMeta>(
                r#"{"version":1,"task":"fac-1","branch":"","head":"x"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<MergeMeta>(
                r#"{"version":1,"task":"fac-1","branch":"b","head":"x"}"#
            )
            .is_err()
        );
        let bad_task = r#"{"version":1,"verify_bead":"","base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","state":{"state":"open"}}"#;
        assert!(serde_json::from_str::<FactoryMeta>(bad_task).is_err());
    }

    #[test]
    fn bead_meta_keys_and_task_roundtrip() {
        let m = FactoryMeta {
            verify_bead: BeadId::try_new("fac-2").unwrap(),
            base: Sha::try_new("a".repeat(40)).unwrap(),
            budget: Budget::default(),
            usage: Usage::default(),
            lease_expiries: 0,
            state: TaskState::Open,
        };
        assert_eq!(BeadMeta::Task(m.clone()).key(), META_KEY);
        let v = VerifyMeta {
            task: BeadId::try_new("fac-1").unwrap(),
            commands: vec!["true".into()],
            timeout: crate::time::Duration::from_seconds(1),
        };
        assert_eq!(BeadMeta::Verify(v).key(), VERIFY_META_KEY);
        let mm = MergeMeta {
            task: BeadId::try_new("fac-1").unwrap(),
            branch: crate::ids::BranchName::try_new("b").unwrap(),
            head: Sha::try_new("b".repeat(40)).unwrap(),
        };
        assert_eq!(BeadMeta::Merge(mm).key(), MERGE_META_KEY);
        let task = m.clone().into_task(BeadId::try_new("fac-9").unwrap());
        assert_eq!(FactoryMeta::from(task), m);
    }

    #[test]
    fn missing_budget_defaults() {
        let s = r#"{"version":1,"verify_bead":"fac-2","base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","state":{"state":"open"}}"#;
        let m: FactoryMeta = serde_json::from_str(s).unwrap();
        assert_eq!(m.budget, Budget::default());
        assert_eq!(m.state, TaskState::Open);
    }
}
