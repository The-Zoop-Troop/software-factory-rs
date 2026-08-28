//! Identifier newtypes. Each is constructed fallibly from raw text exactly once.
// nutype's `regex` validator expands to a lazily-initialised `Regex::new(..).expect(..)`;
// the patterns are literals checked by the tests below, so the expect is a proof, not a hope.
#![allow(
    clippy::disallowed_methods,
    reason = "nutype-generated regex init on literal patterns"
)]

use nutype::nutype;

/// A beads issue id, e.g. `fac-ec6.2`.
#[nutype(
    sanitize(trim),
    validate(not_empty, regex = r"^[A-Za-z0-9_-]+(\.[0-9]+)*$"),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Display,
        AsRef,
        TryFrom,
        Serialize,
        Deserialize
    )
)]
pub struct BeadId(String);

/// Identity of a factory agent process (e.g. `worker-07`).
#[nutype(
    sanitize(trim),
    validate(not_empty, regex = r"^[A-Za-z0-9_.-]+$"),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Display,
        AsRef,
        TryFrom,
        Serialize,
        Deserialize
    )
)]
pub struct AgentId(String);

/// A git branch name (refname component rules, simplified).
#[nutype(
    sanitize(trim),
    validate(not_empty, regex = r"^[A-Za-z0-9][A-Za-z0-9/_.-]*$", predicate = |s| !s.contains("..") && !s.ends_with('/')),
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Display, AsRef, TryFrom, Serialize, Deserialize)
)]
pub struct BranchName(String);

/// A full 40-hex git object id.
#[nutype(
    sanitize(trim, lowercase),
    validate(regex = r"^[0-9a-f]{40}$"),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Display,
        AsRef,
        TryFrom,
        Serialize,
        Deserialize
    )
)]
pub struct Sha(String);

impl BranchName {
    /// The conventional worktree branch for a task bead: `task/<id>`.
    ///
    /// # Errors
    /// Cannot fail for a well-formed `BeadId`; the `Result` keeps the function total
    /// rather than asserting the cross-type invariant.
    pub fn for_task(id: &BeadId) -> Result<Self, BranchNameError> {
        Self::try_new(format!("task/{id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bead_ids_parse() {
        assert!(BeadId::try_new("fac-ec6").is_ok());
        assert!(BeadId::try_new("fac-ec6.2").is_ok());
        assert!(BeadId::try_new("").is_err());
        assert!(BeadId::try_new("fac ec6").is_err());
    }

    #[test]
    fn branch_for_task() {
        let id = BeadId::try_new("fac-ec6.2").unwrap();
        assert_eq!(
            BranchName::for_task(&id).unwrap().as_ref(),
            "task/fac-ec6.2"
        );
    }

    #[test]
    fn sha_normalizes() {
        let s = Sha::try_new(" 0123456789ABCDEF0123456789abcdef01234567 ").unwrap();
        assert_eq!(s.as_ref(), "0123456789abcdef0123456789abcdef01234567");
        assert!(Sha::try_new("abc").is_err());
    }
}
