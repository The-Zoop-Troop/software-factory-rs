//! `factory metrics`: render `app::metrics` reports as a table, JSON, or CSV.

use app::metrics::{EpicMetrics, StageStats};
use app::remote::EventRecord;
use std::fmt::Write as _;

/// Decode an `events.jsonl` body; undecodable lines are skipped like the console does.
#[must_use]
pub(crate) fn parse_log(text: &str) -> Vec<EventRecord> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Reports for `epic` or for every epic in the log.
#[must_use]
pub(crate) fn reports(log: &[EventRecord], epic: Option<&str>) -> Vec<EpicMetrics> {
    let ids = match epic {
        Some(e) => vec![e.to_owned()],
        None => app::metrics::epics_in(log),
    };
    ids.iter().map(|e| app::metrics::epic(e, log)).collect()
}

fn mmss(secs: i64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn stage_row(s: &StageStats) -> String {
    format!(
        "  {:<15}{:>8}{:>10}{:>10}{:>10}\n",
        s.stage,
        s.samples,
        mmss(s.p50),
        mmss(s.max),
        mmss(s.total)
    )
}

/// The human table for one epic.
#[must_use]
pub(crate) fn render(m: &EpicMetrics) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "epic {}", m.epic);
    let _ = writeln!(
        out,
        "  wall-clock {}  work {}  parallelism {}%  critical path {}  retry tax {}",
        mmss(m.wall_clock),
        mmss(m.work),
        m.parallelism_pct,
        mmss(m.critical_path),
        mmss(m.retry_tax)
    );
    let _ = writeln!(
        out,
        "  tasks {}  landed {}  first-pass {}  tokens {}  more workers could save up to {}",
        m.tasks.len(),
        m.landed,
        m.first_pass,
        m.tokens,
        mmss((m.wall_clock - m.critical_path).max(0))
    );
    let _ = writeln!(
        out,
        "  {:<15}{:>8}{:>10}{:>10}{:>10}",
        "stage", "n", "p50", "max", "total"
    );
    for s in &m.stages {
        out.push_str(&stage_row(s));
    }
    let peak = m.concurrency.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let _ = writeln!(out, "  peak live sessions {peak}");
    out
}

/// CSV: `epic,stage,samples,p50_s,max_s,total_s` per stage, plus one `summary` row per epic.
#[must_use]
pub(crate) fn csv(ms: &[EpicMetrics]) -> String {
    let mut out = String::from("epic,stage,samples,p50_s,max_s,total_s\n");
    for m in ms {
        for s in &m.stages {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{}",
                m.epic, s.stage, s.samples, s.p50, s.max, s.total
            );
        }
        let _ = writeln!(
            out,
            "{},summary,{},{},{},{}",
            m.epic,
            m.tasks.len(),
            m.wall_clock,
            m.critical_path,
            m.work
        );
    }
    out
}

/// `factory metrics` cannot read the log.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MetricsError {
    #[error("cannot read {path}: {detail}")]
    Unreadable { path: String, detail: String },
}

/// The report for a local `events.jsonl`.
///
/// # Errors
/// `Unreadable` when the file cannot be read.
pub(crate) fn from_file(
    path: &std::path::Path,
    epic: Option<&str>,
    json: bool,
    csv_out: bool,
) -> Result<String, MetricsError> {
    let text = std::fs::read_to_string(path).map_err(|e| MetricsError::Unreadable {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    let log = parse_log(&text);
    Ok(render_all(&reports(&log, epic), json, csv_out))
}

/// Render local reports in the asked format.
#[must_use]
pub(crate) fn render_all(ms: &[EpicMetrics], json: bool, csv_out: bool) -> String {
    if json {
        return serde_json::to_string_pretty(ms).unwrap_or_default() + "\n";
    }
    if csv_out {
        return csv(ms);
    }
    if ms.is_empty() {
        return "no epics in this log\n".to_owned();
    }
    ms.iter().map(render).collect::<Vec<_>>().join("\n")
}

/// Render a console reply (`{rig, epics: [EpicMetrics]}`) in the asked format.
#[must_use]
pub(crate) fn render_value(body: &serde_json::Value, json: bool, csv_out: bool) -> String {
    if json {
        return serde_json::to_string_pretty(body).unwrap_or_default() + "\n";
    }
    let ms: Vec<EpicMetrics> = body
        .get("epics")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    render_all(&ms, false, csv_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHASE0: &str = include_str!("../../app/fixtures/phase0-events.jsonl");

    #[test]
    fn table_json_and_csv_render_the_phase0_log() {
        let log = parse_log(PHASE0);
        let ms = reports(&log, None);
        assert_eq!(
            ms.iter().map(|m| m.epic.as_str()).collect::<Vec<_>>(),
            ["ex-1"]
        );
        let table = render_all(&ms, false, false);
        assert!(
            table.contains("epic ex-1")
                && table.contains("session")
                && table.contains("peak live sessions 1")
        );
        let json = render_all(&ms, true, false);
        assert!(json.contains("\"critical_path\""));
        let csv = render_all(&ms, false, true);
        assert!(csv.lines().count() == 1 + 6 + 1 && csv.contains("ex-1,summary,5,"));
        assert_eq!(render_all(&[], false, false), "no epics in this log\n");
        let only = reports(&log, Some("ex-1"));
        assert_eq!(only.len(), 1);
        let roundtrip = serde_json::json!({ "rig": "r", "epics": ms });
        assert_eq!(render_value(&roundtrip, false, false), table);
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;
    const PHASE0: &str = include_str!("../../app/fixtures/phase0-events.jsonl");
    #[test]
    fn from_file_reads_a_log_and_reports_missing_files() {
        let dir = std::env::temp_dir().join(format!("factory-metrics-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("events.jsonl");
        let _ = std::fs::write(&f, PHASE0);
        assert!(from_file(&f, None, false, false).is_ok_and(|s| s.contains("epic ex-1")));
        assert!(from_file(&dir.join("missing.jsonl"), None, false, false).is_err());
    }
}
