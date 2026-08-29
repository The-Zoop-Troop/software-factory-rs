//! Plain-text renderings for the local `watch` and `bead show` commands.

use core::fmt::Write as _;

use infra::app::Bead;
use infra::app::domain::TaskState;

pub(crate) fn render_summary(s: &infra::app::LedgerSummary) -> String {
    let mut out = String::new();
    for (id, e) in &s.epics {
        let states = e
            .by_state
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "{id}  {}  [{}/{}] {states}",
            e.title,
            e.by_state.get("closed").copied().unwrap_or(0),
            e.total
        );
    }
    let _ = writeln!(
        out,
        "tasks outside epics: {}  incidents: {}  questions: {}",
        s.tasks_without_epic, s.open_incidents, s.open_questions
    );
    out
}

pub(crate) fn render(b: &Bead) -> String {
    let mut out = String::new();
    // Writing to a String cannot fail; the Results are discarded deliberately.
    let _ = writeln!(out, "{}  {}", b.id, b.title);
    let _ = writeln!(out, "  bd status : {}", b.status.as_str());
    match b.kind {
        Some(k) => {
            let _ = writeln!(out, "  kind      : {k}");
        }
        None => {
            let _ = writeln!(out, "  kind      : (not a factory bead)");
        }
    }
    if let Some(m) = &b.meta {
        let _ = writeln!(out, "  state     : {}", m.state.name());
        let detail = match &m.state {
            TaskState::Leased { lease } => Some(format!(
                "  lease     : {} until {} (claimed {})",
                lease.holder, lease.expires, lease.claimed_at
            )),
            TaskState::InVerify { branch, head } | TaskState::Mergeable { branch, head } => {
                Some(format!("  branch    : {branch} @ {head}"))
            }
            TaskState::Closed { merged } => Some(format!("  merged    : {merged}")),
            TaskState::Incident { reason } => Some(format!("  incident  : {reason:?}")),
            TaskState::Open => None,
        };
        if let Some(d) = detail {
            let _ = writeln!(out, "{d}");
        }
        let _ = writeln!(out, "  verify    : {}", m.verify_bead);
        let _ = writeln!(out, "  base      : {}", m.base);
        let _ = writeln!(
            out,
            "  budget    : tokens {}/{}  wall {}s/{}s  attempts {}/{}  lease expiries {}",
            m.usage.tokens,
            m.budget.tokens,
            m.usage.wall_clock.seconds(),
            m.budget.wall_clock.seconds(),
            m.usage.attempts,
            m.budget.attempts,
            m.lease_expiries
        );
    }
    if let Some(a) = &b.acceptance {
        let _ = writeln!(out, "  accept    : {a}");
    }
    if let Some(n) = &b.notes {
        let _ = writeln!(out, "  notes     :");
        for line in n.lines() {
            let _ = writeln!(out, "    {line}");
        }
    }
    out
}
