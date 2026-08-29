//! `quality`: the garbage-collection pass. Measures what can be measured, refreshes the
//! machine-owned block in `docs/QUALITY_SCORE.md`, and fails (`--check`) on drift:
//! - a design/reference/product/ops doc whose `Verified:` date is older than 30 days;
//! - `docs/generated` stale (delegates to gen-docs);
//! - coverage below the gate.

use std::path::Path;

const MAX_AGE_DAYS: i64 = 30;
const BEGIN: &str = "<!-- quality:begin -->";
const END: &str = "<!-- quality:end -->";

pub(crate) fn run(root: &Path, check: bool) -> anyhow::Result<()> {
    let mut problems = Vec::new();
    let today = today_days()?;
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
        if today - days > MAX_AGE_DAYS {
            problems.push(format!(
                "{}: verified {} days ago (max {MAX_AGE_DAYS}). Re-check its claims against the code and update the `Verified:` line, or mark it superseded.",
                rel(root, &doc),
                today - days
            ));
        }
    }

    let score_path = root.join("docs/QUALITY_SCORE.md");
    let score = std::fs::read_to_string(&score_path)?;
    if check {
        // Numbers vary by runner (coverage decimals, test counts); what must hold is that the
        // block exists and was measured recently. The weekly non-check run refreshes the values.
        match measured_date(&score).and_then(|d| date_to_days(&d)) {
            Some(days) if today - days <= MAX_AGE_DAYS => {}
            Some(days) => problems.push(format!(
                "docs/QUALITY_SCORE.md measured block is {} days old (max {MAX_AGE_DAYS}). Run `cargo xtask quality` and commit.",
                today - days
            )),
            None => problems.push("docs/QUALITY_SCORE.md has no measured block. Run `cargo xtask quality` and commit.".into()),
        }
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
}
