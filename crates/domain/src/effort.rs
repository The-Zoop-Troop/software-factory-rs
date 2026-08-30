//! How hard a harness should think. Each adapter maps it: `claude --effort`, the `variant`
//! field of an `OpenCode` message, `model_reasoning_effort` for `Codex`. `None` on a request
//! means the harness default.

use core::fmt;
use core::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum Effort {
    Low,
    Medium,
    High,
    Max,
}

impl Effort {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }
}

impl fmt::Display for Effort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unknown effort level.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown effort `{0}` (low|medium|high|max)")]
pub struct UnknownEffort(pub String);

impl FromStr for Effort {
    type Err = UnknownEffort;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" | "med" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "max" | "xhigh" => Ok(Self::Max),
            other => Err(UnknownEffort(other.to_owned())), // fp-allow: free text from env/CLI
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_rejects() {
        for e in [Effort::Low, Effort::Medium, Effort::High, Effort::Max] {
            assert_eq!(e.as_str().parse::<Effort>(), Ok(e));
            assert_eq!(e.to_string(), e.as_str());
        }
        assert_eq!(" HIGH ".parse::<Effort>(), Ok(Effort::High));
        assert_eq!("xhigh".parse::<Effort>(), Ok(Effort::Max));
        assert!("ultra".parse::<Effort>().is_err());
        assert!(Effort::Low < Effort::Max);
    }
}
