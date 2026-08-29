//! The factory's bead taxonomy (ARCHITECTURE.md §3.1), carried as a `fac:kind=<x>` label.

use core::fmt;
use core::str::FromStr;

/// What role a bead plays in the factory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum BeadKind {
    /// One per high-level plan item; closed by the Steward when all children close.
    Epic,
    /// A unit of implementation sized for one worker session.
    Task,
    /// Holds an executable acceptance check for a paired task.
    Verify,
    /// A branch that passed verification and awaits the Integrator.
    Merge,
    /// `INPUT_REQUIRED` surfaced to a human.
    Question,
    /// Budget exceeded / repeated failure / lease storm; needs a human or the Steward.
    Incident,
    /// Architecture notes and decisions injected into every worker's context packet.
    Reference,
}

impl BeadKind {
    /// Label prefix under which the kind is stored on the bead.
    pub const LABEL_PREFIX: &'static str = "fac:kind=";

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Epic => "epic",
            Self::Task => "task",
            Self::Verify => "verify",
            Self::Merge => "merge",
            Self::Question => "question",
            Self::Incident => "incident",
            Self::Reference => "reference",
        }
    }

    /// The label string to attach to a bead.
    #[must_use]
    pub fn label(self) -> String {
        format!("{}{}", Self::LABEL_PREFIX, self.as_str())
    }

    /// Recover the kind from a bead's label list, if exactly one kind label is present.
    #[must_use]
    pub fn from_labels<'a, I: IntoIterator<Item = &'a str>>(labels: I) -> Option<Self> {
        let mut found = labels
            .into_iter()
            .filter_map(|l| l.strip_prefix(Self::LABEL_PREFIX))
            .filter_map(|k| k.parse().ok());
        match (found.next(), found.next()) {
            (Some(k), None) => Some(k),
            (None, _) | (Some(_), Some(_)) => None,
        }
    }
}

/// Unknown kind string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown bead kind `{0}`")]
pub struct UnknownKind(pub String);

impl FromStr for BeadKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "epic" => Ok(Self::Epic),
            "task" => Ok(Self::Task),
            "verify" => Ok(Self::Verify),
            "merge" => Ok(Self::Merge),
            "question" => Ok(Self::Question),
            "incident" => Ok(Self::Incident),
            "reference" => Ok(Self::Reference),
            other => Err(UnknownKind(other.to_owned())),
        }
    }
}

impl fmt::Display for BeadKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_roundtrip() {
        for k in [
            BeadKind::Epic,
            BeadKind::Task,
            BeadKind::Verify,
            BeadKind::Merge,
            BeadKind::Question,
            BeadKind::Incident,
            BeadKind::Reference,
        ] {
            let label = k.label();
            assert_eq!(BeadKind::from_labels([label.as_str(), "other"]), Some(k));
        }
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(BeadKind::Verify.to_string(), "verify");
        assert_eq!(format!("{}", BeadKind::Reference), "reference");
    }

    #[test]
    fn ambiguous_or_missing_is_none() {
        assert_eq!(BeadKind::from_labels(["x"]), None);
        assert_eq!(
            BeadKind::from_labels(["fac:kind=task", "fac:kind=verify"]),
            None
        );
        assert_eq!(BeadKind::from_labels(["fac:kind=bogus"]), None);
    }
}
