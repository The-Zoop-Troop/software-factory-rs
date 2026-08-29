//! Remote-control workflows: authorize → act on the rig → audit. Each function is one A2A
//! operation over one rig. The HTTP layer only decodes the request and encodes the result.

use domain::{BeadId, BeadKind, Forbidden, Principal, RigBudgetExceeded, RigSpend, Scope, Tokens};

use super::a2a::{A2aState, CANCELED_LABEL, Task, epic_task, inbox_task};
use super::{Rig, SubmitError, TailError};
use crate::bead::BeadStatus;
use crate::console::resolve;
use crate::events::{EventKind, FactoryEvent};
use crate::ports::{Clock, StoreError};

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
        out.push(inbox_task(&item, &now));
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
        Some(BeadKind::Incident | BeadKind::Question) => Ok(inbox_task(&bead, &now)),
        Some(
            BeadKind::Task
            | BeadKind::Verify
            | BeadKind::Merge
            | BeadKind::Reference
            | BeadKind::PlanRequest,
        )
        | None => Err(RemoteError::TaskNotFound { id: id.to_owned() }),
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

/// `CancelTask`: close every open task under the epic and the epic itself, labelled so it
/// reads back as `CANCELED`. Workers holding a lease lose it at their next persist.
///
/// # Errors
/// `Forbidden` without `plan`; `Terminal` if already closed; ledger failures.
pub async fn cancel_task(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    id: &str,
) -> Result<Task, RemoteError> {
    authorize(rig, clock, who, Scope::Plan, "CancelTask")?;
    let current = get_task(rig, clock, who, id).await?;
    if current.status.state.is_terminal() {
        return Err(RemoteError::Terminal { id: id.to_owned() });
    }
    let epic_id =
        BeadId::try_new(id).map_err(|_| RemoteError::TaskNotFound { id: id.to_owned() })?;
    let reason = format!("canceled by {}", who.client);
    let mut closed = 0usize;
    for child in rig.store.children(&epic_id).await? {
        if child.status != BeadStatus::Closed {
            rig.store.close(&child.id, &reason).await?;
            closed += 1;
        }
    }
    rig.store.label(&epic_id, CANCELED_LABEL).await?;
    rig.store.close(&epic_id, &reason).await?;
    audit(
        rig,
        clock,
        who,
        "cancel",
        Some(epic_id),
        format!("closed {closed} task(s)"),
    );
    let mut task = get_task(rig, clock, who, id).await?;
    task.status.state = A2aState::Canceled;
    Ok(task)
}

/// `SubscribeToTask` backing: event records after `cursor`, filtered to the task's context
/// (the epic id, or any bead under it). `None` context streams everything.
///
/// # Errors
/// `Forbidden` without `watch`; log read failures.
pub async fn events_after(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    cursor: u64,
    context: Option<&str>,
) -> Result<(Vec<super::EventRecord>, u64), RemoteError> {
    authorize(rig, clock, who, Scope::Watch, "SubscribeToTask")?;
    let (records, next) = rig.events.read_from(cursor).await?;
    let Some(ctx) = context else {
        return Ok((records, next));
    };
    let members = context_members(rig, ctx).await?;
    let kept = records
        .into_iter()
        .filter(|r| r.bead.as_ref().is_some_and(|b| members.contains(b)))
        .collect();
    Ok((kept, next))
}

async fn context_members(rig: &Rig, ctx: &str) -> Result<Vec<BeadId>, RemoteError> {
    let id = BeadId::try_new(ctx).map_err(|_| RemoteError::TaskNotFound { id: ctx.to_owned() })?;
    let children = match rig.store.children(&id).await {
        Ok(c) => c,
        Err(StoreError::NotFound { .. }) => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    let mut members: Vec<BeadId> = children.into_iter().map(|c| c.id).collect();
    members.push(id);
    Ok(members)
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect()
}
