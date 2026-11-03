//! Which tests a submission weakened, as far as file paths show it. A Worker that cannot make a
//! check pass can edit or delete the test instead, and nothing else in the factory would notice.
//! Detection only: the Worker records it as an event, and later gates and the audit judge it.
//!
//! Paths are all this sees. A test inside a source file (a Rust `#[cfg(test)]` module) is not a
//! test file here, and a file that merely sits in a `tests/` directory counts as one.

use crate::ports::FileChange;

/// Test files that existed on the base and the submission changed: modified (including renamed
/// away) and deleted. Added test files are not listed: adding tests weakens nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestEdits {
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

impl TestEdits {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modified.is_empty() && self.deleted.is_empty()
    }
}

/// Pure: the existing test files among `changes`.
#[must_use]
pub fn test_edits(changes: &[FileChange]) -> TestEdits {
    let mut edits = TestEdits::default();
    for change in changes {
        match change {
            FileChange::Added { .. } => {}
            FileChange::Modified { path } | FileChange::Renamed { from: path, .. } => {
                if is_test_path(path) {
                    edits.modified.push(path.clone());
                }
            }
            FileChange::Deleted { path } => {
                if is_test_path(path) {
                    edits.deleted.push(path.clone());
                }
            }
        }
    }
    edits
}

/// Pure: does this path name a test file, by the conventions of the languages the rig images
/// carry? A directory named for tests, or a file name marked as one.
#[must_use]
pub fn is_test_path(path: &str) -> bool {
    let mut parts = path.split('/').collect::<Vec<_>>();
    let Some(file) = parts.pop() else {
        return false;
    };
    let in_test_dir = parts.iter().any(|d| {
        matches!(
            *d,
            "test" | "tests" | "__tests__" | "spec" | "specs" | "testdata" | "e2e"
        )
    });
    let stem = file.split('.').next().unwrap_or(file);
    let marked = file.contains(".test.")
        || file.contains(".spec.")
        || file.contains("_test.")
        || file.contains("_tests.")
        || file.contains("_spec.")
        || stem.starts_with("test_")
        || stem.ends_with("Test")
        || stem.ends_with("Tests")
        || stem.ends_with("Spec");
    in_test_dir || marked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_each_languages_convention() {
        for path in [
            "tests/integration/inbound.test.ts",
            "src/ui/App.test.tsx",
            "web/form.spec.js",
            "pkg/mail/send_test.go",
            "app/test_models.py",
            "lib/parser_spec.rb",
            "crates/app/src/worker_tests.rs",
            "src/test/java/com/x/MailerTest.java",
            "spec/models/user.rb",
            "test/server_test.exs",
        ] {
            assert!(is_test_path(path), "{path}");
        }
        for path in [
            "src/wiring/server.ts",
            "docs/testing.md",
            "src/contest.rs",
            "latest.json",
            "src/attestation.go",
        ] {
            assert!(!is_test_path(path), "{path}");
        }
    }

    #[test]
    fn lists_changed_and_deleted_tests_but_not_new_ones() {
        let edits = test_edits(&[
            FileChange::Added {
                path: "tests/new.test.ts".into(),
            },
            FileChange::Modified {
                path: "tests/old.test.ts".into(),
            },
            FileChange::Modified {
                path: "src/server.ts".into(),
            },
            FileChange::Deleted {
                path: "tests/gone.test.ts".into(),
            },
            FileChange::Deleted {
                path: "src/unused.ts".into(),
            },
            FileChange::Renamed {
                from: "tests/moved.test.ts".into(),
                to: "src/moved.ts".into(),
            },
        ]);
        assert_eq!(edits.modified, ["tests/old.test.ts", "tests/moved.test.ts"]);
        assert_eq!(edits.deleted, ["tests/gone.test.ts"]);
        assert!(
            test_edits(&[FileChange::Added {
                path: "tests/a_test.go".into()
            }])
            .is_empty()
        );
    }
}
