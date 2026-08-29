//! Read model of a bead as the factory sees it, plus the write model for creating one.

use domain::{BeadId, BeadKind, BeadMeta, FactoryMeta, MergeMeta, VerifyMeta};

/// The beads-native status of an issue (distinct from the factory's `TaskState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeadStatus {
    Open,
    InProgress,
    Blocked,
    Deferred,
    Closed,
    Pinned,
    Hooked,
}

impl BeadStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Deferred => "deferred",
            Self::Closed => "closed",
            Self::Pinned => "pinned",
            Self::Hooked => "hooked",
        }
    }
}

/// Unknown beads status string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown bead status `{0}`")]
pub struct UnknownStatus(pub String);

impl core::str::FromStr for BeadStatus {
    type Err = UnknownStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "in_progress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "deferred" => Ok(Self::Deferred),
            "closed" => Ok(Self::Closed),
            "pinned" => Ok(Self::Pinned),
            "hooked" => Ok(Self::Hooked),
            other => Err(UnknownStatus(other.to_owned())),
        }
    }
}

/// A bead, already decoded. `kind`/`meta` are `None` for beads the factory doesn't own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bead {
    pub id: BeadId,
    pub title: String,
    pub description: String,
    pub acceptance: Option<String>,
    pub notes: Option<String>,
    pub status: BeadStatus,
    pub labels: Vec<String>,
    pub parent: Option<BeadId>,
    pub kind: Option<BeadKind>,
    /// Task fields (`metadata.fac`).
    pub meta: Option<FactoryMeta>,
    /// Verify fields (`metadata.fac_verify`).
    pub verify: Option<VerifyMeta>,
    /// Merge fields (`metadata.fac_merge`).
    pub merge: Option<MergeMeta>,
}

/// What the factory needs to create a bead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBead {
    pub title: String,
    pub description: String,
    pub kind: BeadKind,
    pub priority: u8,
    pub parent: Option<BeadId>,
    /// `blocks` edges: this bead NEEDS each of these.
    pub needs: Vec<BeadId>,
    pub acceptance: Option<String>,
    pub meta: Option<BeadMeta>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip_and_unknown() {
        for s in [
            BeadStatus::Open,
            BeadStatus::InProgress,
            BeadStatus::Blocked,
            BeadStatus::Deferred,
            BeadStatus::Closed,
            BeadStatus::Pinned,
            BeadStatus::Hooked,
        ] {
            assert_eq!(s.as_str().parse::<BeadStatus>(), Ok(s));
        }
        assert!("bogus".parse::<BeadStatus>().is_err());
    }
}
