//! Remote-control workflows: authorize → act on the rig → audit. Each function is one A2A
//! operation over one rig. The HTTP layer only decodes the request and encodes the result.

use domain::{BeadId, BeadKind, Forbidden, Principal, RigBudgetExceeded, RigSpend, Scope, Tokens};

use super::a2a::{Task, epic_task, inbox_task, request_task};

#[path = "service_lists.rs"]
mod lists;
use super::attention::incident_task_id;
use super::{Rig, SubmitError, TailError};
use crate::bead::Bead;
use crate::console::resolve;
use crate::events::{EventKind, FactoryEvent};
use crate::ports::{Clock, StoreError};
pub use lists::{list_history, list_tasks_with_vanished};

/// Why a remote operation was refused or failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    #[error(transparent)]
    Forbidden(#[from] Forbidden),
    #[error("task `{id}` not found")]
    TaskNotFound { id: String },
    #[error("task `{id}` is terminal; send a new message without taskId")]
    Terminal { id: String },
    #[error("message has no text")]
    EmptyMessage,
    #[error(transparent)]
    Budget(#[from] RigBudgetExceeded),
    #[error(transparent)]
    Submit(#[from] SubmitError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Tail(#[from] TailError),
}

fn audit(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    action: &str,
    bead: Option<BeadId>,
    detail: String,
) {
    rig.sink.record(&FactoryEvent {
        at: clock.now(),
        actor: format!("remote:{}", who.client),
        bead,
        kind: EventKind::Remote {
            action: action.to_owned(),
            detail,
        },
    });
}

fn authorize(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    scope: Scope,
    action: &str,
) -> Result<(), RemoteError> {
    domain::require(who, &rig.name, scope).map_err(|e| {
        audit(rig, clock, who, "refused", None, format!("{action}: {e}"));
        RemoteError::Forbidden(e)
    })
}

/// Tokens consumed by every task under every epic, closed or not.
///
/// # Errors
/// Ledger failures.
pub async fn spend(rig: &Rig) -> Result<RigSpend, StoreError> {
    let mut tokens = Tokens::new(0);
    for epic in rig.store.list_active(BeadKind::Epic).await? {
        for child in rig.store.children(&epic.id).await? {
            if let Some(meta) = child.meta {
                tokens = tokens + meta.usage.tokens;
            }
        }
    }
    Ok(RigSpend {
        tokens,
        usd: domain::MicroUsd::new(0),
    })
}

/// `ListTasks`: every active epic plus every open inbox item.
///
/// # Errors
/// `Forbidden` without `watch`; ledger failures.
pub async fn list_tasks(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
) -> Result<Vec<Task>, RemoteError> {
    authorize(rig, clock, who, Scope::Watch, "ListTasks")?;
    let now = clock.now().to_string();
    let mut out = Vec::new();
    for epic in rig.store.list_active(BeadKind::Epic).await? {
        let children = rig.store.children(&epic.id).await?;
        out.push(epic_task(&epic, &children, &now));
    }
    for item in crate::console::inbox(rig.store.as_ref()).await? {
        let task = incident_task_bead(rig, &item).await?;
        out.push(inbox_task(&item, task.as_ref(), &now));
    }
    for req in rig.store.list_active(BeadKind::PlanRequest).await? {
        out.push(request_task(&req, &now));
    }
    // Requests waiting on other rigs are deferred, not active; they still belong on the page.
    for req in rig.store.list_deferred(BeadKind::PlanRequest).await? {
        out.push(request_task(&req, &now));
    }
    Ok(out)
}

/// `GetTask`: an epic or an inbox item by id.
///
/// # Errors
/// `Forbidden` without `watch`; `TaskNotFound` for ids that are not epics/inbox items.
pub async fn get_task(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    id: &str,
) -> Result<Task, RemoteError> {
    authorize(rig, clock, who, Scope::Watch, "GetTask")?;
    let bead_id =
        BeadId::try_new(id).map_err(|_| RemoteError::TaskNotFound { id: id.to_owned() })?;
    let bead = match rig.store.show(&bead_id).await {
        Ok(b) => b,
        Err(StoreError::NotFound { .. }) => {
            return Err(RemoteError::TaskNotFound { id: id.to_owned() });
        }
        Err(e) => return Err(e.into()),
    };
    let now = clock.now().to_string();
    match bead.kind {
        Some(BeadKind::Epic) => {
            let children = rig.store.children(&bead.id).await?;
            Ok(epic_task(&bead, &children, &now))
        }
        Some(BeadKind::Incident | BeadKind::Question) => {
            let task = incident_task_bead(rig, &bead).await?;
            Ok(inbox_task(&bead, task.as_ref(), &now))
        }
        Some(BeadKind::PlanRequest) => Ok(request_task(&bead, &now)),
        Some(
            BeadKind::Task
            | BeadKind::Verify
            | BeadKind::Merge
            | BeadKind::Reference
            | BeadKind::Contract,
        )
        | None => Err(RemoteError::TaskNotFound { id: id.to_owned() }),
    }
}

/// The task an incident bead points at, when it still exists.
async fn incident_task_bead(rig: &Rig, item: &Bead) -> Result<Option<Bead>, RemoteError> {
    let Some(id) = incident_task_id(item) else {
        return Ok(None);
    };
    match rig.store.show(&id).await {
        Ok(b) => Ok(Some(b)),
        Err(StoreError::NotFound { .. }) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// What `SendMessage` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    /// A plan was accepted; the new epic.
    Planned(Task),
    /// An inbox item was resolved; `reopened` is the task put back to work, if any.
    Resolved {
        task: Task,
        reopened: Option<BeadId>,
    },
}

/// `SendMessage` with `returnImmediately`: queue the plan and answer with the request as a
/// `SUBMITTED` task the client can watch (`GetTask` / the event stream). The rig's planner
/// closes the request with the epic id; `list_tasks_with_vanished` surfaces the outcome.
///
/// # Errors
/// Scope, budget, ledger failures; `EmptyMessage`.
pub async fn enqueue_plan(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    text: &str,
) -> Result<Task, RemoteError> {
    enqueue_plan_with_needs(rig, clock, who, text, Vec::new()).await
}

/// `enqueue_plan` for a plan that waits on epics of other rigs (`fac-e8o`): created deferred.
///
/// # Errors
/// As `enqueue_plan`.
pub async fn enqueue_plan_with_needs(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    text: &str,
    needs: Vec<domain::CrossRigNeed>,
) -> Result<Task, RemoteError> {
    if text.trim().is_empty() {
        return Err(RemoteError::EmptyMessage);
    }
    authorize(rig, clock, who, Scope::Plan, "SendMessage")?;
    rig.budget
        .admit(spend(rig).await?)
        .inspect_err(|e| audit(rig, clock, who, "plan-refused", None, e.to_string()))?;
    let request = rig
        .store
        .create(crate::plan_queue::plan_request_with_needs(
            text,
            who.client.as_ref(),
            needs,
        ))
        .await?;
    audit(
        rig,
        clock,
        who,
        "plan-queued",
        Some(request.clone()),
        first_line(text),
    );
    let bead = rig.store.show(&request).await?;
    Ok(request_task(&bead, &clock.now().to_string()))
}

/// `SendMessage`: without `taskId` the text is a plan (needs `plan`, subject to the rig
/// budget); with `taskId` of an inbox item the text is the resolution note (needs `resolve`).
///
/// # Errors
/// Scope, budget, ledger, and submission failures; `Terminal` for a closed inbox item.
pub async fn send_message(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    task_id: Option<&str>,
    text: &str,
) -> Result<Sent, RemoteError> {
    if text.trim().is_empty() {
        return Err(RemoteError::EmptyMessage);
    }
    match task_id {
        None => {
            authorize(rig, clock, who, Scope::Plan, "SendMessage")?;
            rig.budget
                .admit(spend(rig).await?)
                .inspect_err(|e| audit(rig, clock, who, "plan-refused", None, e.to_string()))?;
            let epic = rig.planner.submit(text).await?;
            audit(
                rig,
                clock,
                who,
                "plan",
                Some(epic.clone()),
                first_line(text),
            );
            get_task(rig, clock, who, epic.as_ref())
                .await
                .map(Sent::Planned)
        }
        Some(id) => {
            authorize(rig, clock, who, Scope::Resolve, "SendMessage")?;
            let current = get_task(rig, clock, who, id).await?;
            if current.status.state.is_terminal() {
                return Err(RemoteError::Terminal { id: id.to_owned() });
            }
            let bead_id =
                BeadId::try_new(id).map_err(|_| RemoteError::TaskNotFound { id: id.to_owned() })?;
            let reopened = resolve(rig.store.as_ref(), &bead_id, text).await?;
            audit(
                rig,
                clock,
                who,
                "resolve",
                Some(bead_id.clone()),
                first_line(text),
            );
            let task = get_task(rig, clock, who, id).await?;
            Ok(Sent::Resolved { task, reopened })
        }
    }
}

/// Note prefix the Worker looks for to continue from an existing branch.
pub const RESUME_MARKER: &str = "resume-from: ";

#[path = "service_options.rs"]
mod options;
use options::first_line;
pub use options::{Overview, apply_option, cancel_task, events_after, overview};
