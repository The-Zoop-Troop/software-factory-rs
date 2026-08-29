//! The one workflow every agent shares: load a task, apply an event, persist, run effects.
//!
//! Worker, Verifier, Integrator and Steward never touch bead metadata directly —
//! they call `apply_event` with a fact and let the domain decide.

use domain::task::Effect;
use domain::{
    BeadId, BeadKind, BeadMeta, Event, FactoryMeta, IllegalTransition, MergeMeta, Priority, Task,
    Title, Transition,
};

use crate::bead::NewBead;
use crate::ports::{BeadStore, RepoError, RunError, StoreError};

/// Failures of the transition workflow.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Repo(#[from] RepoError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("bead {0} is not a factory task (no fac:kind=task label)")]
    NotATask(BeadId),
    #[error("bead {0} has no factory metadata")]
    NoMeta(BeadId),
    #[error(transparent)]
    Illegal(#[from] IllegalTransition),
}

/// Load a task bead into its typed form.
///
/// # Errors
/// `NotATask` / `NoMeta` if the bead isn't a factory task; store failures otherwise.
pub async fn load_task(store: &dyn BeadStore, id: &BeadId) -> Result<Task, TransitionError> {
    let bead = store.show(id).await?;
    if bead.kind != Some(BeadKind::Task) {
        return Err(TransitionError::NotATask(id.clone()));
    }
    let meta = bead
        .meta
        .ok_or_else(|| TransitionError::NoMeta(id.clone()))?;
    Ok(meta.into_task(id.clone()))
}

/// Apply `event` to task `id`: decide (pure), persist the new state, then run the effects.
///
/// Persisting before effects means a crash mid-effects leaves the ledger in the new
/// state with some effects missing — which the Steward can detect — rather than in the
/// old state with effects already applied, which nothing could untangle.
///
/// # Errors
/// Store failures, or `Illegal` if the event is not valid in the task's current state.
#[tracing::instrument(skip(store), err)]
pub async fn apply_event(
    store: &dyn BeadStore,
    id: &BeadId,
    event: Event,
) -> Result<Transition, TransitionError> {
    let task = load_task(store, id).await?;
    let transition = task.apply(event)?;
    store
        .set_meta(id, &FactoryMeta::from(transition.task.clone()))
        .await?;
    for effect in &transition.effects {
        run_effect(store, effect).await?;
    }
    Ok(transition)
}

async fn run_effect(store: &dyn BeadStore, effect: &Effect) -> Result<(), StoreError> {
    match effect {
        Effect::AppendNote { task: id, note } => store.note(id, note).await,
        Effect::OpenMergeBead {
            task: id,
            branch,
            head,
        } => open_merge_bead(store, id, branch, head).await,
        Effect::OpenIncidentBead { task: id, reason } => store
            .create(NewBead {
                title: Title::derived(&format!("incident on {id}")),
                description: format!("{reason:?}"),
                kind: BeadKind::Incident,
                priority: Priority::CRITICAL,
                parent: None,
                needs: vec![],
                acceptance: None,
                meta: None,
            })
            .await
            .map(|_| ()),
        Effect::CloseTaskBead { task: id } => store.close(id, "merged to main").await,
        Effect::CloseVerifyBead { verify } => store.close(verify, "paired task merged").await,
    }
}

/// Create the merge bead for a verified task. Also used by the Steward to repair a task left
/// `mergeable` without one (a crash between persist and effect).
///
/// # Errors
/// Ledger failures.
pub async fn open_merge_bead(
    store: &dyn BeadStore,
    id: &BeadId,
    branch: &domain::BranchName,
    head: &domain::Sha,
) -> Result<(), StoreError> {
    store
        .create(NewBead {
            title: Title::derived(&format!("merge {branch} for {id}")),
            description: format!(
                "Branch `{branch}` at {head} passed verification and awaits the Integrator."
            ),
            kind: BeadKind::Merge,
            priority: Priority::HIGH,
            parent: None,
            needs: vec![],
            acceptance: None,
            meta: Some(BeadMeta::Merge(MergeMeta {
                task: id.clone(),
                branch: branch.clone(),
                head: head.clone(),
            })),
        })
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use domain::{AgentId, Budget, Duration, Sha, TaskState, Timestamp, Usage};

    use super::*;
    use crate::testing::FakeStore;
    use domain::{Attempts, Tokens};

    fn id(s: &str) -> BeadId {
        BeadId::try_new(s).unwrap()
    }
    fn sha(c: char) -> Sha {
        Sha::try_new(core::iter::repeat_n(c, 40).collect::<String>()).unwrap()
    }

    async fn store_with_task() -> FakeStore {
        let store = FakeStore::default();
        store
            .seed_task(
                id("fac-1"),
                FactoryMeta {
                    verify_bead: id("fac-2"),
                    base: sha('a'),
                    budget: Budget {
                        attempts: Attempts::new(1),
                        ..Budget::default()
                    },
                    usage: Usage::default(),
                    lease_expiries: Attempts::new(0),
                    state: TaskState::Open,
                },
            )
            .await;
        store
    }

    #[tokio::test]
    async fn claim_persists_lease() {
        let store = store_with_task().await;
        let tr = apply_event(
            &store,
            &id("fac-1"),
            Event::Claim {
                holder: AgentId::try_new("w1").unwrap(),
                now: Timestamp::from_unix_seconds(0),
                ttl: Duration::from_seconds(60),
            },
        )
        .await
        .unwrap();
        assert_eq!(tr.task.state.name(), "leased");
        let reloaded = load_task(&store, &id("fac-1")).await.unwrap();
        assert_eq!(reloaded.state, tr.task.state);
    }

    #[tokio::test]
    async fn verify_failure_over_budget_opens_incident_bead() {
        let store = store_with_task().await;
        let now = Timestamp::from_unix_seconds(0);
        apply_event(
            &store,
            &id("fac-1"),
            Event::Claim {
                holder: AgentId::try_new("w1").unwrap(),
                now,
                ttl: Duration::from_seconds(60),
            },
        )
        .await
        .unwrap();
        apply_event(
            &store,
            &id("fac-1"),
            Event::Submit {
                holder: AgentId::try_new("w1").unwrap(),
                branch: domain::BranchName::try_new("task/fac-1").unwrap(),
                head: sha('b'),
                now,
                tokens: Tokens::new(10),
            },
        )
        .await
        .unwrap();
        let tr = apply_event(
            &store,
            &id("fac-1"),
            Event::VerifyFailed {
                note: "nope".into(),
            },
        )
        .await
        .unwrap();
        assert!(matches!(tr.task.state, TaskState::Incident { .. }));
        let incidents = store.list_active(BeadKind::Incident).await.unwrap();
        assert_eq!(incidents.len(), 1);
        assert!(
            store
                .show(&id("fac-1"))
                .await
                .unwrap()
                .notes
                .unwrap()
                .contains("nope")
        );
    }

    #[tokio::test]
    async fn crash_between_persist_and_effect_leaves_a_detectable_gap() {
        // Allow exactly the set_meta write, then fail: the merge bead is never created.
        let inner = store_with_task().await;
        let now = Timestamp::from_unix_seconds(0);
        let w = AgentId::try_new("w1").unwrap();
        apply_event(
            &inner,
            &id("fac-1"),
            Event::Claim {
                holder: w.clone(),
                now,
                ttl: Duration::from_seconds(60),
            },
        )
        .await
        .unwrap();
        apply_event(
            &inner,
            &id("fac-1"),
            Event::Submit {
                holder: w,
                branch: domain::BranchName::try_new("task/fac-1").unwrap(),
                head: sha('b'),
                now,
                tokens: domain::Tokens::new(1),
            },
        )
        .await
        .unwrap();
        let store = crate::testing::FlakyStore::new(inner, 1);
        let err = apply_event(&store, &id("fac-1"), Event::VerifyPassed)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            TransitionError::Store(StoreError::Unavailable { .. })
        ));
        // Persisted first: the task says mergeable …
        assert!(matches!(
            load_task(&store, &id("fac-1")).await.unwrap().state,
            TaskState::Mergeable { .. }
        ));
        // … and the missing merge bead is the visible symptom the Steward repairs.
        assert!(store.list_active(BeadKind::Merge).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn illegal_event_does_not_persist() {
        let store = store_with_task().await;
        let err = apply_event(&store, &id("fac-1"), Event::VerifyPassed)
            .await
            .unwrap_err();
        assert!(matches!(err, TransitionError::Illegal(_)));
        assert_eq!(
            load_task(&store, &id("fac-1")).await.unwrap().state,
            TaskState::Open
        );
    }

    #[tokio::test]
    async fn non_task_is_rejected() {
        let store = FakeStore::default();
        store.seed_plain(id("fac-9"), "not ours").await;
        assert!(matches!(
            load_task(&store, &id("fac-9")).await,
            Err(TransitionError::NotATask(_))
        ));
    }
}
