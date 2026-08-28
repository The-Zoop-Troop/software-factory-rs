//! Boundary: the JSON stored in a bead's `metadata.fac` field ⇄ `Task`.
//!
//! Serde decodes into `RawFactoryMeta`; `TryFrom` turns it into a `FactoryMeta`
//! whose fields are already-valid domain types. Nothing else in the factory reads
//! bead metadata directly.

use crate::budget::{Budget, Usage};
use crate::ids::{BeadId, Sha};
use crate::task::{Task, TaskState};

/// Key under a bead's metadata object where the factory keeps its fields.
pub const META_KEY: &str = "fac";

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
    fn missing_budget_defaults() {
        let s = r#"{"version":1,"verify_bead":"fac-2","base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","state":{"state":"open"}}"#;
        let m: FactoryMeta = serde_json::from_str(s).unwrap();
        assert_eq!(m.budget, Budget::default());
        assert_eq!(m.state, TaskState::Open);
    }
}
