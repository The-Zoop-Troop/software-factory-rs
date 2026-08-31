//! Throughput metrics: a pure fold of a rig's event log into per-task stage timings and
//! per-epic totals (`docs/exec-plans/completed/throughput-metrics.md`). No I/O; the console and
//! the CLI feed it `EventRecord`s read from `events.jsonl`.
//!
//! Stage edges per attempt: `claimed → submitted → verify_started → verified →
//! integrate_started → integrated`; `released`, `lease_reaped`, `escalated` end an attempt.
//! A task is ready at `max(task_planned, integrated of each need)`.

use std::collections::BTreeMap;

use crate::remote::EventRecord;

/// Unix seconds.
type Secs = i64;

/// One session of a task, from claim to whatever ended it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Attempt {
    pub claimed: Secs,
    pub submitted: Option<Secs>,
    pub verify_started: Option<Secs>,
    pub verified: Option<Secs>,
    pub passed: Option<bool>,
    pub integrate_started: Option<Secs>,
    pub integrated: Option<Secs>,
    pub landed: bool,
    /// What ended the attempt when it did not land: `released`, `lease_reaped`, `escalated`,
    /// `verify_failed`, `rejected`.
    pub ended_by: Option<String>,
    pub tokens: u64,
}

impl Attempt {
    /// Claim to submit.
    #[must_use]
    pub fn session(&self) -> Option<Secs> {
        self.submitted.map(|s| s - self.claimed)
    }
}

/// A task's history across attempts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TaskMetrics {
    pub task: String,
    pub planned: Option<Secs>,
    pub needs: Vec<String>,
    pub attempts: Vec<Attempt>,
}

impl TaskMetrics {
    /// When the task landed, if it did.
    #[must_use]
    pub fn landed_at(&self) -> Option<Secs> {
        self.attempts
            .iter()
            .filter(|a| a.landed)
            .find_map(|a| a.integrated)
    }
    /// Time spent in sessions that did not land — the retry tax.
    #[must_use]
    pub fn retry_tax(&self) -> Secs {
        self.attempts
            .iter()
            .filter(|a| !a.landed)
            .filter_map(Attempt::session)
            .sum()
    }
    #[must_use]
    pub fn tokens(&self) -> u64 {
        self.attempts.iter().map(|a| a.tokens).sum()
    }
    /// Verified on the first attempt's first check.
    #[must_use]
    pub fn first_pass(&self) -> bool {
        self.attempts
            .first()
            .is_some_and(|a| a.passed == Some(true))
    }
}

/// A named duration sample set.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StageStats {
    pub stage: String,
    pub samples: usize,
    pub p50: Secs,
    pub max: Secs,
    pub total: Secs,
}

/// Everything the report shows for one epic.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EpicMetrics {
    pub epic: String,
    pub tasks: Vec<TaskMetrics>,
    /// First claim (or plan) to `epic_closed` (or the last event).
    pub wall_clock: Secs,
    /// Sum of all sessions, landed or not.
    pub work: Secs,
    /// `work / wall_clock`; 1.0 means strictly serial.
    pub parallelism_pct: u32,
    /// Longest chain of landed sessions along `needs` — the floor for wall-clock with unlimited
    /// workers. Without `task_planned` edges it is the longest single task.
    pub critical_path: Secs,
    pub retry_tax: Secs,
    pub first_pass: usize,
    pub landed: usize,
    pub tokens: u64,
    pub stages: Vec<StageStats>,
    /// Live sessions over time, seconds from the first claim: `(second, sessions)`, one entry
    /// per change.
    pub concurrency: Vec<(Secs, usize)>,
}

fn at(r: &EventRecord) -> Option<Secs> {
    r.at.parse::<Secs>().ok()
}

fn bead(r: &EventRecord) -> Option<&str> {
    r.bead.as_ref().map(AsRef::as_ref)
}

fn under(epic: &str, b: &str) -> bool {
    b == epic
        || b.strip_prefix(epic)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Fold the log into per-task histories for `epic`. Records outside the epic are ignored.
#[must_use]
pub fn tasks_of(epic: &str, log: &[EventRecord]) -> Vec<TaskMetrics> {
    let mut tasks: BTreeMap<String, TaskMetrics> = BTreeMap::new();
    for r in log {
        let (Some(t), Some(b)) = (at(r), bead(r)) else {
            continue;
        };
        if !under(epic, b) || b == epic {
            continue;
        }
        let task = tasks.entry(b.to_owned()).or_insert_with(|| TaskMetrics {
            task: b.to_owned(),
            ..TaskMetrics::default()
        });
        let last = task.attempts.last_mut();
        match r.kind.as_str() {
            "task_planned" => {
                task.planned = Some(t);
                task.needs = r
                    .detail
                    .get("needs")
                    .and_then(serde_json::Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
            }
            "claimed" => task.attempts.push(Attempt {
                claimed: t,
                ..Attempt::default()
            }),
            "submitted" => {
                if let Some(a) = last {
                    a.submitted = Some(t);
                    a.tokens = r
                        .detail
                        .get("tokens")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                }
            }
            "verify_started" => {
                if let Some(a) = last {
                    a.verify_started = Some(t);
                }
            }
            "verified" => {
                if let Some(a) = last {
                    let passed = r.detail.get("passed").and_then(serde_json::Value::as_bool);
                    a.verified = Some(t);
                    a.passed = passed;
                    if passed == Some(false) {
                        a.ended_by = Some("verify_failed".to_owned());
                    }
                }
            }
            "integrate_started" => {
                if let Some(a) = last {
                    a.integrate_started = Some(t);
                }
            }
            "integrated" => {
                if let Some(a) = last {
                    a.integrated = Some(t);
                    a.landed = r.detail.get("landed").is_some_and(|v| !v.is_null());
                    if !a.landed {
                        a.ended_by = Some("rejected".to_owned());
                    }
                }
            }
            "released" | "lease_reaped" | "escalated" => {
                if let Some(a) = last {
                    a.ended_by = Some(r.kind.clone());
                }
            }
            // Event kinds are an open set on disk (older and newer logs); anything else is
            // not a stage edge.
            unknown_kind => {
                debug_assert!(!unknown_kind.is_empty());
            }
        }
    }
    tasks.into_values().collect()
}

fn stats(stage: &str, mut xs: Vec<Secs>) -> StageStats {
    xs.sort_unstable();
    let samples = xs.len();
    let p50 = xs.get(samples / 2).copied().unwrap_or(0);
    StageStats {
        stage: stage.to_owned(),
        samples,
        p50,
        max: xs.last().copied().unwrap_or(0),
        total: xs.iter().sum(),
    }
}

fn stage_table(tasks: &[TaskMetrics]) -> Vec<StageStats> {
    let attempts = || tasks.iter().flat_map(|t| t.attempts.iter());
    let diff = |a: Option<Secs>, b: Option<Secs>| a.zip(b).map(|(x, y)| y - x);
    // Queue wait: ready → first claim. Ready = max(planned, landed_at of each need).
    let landed: BTreeMap<&str, Secs> = tasks
        .iter()
        .filter_map(|t| t.landed_at().map(|l| (t.task.as_str(), l)))
        .collect();
    let queue: Vec<Secs> = tasks
        .iter()
        .filter_map(|t| {
            let first = t.attempts.first()?.claimed;
            let ready = t
                .needs
                .iter()
                .filter_map(|n| landed.get(n.as_str()).copied())
                .chain(t.planned)
                .max()?;
            Some((first - ready).max(0))
        })
        .collect();
    vec![
        stats("queue_wait", queue),
        stats("session", attempts().filter_map(Attempt::session).collect()),
        stats(
            "verify_wait",
            attempts()
                .filter_map(|a| diff(a.submitted, a.verify_started))
                .collect(),
        ),
        stats(
            "verify",
            attempts()
                .filter_map(|a| diff(a.verify_started.or(a.submitted), a.verified))
                .collect(),
        ),
        stats(
            "integrate_wait",
            attempts()
                .filter_map(|a| diff(a.verified, a.integrate_started))
                .collect(),
        ),
        stats(
            "integrate",
            attempts()
                .filter_map(|a| diff(a.integrate_started.or(a.verified), a.integrated))
                .collect(),
        ),
    ]
}

/// Longest chain of landed sessions along `needs`; the longest single task without edges.
fn critical_path(tasks: &[TaskMetrics]) -> Secs {
    fn own(t: &TaskMetrics) -> Secs {
        t.attempts
            .iter()
            .filter(|a| a.landed)
            .filter_map(Attempt::session)
            .sum()
    }
    fn longest<'a>(
        t: &'a TaskMetrics,
        by_id: &BTreeMap<&str, &'a TaskMetrics>,
        memo: &mut BTreeMap<&'a str, Secs>,
        depth: usize,
    ) -> Secs {
        if let Some(v) = memo.get(t.task.as_str()) {
            return *v;
        }
        // A cycle cannot happen in a validated plan; depth-guard anyway rather than recurse forever.
        let deps = if depth > 64 {
            0
        } else {
            t.needs
                .iter()
                .filter_map(|n| by_id.get(n.as_str()).copied())
                .map(|d| longest(d, by_id, memo, depth + 1))
                .max()
                .unwrap_or(0)
        };
        let v = own(t) + deps;
        memo.insert(t.task.as_str(), v);
        v
    }
    let by_id: BTreeMap<&str, &TaskMetrics> = tasks.iter().map(|t| (t.task.as_str(), t)).collect();
    let mut memo = BTreeMap::new();
    tasks
        .iter()
        .map(|t| longest(t, &by_id, &mut memo, 0))
        .max()
        .unwrap_or(0)
}

/// Live sessions over time from `origin` (seconds), one entry per change.
fn concurrency(tasks: &[TaskMetrics], origin: Secs) -> Vec<(Secs, usize)> {
    let mut deltas: BTreeMap<Secs, i64> = BTreeMap::new();
    for a in tasks.iter().flat_map(|t| t.attempts.iter()) {
        let start = a.claimed - origin;
        let end = a.submitted.unwrap_or(a.claimed) - origin;
        *deltas.entry(start).or_insert(0) += 1;
        *deltas.entry(end).or_insert(0) -= 1;
    }
    let mut live: i64 = 0;
    deltas
        .into_iter()
        .map(|(sec, d)| {
            live += d;
            (sec, usize::try_from(live.max(0)).unwrap_or(0))
        })
        .collect()
}

/// Epic ids seen in the log (a bead `x-1.2` belongs to `x-1`; `x-1` itself is an epic when
/// it has children or was closed as one), in first-seen order.
#[must_use]
pub fn epics_in(log: &[EventRecord]) -> Vec<String> {
    let mut seen = Vec::new();
    for r in log {
        let Some(b) = bead(r) else { continue };
        let epic = match b.rsplit_once('.') {
            Some((e, _)) => e,
            None if r.kind == "epic_closed" || r.kind == "task_planned" => b,
            None => continue,
        };
        if !seen.iter().any(|s: &String| s == epic) {
            seen.push(epic.to_owned());
        }
    }
    seen
}

/// The report for one epic.
#[must_use]
pub fn epic(epic_id: &str, log: &[EventRecord]) -> EpicMetrics {
    let tasks = tasks_of(epic_id, log);
    let stamps = || {
        log.iter()
            .filter(|r| bead(r).is_some_and(|b| under(epic_id, b)))
            .filter_map(at)
    };
    let origin = tasks
        .iter()
        .filter_map(|t| t.planned.or_else(|| t.attempts.first().map(|a| a.claimed)))
        .min()
        .or_else(|| stamps().min())
        .unwrap_or(0);
    let end = log
        .iter()
        .find(|r| r.kind == "epic_closed" && bead(r) == Some(epic_id))
        .and_then(at)
        .or_else(|| stamps().max())
        .unwrap_or(origin);
    let wall_clock = (end - origin).max(0);
    let work: Secs = tasks
        .iter()
        .flat_map(|t| t.attempts.iter())
        .filter_map(Attempt::session)
        .sum();
    let parallelism_pct = if wall_clock == 0 {
        0
    } else {
        u32::try_from(work * 100 / wall_clock).unwrap_or(u32::MAX)
    };
    EpicMetrics {
        epic: epic_id.to_owned(),
        wall_clock,
        work,
        parallelism_pct,
        critical_path: critical_path(&tasks),
        retry_tax: tasks.iter().map(TaskMetrics::retry_tax).sum(),
        first_pass: tasks.iter().filter(|t| t.first_pass()).count(),
        landed: tasks.iter().filter(|t| t.landed_at().is_some()).count(),
        tokens: tasks.iter().map(TaskMetrics::tokens).sum(),
        stages: stage_table(&tasks),
        concurrency: concurrency(&tasks, origin),
        tasks,
    }
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
