//! Read models for the operator console (`factory watch` / `factory inbox`): what a human
//! needs to see, computed from the ledger and nothing else.

use std::collections::BTreeMap;

use domain::{BeadId, BeadKind, TaskState};

use crate::bead::Bead;
use crate::ports::{BeadStore, StoreError};

/// Per-epic task counts by factory state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EpicSummary {
    pub title: String,
    /// state name → count, e.g. `open=2 leased=1 closed=4`.
    pub by_state: BTreeMap<&'static str, usize>,
    pub total: usize,
}

/// Everything `factory watch` renders.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LedgerSummary {
    pub epics: BTreeMap<BeadId, EpicSummary>,
    pub tasks_without_epic: usize,
    pub open_incidents: usize,
    pub open_questions: usize,
}

/// Summarize active work. Tasks are grouped by parent epic; closed tasks are included so
/// progress reads as `closed=4/5`.
///
/// # Errors
/// Ledger failures.
pub async fn ledger_summary(store: &dyn BeadStore) -> Result<LedgerSummary, StoreError> {
    let mut summary = LedgerSummary::default();
    for epic in store.list_active(BeadKind::Epic).await? {
        let children = store.children(&epic.id).await?;
        let mut es = EpicSummary {
            title: epic.title.clone(),
            ..EpicSummary::default()
        };
        for task in children
            .into_iter()
            .filter(|c| c.kind == Some(BeadKind::Task))
        {
            let state = task.meta.as_ref().map_or("unknown", |m| m.state.name());
            *es.by_state.entry(state).or_default() += 1;
            es.total += 1;
        }
        summary.epics.insert(epic.id, es);
    }
    let epic_ids: Vec<BeadId> = summary.epics.keys().cloned().collect();
    summary.tasks_without_epic = store
        .list_active(BeadKind::Task)
        .await?
        .into_iter()
        .filter(|t| !t.parent.as_ref().is_some_and(|p| epic_ids.contains(p)))
        .count();
    summary.open_incidents = store.list_active(BeadKind::Incident).await?.len();
    summary.open_questions = store.list_active(BeadKind::Question).await?.len();
    Ok(summary)
}

/// Beads that need a human: open incidents and questions, incidents first.
///
/// # Errors
/// Ledger failures.
pub async fn inbox(store: &dyn BeadStore) -> Result<Vec<Bead>, StoreError> {
    let mut items = store.list_active(BeadKind::Incident).await?;
    items.extend(store.list_active(BeadKind::Question).await?);
    Ok(items)
}

/// Resolve an inbox item: close it with the operator's note. For an incident whose task is
/// still in `incident` state, the task is also reopened with attempts reset so work resumes.
///
/// # Errors
/// `NotFound` for an unknown id; other ledger failures.
pub async fn resolve(
    store: &dyn BeadStore,
    id: &BeadId,
    note: &str,
) -> Result<Option<BeadId>, StoreError> {
    let bead = store.show(id).await?;
    store.close(id, note).await?;
    let Some(task_id) = incident_task(&bead) else {
        return Ok(None);
    };
    let task = store.show(&task_id).await?;
    if let Some(meta) = task.meta
        && matches!(meta.state, TaskState::Incident { .. })
    {
        let reopened = domain::FactoryMeta {
            state: TaskState::Open,
            usage: domain::Usage {
                attempts: 0,
                ..meta.usage
            },
            lease_expiries: 0,
            ..meta
        };
        store.set_meta(&task_id, &reopened).await?;
        store
            .note(&task_id, &format!("incident resolved by operator: {note}"))
            .await?;
        return Ok(Some(task_id));
    }
    Ok(None)
}

/// Incident beads are titled `incident on <task-id>` by `transition::run_effect`.
fn incident_task(bead: &Bead) -> Option<BeadId> {
    if bead.kind != Some(BeadKind::Incident) {
        return None;
    }
    bead.title
        .strip_prefix("incident on ")
        .and_then(|s| BeadId::try_new(s.trim()).ok())
}

#[cfg(test)]
mod tests {
    use domain::{Budget, FactoryMeta, Sha, Usage};

    use super::*;
    use crate::bead::NewBead;
    use crate::testing::FakeStore;

    fn id(s: &str) -> BeadId {
        BeadId::try_new(s).unwrap()
    }
    fn meta(state: TaskState) -> FactoryMeta {
        FactoryMeta {
            verify_bead: id("fac-v"),
            base: Sha::try_new("a".repeat(40)).unwrap(),
            budget: Budget::default(),
            usage: Usage {
                attempts: 3,
                ..Usage::default()
            },
            lease_expiries: 2,
            state,
        }
    }

    #[tokio::test]
    async fn summary_groups_tasks_by_epic_and_state() {
        let store = FakeStore::default();
        store.seed_epic(id("fac-e"), &[]).await;
        store.seed_task(id("fac-e.1"), meta(TaskState::Open)).await;
        store
            .seed_task(
                id("fac-e.2"),
                meta(TaskState::Incident {
                    reason: domain::task::IncidentReason::Manual { detail: "x".into() },
                }),
            )
            .await;
        store.set_parent(&id("fac-e.1"), &id("fac-e")).await;
        store.set_parent(&id("fac-e.2"), &id("fac-e")).await;
        store
            .seed_task(id("fac-loose"), meta(TaskState::Open))
            .await;
        store
            .create(NewBead {
                title: "incident on fac-e.2".into(),
                description: String::new(),
                kind: BeadKind::Incident,
                priority: 0,
                parent: None,
                needs: vec![],
                acceptance: None,
                meta: None,
            })
            .await
            .unwrap();
        let s = ledger_summary(&store).await.unwrap();
        let e = &s.epics[&id("fac-e")];
        assert_eq!(e.total, 2);
        assert_eq!(e.by_state["open"], 1);
        assert_eq!(e.by_state["incident"], 1);
        assert_eq!(s.tasks_without_epic, 1);
        assert_eq!((s.open_incidents, s.open_questions), (1, 0));
    }

    #[tokio::test]
    async fn resolve_closes_incident_and_reopens_task() {
        let store = FakeStore::default();
        store
            .seed_task(
                id("fac-t"),
                meta(TaskState::Incident {
                    reason: domain::task::IncidentReason::Manual { detail: "x".into() },
                }),
            )
            .await;
        let inc = store
            .create(NewBead {
                title: "incident on fac-t".into(),
                description: String::new(),
                kind: BeadKind::Incident,
                priority: 0,
                parent: None,
                needs: vec![],
                acceptance: None,
                meta: None,
            })
            .await
            .unwrap();
        assert_eq!(inbox(&store).await.unwrap().len(), 1);
        let reopened = resolve(&store, &inc, "fixed the verify command")
            .await
            .unwrap();
        assert_eq!(reopened, Some(id("fac-t")));
        let t = store.show(&id("fac-t")).await.unwrap();
        let m = t.meta.unwrap();
        assert_eq!(m.state, TaskState::Open);
        assert_eq!((m.usage.attempts, m.lease_expiries), (0, 0));
        assert!(t.notes.unwrap().contains("fixed the verify"));
        assert!(inbox(&store).await.unwrap().is_empty());
        // A question resolves without touching any task.
        let q = store
            .create(NewBead {
                title: "which db?".into(),
                description: String::new(),
                kind: BeadKind::Question,
                priority: 1,
                parent: None,
                needs: vec![],
                acceptance: None,
                meta: None,
            })
            .await
            .unwrap();
        assert_eq!(resolve(&store, &q, "postgres").await.unwrap(), None);
        assert!(matches!(
            resolve(&store, &id("fac-nope"), "x").await,
            Err(StoreError::NotFound { .. })
        ));
    }
}
