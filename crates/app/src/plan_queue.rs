//! The plan queue: remote clients cannot run the Planner (it needs the rig's harness
//! credential), so they leave a `plan_request` bead in the ledger and the rig's Planner
//! service pulls it, plans, and closes it with the epic id. Pull-based, like everything else.

use std::path::Path;

use domain::{BeadId, BeadKind, BranchName, PlanDefaults, Priority, Title};

use crate::bead::{Bead, BeadStatus, NewBead};
use crate::events::{EventKind, FactoryEvent};
use crate::planner::{PlanReport, PlannerError, plan};
use crate::ports::{BeadStore, Clock, EventSink, Harness, Repo, StoreError};
use crate::remote::SubmitError;

const EPIC_PREFIX: &str = "epic ";
const FAILED_PREFIX: &str = "failed: ";

/// The bead a remote plan submission becomes. `client` is recorded for the audit trail.
#[must_use]
pub fn plan_request(text: &str, client: &str) -> NewBead {
    plan_request_with_needs(text, client, Vec::new())
}

/// A plan request that waits for epics on other rigs: created deferred, carrying `fac_needs`;
/// the console's dependency sweep injects their contracts and un-defers it.
#[must_use]
pub fn plan_request_with_needs(
    text: &str,
    client: &str,
    needs: Vec<domain::CrossRigNeed>,
) -> NewBead {
    let deferred = !needs.is_empty();
    NewBead {
        title: Title::derived(text),
        description: text.to_owned(),
        kind: BeadKind::PlanRequest,
        priority: Priority::HIGH,
        parent: None,
        needs: Vec::new(),
        acceptance: Some(format!(
            "submitted by {client}; closed by the Planner with `epic <id>`"
        )),
        meta: deferred.then_some(domain::BeadMeta::Needs(needs)),
        deferred,
    }
}

/// Read a request's outcome back: `None` while open; `Ok(epic)` or `Rejected` once closed.
#[must_use]
pub fn plan_outcome(bead: &Bead) -> Option<Result<BeadId, SubmitError>> {
    if bead.status != BeadStatus::Closed {
        return None;
    }
    let notes = bead.notes.as_deref().unwrap_or_default();
    let epic = notes
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix(EPIC_PREFIX))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|id| BeadId::try_new(id).ok());
    Some(match epic {
        Some(id) => Ok(id),
        None => Err(SubmitError::Rejected {
            detail: notes
                .lines()
                .rev()
                .find_map(|l| l.strip_prefix(FAILED_PREFIX))
                .unwrap_or("request closed without an epic")
                .to_owned(),
        }),
    })
}

/// Where a sweep reports what it is doing.
#[derive(Clone, Copy)]
pub struct Progress<'a> {
    pub sink: &'a dyn EventSink,
    pub clock: &'a dyn Clock,
}

impl core::fmt::Debug for Progress<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Progress")
    }
}

/// What one queue sweep did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedOutcome {
    pub request: BeadId,
    pub result: Result<PlanReport, PlannerError>,
}

/// Take the oldest ready plan request, plan it, and close it with the result. `Ok(None)`
/// when the queue is empty. A planner failure closes the request as failed so the
/// submitter gets an answer; the text stays in the bead for a retry.
///
/// # Errors
/// Ledger failures while reading or closing the request.
#[tracing::instrument(skip_all, err)]
pub async fn plan_queued_once(
    store: &dyn BeadStore,
    harness: &dyn Harness,
    repo: &dyn Repo,
    repo_path: &Path,
    main: &BranchName,
    defaults: PlanDefaults,
    progress: Progress<'_>,
) -> Result<Option<QueuedOutcome>, StoreError> {
    let Progress { sink, clock } = progress;
    let Some(request) = store.ready(BeadKind::PlanRequest).await?.into_iter().next() else {
        return Ok(None);
    };
    let record = |action: &str, detail: String| {
        sink.record(&FactoryEvent {
            at: clock.now(),
            actor: "planner".to_owned(),
            bead: Some(request.id.clone()),
            kind: EventKind::Remote {
                action: action.to_owned(),
                detail,
            },
        });
    };
    store
        .note(
            &request.id,
            "planning: the rig's planner is reading the repository",
        )
        .await?;
    record("plan_started", request.title.clone());
    let result = plan(
        store,
        harness,
        repo,
        repo_path,
        main,
        &request.description,
        defaults,
    )
    .await;
    let line = match &result {
        Ok(report) => format!("{EPIC_PREFIX}{}", report.epic),
        Err(e) => format!("{FAILED_PREFIX}{e}"),
    };
    match &result {
        Ok(report) => {
            for (task, needs) in &report.edges {
                sink.record(&FactoryEvent {
                    at: clock.now(),
                    actor: "planner".to_owned(),
                    bead: Some(task.clone()),
                    kind: EventKind::TaskPlanned {
                        epic: report.epic.clone(),
                        needs: needs.clone(),
                    },
                });
            }
            record(
                "planned",
                format!("{} ({} tasks)", report.epic, report.tasks.len()),
            );
        }
        Err(e) => record("plan_failed", e.to_string()),
    }
    store.note(&request.id, &line).await?;
    store.close(&request.id, &line).await?;
    Ok(Some(QueuedOutcome {
        request: request.id,
        result,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeHarness, FakeRepo, FakeStore};

    fn id(s: &str) -> BeadId {
        BeadId::try_new(s).expect("id")
    }

    #[test]
    fn request_shape_and_outcome_parsing() {
        let new = plan_request("Build the thing\n\nmore detail", "phone");
        assert_eq!(new.kind, BeadKind::PlanRequest);
        assert_eq!(new.title.as_ref(), "Build the thing");
        assert!(
            new.acceptance
                .as_deref()
                .is_some_and(|a| a.contains("phone"))
        );

        let mut bead = crate::testing::plain_bead(id("pr-1"), Some(BeadKind::PlanRequest));
        assert_eq!(plan_outcome(&bead), None);
        bead.status = BeadStatus::Closed;
        assert_eq!(
            plan_outcome(&bead),
            Some(Err(SubmitError::Rejected {
                detail: "request closed without an epic".into()
            }))
        );
        bead.notes = Some("failed: model returned no structured output".into());
        assert_eq!(
            plan_outcome(&bead),
            Some(Err(SubmitError::Rejected {
                detail: "model returned no structured output".into()
            }))
        );
        bead.notes = Some("failed: first try\nepic fac-abc".into());
        assert_eq!(plan_outcome(&bead), Some(Ok(id("fac-abc"))));
    }

    #[tokio::test]
    async fn empty_queue_is_a_noop() {
        let store = FakeStore::default();
        let harness = FakeHarness::structured(serde_json::json!({}));
        let repo = FakeRepo::default();
        let main = BranchName::try_new("main").expect("branch");
        let sink = crate::testing::MemorySink::default();
        let clock = crate::testing::FixedClock(domain::Timestamp::from_unix_seconds(0));
        let out = plan_queued_once(
            &store,
            &harness,
            &repo,
            Path::new("."),
            &main,
            PlanDefaults::default(),
            Progress {
                sink: &sink,
                clock: &clock,
            },
        )
        .await;
        assert_eq!(out, Ok(None));
    }

    #[tokio::test]
    async fn a_failed_plan_closes_the_request_as_failed() {
        let store = FakeStore::default();
        let req = store
            .create(plan_request("do it", "cli"))
            .await
            .expect("created");
        let harness = FakeHarness::structured(serde_json::json!({"not": "a plan"}));
        let repo = FakeRepo::default();
        let main = BranchName::try_new("main").expect("branch");
        let sink = crate::testing::MemorySink::default();
        let clock = crate::testing::FixedClock(domain::Timestamp::from_unix_seconds(0));
        let out = plan_queued_once(
            &store,
            &harness,
            &repo,
            Path::new("."),
            &main,
            PlanDefaults::default(),
            Progress {
                sink: &sink,
                clock: &clock,
            },
        )
        .await
        .expect("ok")
        .expect("one request");
        assert_eq!(out.request, req);
        assert!(out.result.is_err());
        let bead = store.show(&req).await.expect("shown");
        assert!(matches!(
            plan_outcome(&bead),
            Some(Err(SubmitError::Rejected { .. }))
        ));
    }
}
