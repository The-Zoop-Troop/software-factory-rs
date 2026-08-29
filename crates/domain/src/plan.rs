//! A plan as the Planner emits it (ARCHITECTURE.md §4.1): tasks with acceptance criteria,
//! executable verify commands, and `needs` edges. Parsed once from the model's structured
//! output; invalid plans never reach the ledger.

use std::collections::{BTreeMap, BTreeSet};

use crate::budget::Budget;
use crate::nonempty::NonEmpty;
use crate::text::{Title, VerifyCommand};
use crate::time::Duration;

/// Stable key the model uses to reference tasks within one plan (not a bead id).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct TaskKey(String);

impl TaskKey {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for TaskKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One planned task, validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTask {
    pub key: TaskKey,
    pub title: Title,
    pub description: String,
    pub acceptance: Vec<String>,
    /// Shell commands proving `acceptance`; run in order in a fresh worktree of the branch.
    pub verify: NonEmpty<VerifyCommand>,
    /// Keys of tasks this one NEEDS (blocks edges).
    pub needs: Vec<TaskKey>,
    pub budget: Budget,
    pub verify_timeout: Duration,
}

/// A validated plan: non-empty, unique keys, every `needs` resolves, acyclic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub summary: Title,
    /// Architecture notes / decisions the Planner wants every worker to see.
    pub reference: Option<String>,
    /// Tasks in a topological order (needs before dependents).
    pub tasks: NonEmpty<PlannedTask>,
}

/// Wire shape: what the JSON schema handed to the model describes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawPlan {
    pub summary: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub reference: Option<String>,
    pub tasks: Vec<RawPlannedTask>,
}

/// Wire shape of one task.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawPlannedTask {
    pub key: String,
    pub title: String,
    pub description: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub acceptance: Vec<String>,
    pub verify: Vec<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub needs: Vec<String>,
}

/// Why a raw plan was rejected. Each variant names the offending task so the Planner can be re-prompted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("plan has no tasks")]
    Empty,
    #[error("task key `{key}` is empty or malformed")]
    BadKey { key: String },
    #[error("duplicate task key `{key}`")]
    DuplicateKey { key: String },
    #[error("task `{key}` has an invalid title: {source}")]
    EmptyTitle {
        key: String,
        source: crate::text::TitleError,
    },
    #[error("plan summary is invalid: {source}")]
    BadSummary { source: crate::text::TitleError },
    #[error("task `{key}` has an invalid verify command: {source}")]
    BadVerify {
        key: String,
        source: crate::text::VerifyCommandError,
    },
    #[error("task `{key}` has no verify commands")]
    NoVerify { key: String },
    #[error("task `{task}` needs unknown task `{needs}`")]
    UnknownNeed { task: String, needs: String },
    #[error("task `{key}` needs itself")]
    SelfNeed { key: String },
    #[error("dependency cycle involving `{key}`")]
    Cycle { key: String },
}

impl RawPlan {
    /// Validate and topologically order.
    ///
    /// # Errors
    /// The first structural problem found, naming the task.
    pub fn validate(self, defaults: PlanDefaults) -> Result<Plan, PlanError> {
        if self.tasks.is_empty() {
            return Err(PlanError::Empty);
        }
        let mut seen = BTreeSet::new();
        let mut tasks = Vec::with_capacity(self.tasks.len());
        for raw in self.tasks {
            let key = parse_key(&raw.key)?;
            if !seen.insert(key.clone()) {
                return Err(PlanError::DuplicateKey { key: raw.key });
            }
            let title = Title::try_new(raw.title).map_err(|source| PlanError::EmptyTitle {
                key: raw.key.clone(),
                source,
            })?;
            let verify = raw
                .verify
                .into_iter()
                .filter(|c| !c.trim().is_empty())
                .map(VerifyCommand::try_new)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source| PlanError::BadVerify {
                    key: raw.key.clone(),
                    source,
                })?;
            let verify = NonEmpty::try_from(verify).map_err(|_| PlanError::NoVerify {
                key: raw.key.clone(),
            })?;
            // Duplicate needs are a plausible model output; keep first occurrence so the
            // in-degree count in `topo_sort` matches the decrements it will see.
            let mut needs: Vec<TaskKey> = Vec::new();
            for n in &raw.needs {
                let k = parse_key(n)?;
                if !needs.contains(&k) {
                    needs.push(k);
                }
            }
            if needs.contains(&key) {
                return Err(PlanError::SelfNeed { key: raw.key });
            }
            tasks.push(PlannedTask {
                key,
                title,
                description: raw.description,
                acceptance: raw.acceptance,
                verify,
                needs,
                budget: defaults.budget,
                verify_timeout: defaults.verify_timeout,
            });
        }
        for t in &tasks {
            if let Some(n) = t.needs.iter().find(|n| !seen.contains(*n)) {
                return Err(PlanError::UnknownNeed {
                    task: t.key.to_string(),
                    needs: n.to_string(),
                });
            }
        }
        let tasks = topo_sort(tasks)?;
        let tasks = NonEmpty::try_from(tasks).map_err(|_| PlanError::Empty)?;
        let summary =
            Title::try_new(self.summary).map_err(|source| PlanError::BadSummary { source })?;
        Ok(Plan {
            summary,
            reference: self.reference.filter(|r| !r.trim().is_empty()),
            tasks,
        })
    }
}

/// Per-task defaults the Planner doesn't set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanDefaults {
    pub budget: Budget,
    pub verify_timeout: Duration,
}

impl Default for PlanDefaults {
    fn default() -> Self {
        Self {
            budget: Budget::default(),
            verify_timeout: Duration::from_minutes(20),
        }
    }
}

fn parse_key(s: &str) -> Result<TaskKey, PlanError> {
    let t = s.trim();
    if t.is_empty()
        || !t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(PlanError::BadKey { key: s.to_owned() });
    }
    Ok(TaskKey(t.to_owned()))
}

/// Kahn's algorithm; stable with respect to input order among ready tasks.
fn topo_sort(tasks: Vec<PlannedTask>) -> Result<Vec<PlannedTask>, PlanError> {
    let mut indegree: BTreeMap<TaskKey, usize> = tasks
        .iter()
        .map(|t| (t.key.clone(), t.needs.len()))
        .collect();
    let mut pending: Vec<PlannedTask> = tasks;
    let mut ordered = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let Some(pos) = pending
            .iter()
            .position(|t| indegree.get(&t.key).copied() == Some(0))
        else {
            let stuck = pending
                .first()
                .map(|t| t.key.to_string())
                .unwrap_or_default();
            return Err(PlanError::Cycle { key: stuck });
        };
        let done = pending.remove(pos);
        for t in &pending {
            if t.needs.contains(&done.key)
                && let Some(d) = indegree.get_mut(&t.key)
            {
                *d = d.saturating_sub(1);
            }
        }
        ordered.push(done);
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(key: &str, needs: &[&str]) -> RawPlannedTask {
        RawPlannedTask {
            key: key.into(),
            title: format!("task {key}"),
            description: String::new(),
            acceptance: vec![],
            verify: vec!["true".into()],
            needs: needs.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
    fn plan(tasks: Vec<RawPlannedTask>) -> RawPlan {
        RawPlan {
            summary: "s".into(),
            reference: None,
            tasks,
        }
    }

    #[test]
    fn orders_needs_first() {
        let p = plan(vec![raw("c", &["b"]), raw("a", &[]), raw("b", &["a"])])
            .validate(PlanDefaults::default())
            .unwrap();
        let keys: Vec<_> = p.tasks.iter().map(|t| t.key.as_str().to_owned()).collect();
        assert_eq!(keys, ["a", "b", "c"]);
    }

    #[test]
    fn blank_reference_is_none_and_keys_display() {
        let mut p = plan(vec![raw("a", &[])]);
        p.reference = Some("   ".into());
        let v = p.validate(PlanDefaults::default()).unwrap();
        assert_eq!(v.reference, None);
        assert_eq!(v.tasks.first().key.to_string(), "a");
        assert_eq!(v.tasks.first().key.as_str(), "a");
        assert!(matches!(
            plan(vec![raw("has space", &[])]).validate(PlanDefaults::default()),
            Err(PlanError::BadKey { .. })
        ));
        assert!(matches!(
            plan(vec![raw("", &[])]).validate(PlanDefaults::default()),
            Err(PlanError::BadKey { .. })
        ));
        assert!(
            plan(vec![raw("ok-key_1", &[])])
                .validate(PlanDefaults::default())
                .is_ok()
        );
    }

    #[test]
    fn duplicate_needs_are_not_a_cycle() {
        let p = plan(vec![raw("a", &[]), raw("b", &["a", "a"])])
            .validate(PlanDefaults::default())
            .unwrap();
        assert_eq!(p.tasks.len(), 2);
        assert_eq!(
            p.tasks.iter().last().unwrap().needs.len(),
            1,
            "deduplicated"
        );
    }

    #[test]
    fn rejects_cycle_dangling_dup_self_empty() {
        let d = PlanDefaults::default();
        assert!(matches!(
            plan(vec![raw("a", &["b"]), raw("b", &["a"])]).validate(d),
            Err(PlanError::Cycle { .. })
        ));
        assert!(matches!(
            plan(vec![raw("a", &["zz"])]).validate(d),
            Err(PlanError::UnknownNeed { .. })
        ));
        assert!(matches!(
            plan(vec![raw("a", &[]), raw("a", &[])]).validate(d),
            Err(PlanError::DuplicateKey { .. })
        ));
        assert!(matches!(
            plan(vec![raw("a", &["a"])]).validate(d),
            Err(PlanError::SelfNeed { .. })
        ));
        assert!(matches!(plan(vec![]).validate(d), Err(PlanError::Empty)));
        let mut nv = raw("a", &[]);
        nv.verify = vec!["  ".into()];
        assert!(matches!(
            plan(vec![nv]).validate(d),
            Err(PlanError::NoVerify { .. })
        ));
        assert!(matches!(
            plan(vec![raw("bad key", &[])]).validate(d),
            Err(PlanError::BadKey { .. })
        ));
    }
}
