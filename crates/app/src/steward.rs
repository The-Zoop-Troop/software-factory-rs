//! The Steward's sweep (ARCHITECTURE.md §4.5): lease reaping, budget enforcement, epic
//! closing. Deterministic, no LLM. Each step is isolated so one bad bead cannot stall
//! the rest of the sweep; failures are logged as events and counted.

use domain::{BeadId, BeadKind, Event, TaskState, Timestamp};

use crate::bead::{Bead, BeadStatus};
use crate::events::{EventKind, FactoryEvent};
use crate::ports::{BeadStore, Clock, EventSink};
use crate::transition::{TransitionError, apply_event};

/// What a sweep did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepReport {
    pub reaped: usize,
    pub escalated: usize,
    pub epics_closed: usize,
    pub errors: usize,
}

/// Failure to even list the beads; per-bead failures are recorded in the report instead.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StewardError {
    #[error(transparent)]
    Store(#[from] crate::ports::StoreError),
}

/// Run one sweep.
///
/// # Errors
/// Only when the ledger cannot be listed at all.
#[tracing::instrument(skip_all, err)]
pub async fn sweep(
    store: &dyn BeadStore,
    clock: &dyn Clock,
    log: &dyn EventSink,
    actor: &str,
) -> Result<SweepReport, StewardError> {
    let now = clock.now();
    let mut report = SweepReport::default();

    for bead in store.list_active(BeadKind::Task).await? {
        match sweep_task(store, &bead, now).await {
            Ok(Some(Outcome::Reaped)) => {
                report.reaped += 1;
                log.record(&event(
                    now,
                    actor,
                    Some(bead.id.clone()),
                    EventKind::LeaseReaped,
                ));
            }
            Ok(Some(Outcome::Escalated(detail))) => {
                report.escalated += 1;
                log.record(&event(
                    now,
                    actor,
                    Some(bead.id.clone()),
                    EventKind::Escalated { detail },
                ));
            }
            Ok(None) => {}
            Err(e) => {
                report.errors += 1;
                log.record(&event(
                    now,
                    actor,
                    Some(bead.id.clone()),
                    EventKind::Error {
                        detail: e.to_string(),
                    },
                ));
            }
        }
    }

    for epic in store.list_active(BeadKind::Epic).await? {
        match sweep_epic(store, &epic).await {
            Ok(Some(children)) => {
                report.epics_closed += 1;
                log.record(&event(
                    now,
                    actor,
                    Some(epic.id.clone()),
                    EventKind::EpicClosed { children },
                ));
            }
            Ok(None) => {}
            Err(e) => {
                report.errors += 1;
                log.record(&event(
                    now,
                    actor,
                    Some(epic.id.clone()),
                    EventKind::Error {
                        detail: e.to_string(),
                    },
                ));
            }
        }
    }

    log.record(&event(
        now,
        actor,
        None,
        EventKind::SweepDone {
            reaped: report.reaped,
            escalated: report.escalated,
            epics_closed: report.epics_closed,
        },
    ));
    Ok(report)
}

enum Outcome {
    Reaped,
    Escalated(String),
}

/// Decide, purely, what the Steward should do about one task right now.
fn decide(bead: &Bead, now: Timestamp) -> Option<Event> {
    let meta = bead.meta.as_ref()?;
    match &meta.state {
        TaskState::Leased { lease } => {
            if lease.is_expired(now) {
                return Some(Event::LeaseExpired { now });
            }
            // Project wall-clock to now so a runaway session is caught mid-lease, not at submit.
            let projected = meta.usage.add_wall_clock(now.since(lease.claimed_at));
            meta.budget
                .check(projected)
                .err()
                .map(|exceeded| Event::Escalate {
                    detail: exceeded.to_string(),
                })
        }
        TaskState::Open
        | TaskState::InVerify { .. }
        | TaskState::Mergeable { .. }
        | TaskState::Closed { .. }
        | TaskState::Incident { .. } => None,
    }
}

async fn sweep_task(
    store: &dyn BeadStore,
    bead: &Bead,
    now: Timestamp,
) -> Result<Option<Outcome>, TransitionError> {
    let Some(ev) = decide(bead, now) else {
        return Ok(None);
    };
    let outcome = match &ev {
        Event::LeaseExpired { .. } => Outcome::Reaped,
        Event::Escalate { detail } => Outcome::Escalated(detail.clone()),
        Event::Claim { .. }
        | Event::Heartbeat { .. }
        | Event::Submit { .. }
        | Event::VerifyPassed
        | Event::VerifyFailed { .. }
        | Event::Merged { .. }
        | Event::MergeFailed { .. } => return Ok(None),
    };
    apply_event(store, &bead.id, ev).await?;
    Ok(Some(outcome))
}

/// Close an epic when it has children and every one is closed. Returns the child count.
async fn sweep_epic(store: &dyn BeadStore, epic: &Bead) -> Result<Option<usize>, StewardError> {
    let children: Vec<_> = store
        .children(&epic.id)
        .await?
        .into_iter()
        // Reference beads are context; they must never hold an epic open.
        .filter(|c| c.kind != Some(BeadKind::Reference))
        .collect();
    if children.is_empty() || children.iter().any(|c| c.status != BeadStatus::Closed) {
        return Ok(None);
    }
    store
        .close(&epic.id, "all children closed (steward)")
        .await?;
    Ok(Some(children.len()))
}

fn event(at: Timestamp, actor: &str, bead: Option<BeadId>, kind: EventKind) -> FactoryEvent {
    FactoryEvent {
        at,
        actor: actor.to_owned(),
        bead,
        kind,
    }
}

#[cfg(test)]
mod tests {
    use domain::{AgentId, Budget, Duration, FactoryMeta, Lease, Sha, Usage};

    use super::*;
    use crate::testing::{FakeStore, FixedClock, MemorySink};
    use crate::transition::load_task;

    fn id(s: &str) -> BeadId {
        BeadId::try_new(s).unwrap()
    }
    fn sha(c: char) -> Sha {
        Sha::try_new(core::iter::repeat_n(c, 40).collect::<String>()).unwrap()
    }
    fn leased(claimed: i64, ttl: u64, budget: Budget) -> FactoryMeta {
        FactoryMeta {
            verify_bead: id("fac-v"),
            base: sha('a'),
            budget,
            usage: Usage::default(),
            lease_expiries: 0,
            state: TaskState::Leased {
                lease: Lease::grant(
                    AgentId::try_new("w1").unwrap(),
                    Timestamp::from_unix_seconds(claimed),
                    Duration::from_seconds(ttl),
                ),
            },
        }
    }

    #[tokio::test]
    async fn reaps_expired_lease_and_leaves_live_one() {
        let store = FakeStore::default();
        store
            .seed_task(id("fac-dead"), leased(0, 10, Budget::default()))
            .await;
        store
            .seed_task(id("fac-live"), leased(95, 10, Budget::default()))
            .await;
        let log = MemorySink::default();
        let report = sweep(
            &store,
            &FixedClock(Timestamp::from_unix_seconds(100)),
            &log,
            "steward",
        )
        .await
        .unwrap();
        assert_eq!(
            report,
            SweepReport {
                reaped: 1,
                ..SweepReport::default()
            }
        );
        assert_eq!(
            load_task(&store, &id("fac-dead")).await.unwrap().state,
            TaskState::Open
        );
        assert!(matches!(
            load_task(&store, &id("fac-live")).await.unwrap().state,
            TaskState::Leased { .. }
        ));
        let events = log.events().await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e.kind, EventKind::LeaseReaped))
        );
        assert!(matches!(
            events.last().unwrap().kind,
            EventKind::SweepDone { reaped: 1, .. }
        ));
    }

    #[tokio::test]
    async fn escalates_wall_clock_overrun_mid_lease() {
        let store = FakeStore::default();
        let tight = Budget {
            wall_clock: Duration::from_seconds(30),
            ..Budget::default()
        };
        store
            .seed_task(id("fac-slow"), leased(0, 1000, tight))
            .await;
        let log = MemorySink::default();
        let report = sweep(
            &store,
            &FixedClock(Timestamp::from_unix_seconds(100)),
            &log,
            "steward",
        )
        .await
        .unwrap();
        assert_eq!(report.escalated, 1);
        assert!(matches!(
            load_task(&store, &id("fac-slow")).await.unwrap().state,
            TaskState::Incident { .. }
        ));
        assert_eq!(
            store.list_active(BeadKind::Incident).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn closes_epic_only_when_all_children_closed() {
        let store = FakeStore::default();
        store
            .seed_epic(id("fac-e"), &[("fac-e.1", true), ("fac-e.2", false)])
            .await;
        store
            .seed_epic(id("fac-f"), &[("fac-f.1", true), ("fac-f.2", true)])
            .await;
        store.seed_epic(id("fac-g"), &[]).await;
        let log = MemorySink::default();
        let report = sweep(
            &store,
            &FixedClock(Timestamp::from_unix_seconds(0)),
            &log,
            "steward",
        )
        .await
        .unwrap();
        assert_eq!(report.epics_closed, 1);
        assert_eq!(
            store.show(&id("fac-f")).await.unwrap().status,
            BeadStatus::Closed
        );
        assert_eq!(
            store.show(&id("fac-e")).await.unwrap().status,
            BeadStatus::Open
        );
        assert_eq!(
            store.show(&id("fac-g")).await.unwrap().status,
            BeadStatus::Open
        );
    }

    #[test]
    fn decide_ignores_non_leased() {
        let bead = Bead {
            id: id("fac-x"),
            title: String::new(),
            description: String::new(),
            acceptance: None,
            notes: None,
            status: BeadStatus::Open,
            labels: vec![],
            parent: None,
            kind: Some(BeadKind::Task),
            meta: Some(FactoryMeta {
                state: TaskState::Open,
                ..leased(0, 1, Budget::default())
            }),
            verify: None,
            merge: None,
        };
        assert_eq!(decide(&bead, Timestamp::from_unix_seconds(10)), None);
    }
}
