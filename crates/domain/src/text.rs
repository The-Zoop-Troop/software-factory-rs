//! Text newtypes for values that cross domain boundaries: a title, a verify command.

use nutype::nutype;

/// A bead or plan title: non-blank, single line, at most 200 characters.
#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 200, predicate = |s| !s.contains('\n')),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, TryFrom, Serialize, Deserialize)
)]
pub struct Title(String);

/// One shell command line run by the Verifier from the repo root under POSIX `sh`.
#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 4000, predicate = |s| !s.contains('\0')),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, TryFrom, Serialize, Deserialize)
)]
pub struct VerifyCommand(String);

impl Title {
    /// A title derived from trusted parts (ids, other titles): trimmed, first line, cut to the
    /// length limit, never blank. Total, so callers building titles from ids need no error path.
    #[must_use]
    pub fn derived(text: &str) -> Self {
        let line = text.lines().next().unwrap_or_default().trim();
        let cut: String = line.chars().take(200).collect();
        let cut = if cut.trim().is_empty() {
            "untitled".to_owned()
        } else {
            cut
        };
        // Every input to try_new here satisfies the validator by construction.
        Self::try_new(cut).unwrap_or_else(|_| Self::derived("untitled")) // fp-allow: total by construction; recursion bottoms out on a literal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_rules() {
        assert_eq!(
            Title::try_new("  Add login  ").unwrap().as_ref(),
            "Add login"
        );
        assert!(Title::try_new("   ").is_err());
        assert!(Title::try_new("two\nlines").is_err());
        assert!(Title::try_new("x".repeat(201)).is_err());
    }

    #[test]
    fn verify_command_rules() {
        assert_eq!(
            VerifyCommand::try_new(" cargo test ").unwrap().as_ref(),
            "cargo test"
        );
        assert!(VerifyCommand::try_new("").is_err());
        assert!(VerifyCommand::try_new("bad\0nul").is_err());
    }
}

#[cfg(test)]
mod derived_tests {
    use super::*;

    #[test]
    fn derived_is_total() {
        assert_eq!(Title::derived("  a\nb ").as_ref(), "a");
        assert_eq!(Title::derived("   ").as_ref(), "untitled");
        assert_eq!(
            Title::derived(&"x".repeat(500)).as_ref().chars().count(),
            200
        );
    }
}
