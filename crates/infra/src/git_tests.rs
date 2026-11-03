//! `GitCli` unit tests: the pure parsers over git's output.
#[test]
fn name_status_reads_every_kind_of_change() {
    use app::ports::FileChange as F;
    let out = "A\tnew.ts\nM\tsrc/a.ts\nD\ttests/gone.test.ts\nR087\ttests/old.test.ts\tsrc/old.ts\nC100\tsrc/a.ts\tsrc/b.ts\nT\tlink";
    assert_eq!(
        parse_name_status(out),
        [
            F::Added {
                path: "new.ts".into()
            },
            F::Modified {
                path: "src/a.ts".into()
            },
            F::Deleted {
                path: "tests/gone.test.ts".into()
            },
            F::Renamed {
                from: "tests/old.test.ts".into(),
                to: "src/old.ts".into()
            },
            F::Added {
                path: "src/b.ts".into()
            },
            F::Modified {
                path: "link".into()
            },
        ]
    );
    assert!(parse_name_status("").is_empty());
}
use super::*;

#[test]
fn summary_lists_files_and_added_public_surface_only() {
    let numstat = "3\t1\tsrc/lib.rs\n2\t0\tapi/routes.ts\n";
    let patch = "+++ b/src/lib.rs\n+pub fn balance() {}\n+fn private() {}\n+    let k = std::env::var(\"DATABASE_URL\");\n+++ b/api/routes.ts\n+export const listModels = () => {};\n+const hidden = 1;\n+func lower() {}\n+func Exported() {}\n";
    let s = summarise(numstat, patch);
    assert_eq!(s.files, vec!["src/lib.rs", "api/routes.ts"]);
    assert_eq!((s.insertions, s.deletions), (5, 1));
    assert_eq!(
        s.added_surface,
        vec![
            "pub fn balance() {}",
            "let k = std::env::var(\"DATABASE_URL\");",
            "export const listModels = () => {};",
            "func Exported() {}"
        ]
    );
}

#[test]
fn numstat_sums_lines_and_counts_untracked_files() {
    let stat = parse_numstat("12\t3\tsrc/a.rs\n-\t-\tlogo.png\n\n", "new.rs\nnotes.md\n");
    assert_eq!(
        stat,
        DiffStat {
            files: 4,
            insertions: 12,
            deletions: 3
        }
    );
    assert_eq!(parse_numstat("", ""), DiffStat::default());
}

#[test]
fn stderr_parses_into_each_variant() {
    assert!(matches!(
        parse_git_stderr(GitOp::RevParse, "fatal: Needed a single revision"),
        RepoError::RefNotFound { .. }
    ));
    let e = parse_git_stderr(
        GitOp::Rebase,
        "Auto-merging lib.sh\nCONFLICT (content): Merge conflict in lib.sh\nCONFLICT (content): Merge conflict in README.md\nerror: could not apply abc... msg",
    );
    assert!(
        matches!(&e, RepoError::Conflict { paths } if paths == &[PathBuf::from("lib.sh"), PathBuf::from("README.md")])
    );
    assert!(matches!(
        parse_git_stderr(
            GitOp::Push,
            "fatal: 'nowhere' does not appear to be a git repository"
        ),
        RepoError::Rejected {
            op: GitOp::Push,
            ..
        }
    ));
    assert!(conflict_paths("no conflicts here").is_empty());
}
