//! `quality`: the garbage-collection pass. Measures what can be measured, refreshes the
//! machine-owned block in `docs/QUALITY_SCORE.md`, and fails (`--check`) on drift:
//! - a design/reference/product/ops doc verified more than 30 days ago, when code has changed since;
//! - the measured block, by the same rule.
//!
//! Age alone is no drift: a doc checked against code that has not moved is still correct. Waiting
//! for code to move also means a quiet repository stays green, so a red run always has a cause.

use std::path::Path;
use std::process::Command;

const MAX_AGE_DAYS: i64 = 30;
const BEGIN: &str = "<!-- quality:begin -->";
const END: &str = "<!-- quality:end -->";
/// What does not count as code changing: prose, and the task ledger's own log.
const NOT_CODE: [&str; 3] = [
    ":(exclude)docs",
    ":(exclude).beads",
    ":(exclude,glob)**/*.md",
];

/// A date past the limit is stale only if code has changed since it.
fn is_stale(today: i64, checked: i64, last_code_change: i64) -> bool {
    today - checked > MAX_AGE_DAYS && last_code_change > checked
}

/// Day of the newest commit on this branch that changed code. A shallow clone would hide that
/// commit and turn every check green, so it is refused; CI checks out with `fetch-depth: 0`.
fn last_code_change(root: &Path) -> anyhow::Result<i64> {
    let git = |args: &[&str]| -> anyhow::Result<String> {
        let out = Command::new("git").args(args).current_dir(root).output()?;
        anyhow::ensure!(
            out.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };
    anyhow::ensure!(
        git(&["rev-parse", "--is-shallow-repository"])? != "true",
        "quality needs full git history to tell whether code changed; this clone is shallow (use fetch-depth: 0)"
    );
    let mut args = vec!["log", "-1", "--first-parent", "--format=%ct", "--", "."];
    args.extend(NOT_CODE);
    let seconds: i64 = git(&args)?.parse()?;
    Ok(seconds.div_euclid(86_400))
}

pub(crate) fn run(root: &Path, check: bool) -> anyhow::Result<()> {
    let today = today_days()?;
    let last_change = last_code_change(root)?;
    let mut problems = stale_docs(root, today, last_change)?;

    let score_path = root.join("docs/QUALITY_SCORE.md");
    let score = std::fs::read_to_string(&score_path)?;
    if check {
        // Numbers vary by runner (coverage decimals, test counts), so only the block's age is
        // checked. Nothing refreshes it automatically: when this fails, a person runs the command.
        problems.extend(stale_block(&score, today, last_change));
    } else {
        let coverage = measure_coverage(root);
        let tests = measure_tests(root);
        let block = format!(
            "{BEGIN}\n| Measure | Value | Measured |\n|---|---|---|\n| Line coverage (excl. xtask) | {} | {} |\n| Tests (nextest) | {} | {} |\n{END}",
            coverage.as_deref().unwrap_or("n/a"),
            days_to_date(today),
            tests.as_deref().unwrap_or("n/a"),
            days_to_date(today)
        );
        let updated = match (score.find(BEGIN), score.find(END)) {
            (Some(b), Some(e)) if e > b => {
                format!("{}{block}{}", &score[..b], &score[e + END.len()..])
            }
            _ => format!("{score}\n## Measured\n\n{block}\n"),
        };
        if updated != score {
            std::fs::write(&score_path, updated)?;
            println!("updated docs/QUALITY_SCORE.md measured block");
        }
    }

    if problems.is_empty() {
        println!("quality: ok");
        Ok(())
    } else {
        anyhow::bail!(
            "quality: {} problem(s)\n  - {}",
            problems.len(),
            problems.join("\n  - ")
        )
    }
}

fn stale_docs(root: &Path, today: i64, last_change: i64) -> anyhow::Result<Vec<String>> {
    let mut problems = Vec::new();
    for doc in crate::docs::scoped_docs(root)? {
        let text = std::fs::read_to_string(&doc)?;
        let Some(date) = verified_date(&text) else {
            continue;
        };
        let Some(days) = date_to_days(&date) else {
            problems.push(format!(
                "{}: `Verified:` date `{date}` is not YYYY-MM-DD.",
                rel(root, &doc)
            ));
            continue;
        };
        if is_stale(today, days, last_change) {
            problems.push(format!(
                "{}: verified {} days ago (max {MAX_AGE_DAYS}) and code has changed since. Re-check its claims against the code and update the `Verified:` line, or mark it superseded.",
                rel(root, &doc),
                today - days
            ));
        }
    }
    Ok(problems)
}

fn stale_block(score: &str, today: i64, last_change: i64) -> Option<String> {
    match measured_date(score).and_then(|d| date_to_days(&d)) {
        Some(days) if !is_stale(today, days, last_change) => None,
        Some(days) => Some(format!(
            "docs/QUALITY_SCORE.md measured block is {} days old (max {MAX_AGE_DAYS}) and code has changed since. Run `cargo xtask quality` and commit.",
            today - days
        )),
        None => Some(
            "docs/QUALITY_SCORE.md has no measured block. Run `cargo xtask quality` and commit."
                .into(),
        ),
    }
}

/// The most recent `Measured` date inside the machine-owned block.
fn measured_date(score: &str) -> Option<String> {
    let (b, e) = (score.find(BEGIN)?, score.find(END)?);
    let block = score.get(b..e)?;
    block
        .lines()
        .filter(|l| l.starts_with("| "))
        .filter_map(|l| l.rsplit('|').nth(1))
        .map(str::trim)
        .filter(|d| date_to_days(d).is_some())
        .map(str::to_owned)
        .max()
}

fn verified_date(text: &str) -> Option<String> {
    let i = text.find("**Verified:**")?;
    let rest = &text[i + "**Verified:**".len()..];
    let bytes = rest.as_bytes();
    (0..bytes.len().saturating_sub(9)).find_map(|k| {
        let s = rest.get(k..k + 10)?;
        (s.as_bytes().iter().enumerate().all(|(j, b)| {
            if j == 4 || j == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        }))
        .then(|| s.to_owned())
    })
}

fn measure_coverage(root: &Path) -> Option<String> {
    let out = std::process::Command::new("cargo")
        .args([
            "llvm-cov",
            "nextest",
            "--workspace",
            "--exclude",
            "xtask",
            "--all-features",
            "--summary-only",
        ])
        .current_dir(root)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let total = text.lines().find(|l| l.starts_with("TOTAL"))?;
    let cols: Vec<&str> = total.split_whitespace().collect();
    cols.get(cols.len().checked_sub(4)?)
        .map(|s| (*s).to_owned())
}

fn measure_tests(root: &Path) -> Option<String> {
    let out = std::process::Command::new("cargo")
        .args(["nextest", "run", "--workspace", "--all-features"])
        .current_dir(root)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text.lines().find(|l| l.contains("Summary"))?;
    let i = line.find(']')?;
    Some(line[i + 1..].trim().to_owned())
}

// --- civil-date arithmetic without a dependency (days since 1970-01-01) ---

fn today_days() -> anyhow::Result<i64> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    Ok(i64::try_from(secs / 86_400)?)
}

fn date_to_days(s: &str) -> Option<i64> {
    let mut it = s.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn days_to_date(z: i64) -> String {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn date_roundtrip_and_verified_extraction() {
        for s in ["1970-01-01", "2000-02-29", "2026-08-29", "2099-12-31"] {
            assert_eq!(days_to_date(date_to_days(s).unwrap()), s);
        }
        assert_eq!(date_to_days("2026-13-01"), None);
        assert_eq!(
            verified_date("- **Status:** accepted · **Verified:** against code, 2026-08-29 · x")
                .as_deref(),
            Some("2026-08-29")
        );
        assert_eq!(verified_date("no marker"), None);
        let block = format!(
            "x\n{BEGIN}\n| a | b | c |\n|---|---|---|\n| cov | 1% | 2026-08-29 |\n| tests | 3 | 2026-08-30 |\n{END}\n"
        );
        assert_eq!(measured_date(&block).as_deref(), Some("2026-08-30"));
        assert_eq!(measured_date("no block"), None);
    }

    #[test]
    fn age_alone_is_not_stale() {
        let day = |s| date_to_days(s).unwrap();
        let checked = day("2026-11-02");
        let later = day("2026-12-20");
        // Past the limit, no code since: green. Past the limit, code since: stale.
        assert!(!is_stale(later, checked, day("2026-11-01")));
        assert!(!is_stale(later, checked, checked));
        assert!(is_stale(later, checked, day("2026-11-03")));
        // Within the limit, code since: the doc still has time to catch up.
        assert!(!is_stale(day("2026-12-02"), checked, day("2026-11-03")));
        assert!(is_stale(day("2026-12-03"), checked, day("2026-11-03")));
    }

    #[test]
    fn block_follows_the_same_rule() {
        let day = |s| date_to_days(s).unwrap();
        let score = format!("{BEGIN}\n| cov | 1% | 2026-11-02 |\n{END}\n");
        let today = day("2026-12-20");
        assert_eq!(stale_block(&score, today, day("2026-11-01")), None);
        assert!(
            stale_block(&score, today, day("2026-11-03"))
                .unwrap()
                .contains("code has changed since")
        );
        assert!(
            stale_block("no block", today, day("2026-11-01"))
                .unwrap()
                .contains("no measured block")
        );
    }

    struct TempRepo(std::path::PathBuf);

    impl TempRepo {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("xtask-quality-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("docs")).unwrap();
            let repo = Self(dir);
            repo.git(&["init", "-q", "-b", "main"], "2026-01-01");
            repo
        }
        fn git(&self, args: &[&str], date: &str) {
            let when = format!("{date}T12:00:00+00:00");
            let status = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
                .args([
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(&self.0)
                .env("GIT_AUTHOR_DATE", &when)
                .env("GIT_COMMITTER_DATE", &when)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
        fn commit(&self, path: &str, date: &str) {
            std::fs::write(self.0.join(path), date).unwrap();
            self.git(&["add", "-A"], date);
            self.git(&["commit", "-q", "-m", path], date);
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn only_code_commits_move_the_date() {
        let repo = TempRepo::new("code");
        repo.commit("main.rs", "2026-01-10");
        let day = |s| date_to_days(s).unwrap();
        assert_eq!(last_code_change(&repo.0).unwrap(), day("2026-01-10"));
        repo.commit("docs/diagram.svg", "2026-02-01");
        repo.commit("README.md", "2026-02-02");
        std::fs::create_dir_all(repo.0.join(".beads")).unwrap();
        repo.commit(".beads/interactions.jsonl", "2026-02-03");
        assert_eq!(last_code_change(&repo.0).unwrap(), day("2026-01-10"));
        repo.commit("main.rs", "2026-03-01");
        assert_eq!(last_code_change(&repo.0).unwrap(), day("2026-03-01"));
    }

    #[test]
    fn a_merge_counts_on_the_day_it_landed() {
        let repo = TempRepo::new("merge");
        repo.commit("main.rs", "2026-01-10");
        repo.git(&["switch", "-q", "-c", "topic"], "2026-01-11");
        repo.commit("lib.rs", "2026-01-11");
        repo.git(&["switch", "-q", "main"], "2026-01-11");
        repo.git(
            &["merge", "-q", "--no-ff", "-m", "m", "topic"],
            "2026-02-20",
        );
        assert_eq!(
            last_code_change(&repo.0).unwrap(),
            date_to_days("2026-02-20").unwrap()
        );
    }

    #[test]
    fn a_shallow_clone_is_refused_rather_than_read_as_quiet() {
        let repo = TempRepo::new("shallow-src");
        repo.commit("main.rs", "2026-01-10");
        repo.commit("docs/a.md", "2026-02-01");
        let copy = repo.0.with_extension("copy");
        let _ = std::fs::remove_dir_all(&copy);
        let url = format!("file://{}", repo.0.display());
        repo.git(
            &["clone", "-q", "--depth", "1", &url, copy.to_str().unwrap()],
            "2026-02-02",
        );
        let err = last_code_change(&copy).unwrap_err();
        let _ = std::fs::remove_dir_all(&copy);
        assert!(err.to_string().contains("shallow"), "{err}");
    }

    #[test]
    fn a_stale_doc_is_reported_only_when_code_moved() {
        let repo = TempRepo::new("docs");
        std::fs::write(
            repo.0.join("docs/ARCH.md"),
            "- **Status:** accepted · **Verified:** 2026-11-02\n",
        )
        .unwrap();
        let day = |s| date_to_days(s).unwrap();
        let today = day("2026-12-20");
        assert!(
            stale_docs(&repo.0, today, day("2026-11-01"))
                .unwrap()
                .is_empty()
        );
        let found = stale_docs(&repo.0, today, day("2026-11-03")).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found.iter().all(|p| p.contains("docs/ARCH.md")));
    }
}
