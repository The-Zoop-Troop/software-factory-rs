//! Remote-control workflows: authorize → act on the rig → audit. Each function is one A2A
//! operation over one rig. The HTTP layer only decodes the request and encodes the result.

use domain::{BeadId, BeadKind, Forbidden, Principal, RigBudgetExceeded, RigSpend, Scope, Tokens};

use super::a2a::{A2aState, CANCELED_LABEL, Task, epic_task, inbox_task, request_task};
use super::attention::{AttentionOption, incident_task_id};
use super::{Rig, SubmitError, TailError};
use crate::bead::{Bead, BeadStatus};
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
        let task = incident_task_bead(rig, &item).await?;
        out.push(inbox_task(&item, task.as_ref(), &now));
    }
    for req in rig.store.list_active(BeadKind::PlanRequest).await? {
        out.push(request_task(&req, &now));
    }
    Ok(out)
}

/// Closed epics — the rig's history — as the ledger lists them (sort by the `epic_closed`
/// event when the log is at hand).
///
/// # Errors
/// `Unauthorized` without `watch`; store failures.
pub async fn list_history(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
) -> Result<Vec<Task>, RemoteError> {
    authorize(rig, clock, who, Scope::Watch, "ListTasks")?;
    let now = clock.now().to_string();
    let mut out = Vec::new();
    for epic in rig.store.list_closed(BeadKind::Epic).await? {
        let children = rig.store.children(&epic.id).await?;
        out.push(epic_task(&epic, &children, &now));
    }
    Ok(out)
}

/// `ListTasks` plus the tasks in `seen` that dropped out of the listing (closed epics),
/// each fetched once so a watcher observes its terminal state.
///
/// # Errors
/// As `list_tasks`.
pub async fn list_tasks_with_vanished(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    seen: &std::collections::BTreeMap<String, A2aState>,
) -> Result<Vec<Task>, RemoteError> {
    let mut tasks = list_tasks(rig, clock, who).await?;
    let listed: std::collections::BTreeSet<String> = tasks.iter().map(|t| t.id.clone()).collect();
    for id in seen
        .iter()
        .filter(|(id, state)| !listed.contains(*id) && !state.is_terminal())
        .map(|(id, _)| id)
    {
        if let Ok(t) = get_task(rig, clock, who, id).await {
            tasks.push(t);
        }
    }
    Ok(tasks)
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
    if text.trim().is_empty() {
        return Err(RemoteError::EmptyMessage);
    }
    authorize(rig, clock, who, Scope::Plan, "SendMessage")?;
    rig.budget
        .admit(spend(rig).await?)
        .inspect_err(|e| audit(rig, clock, who, "plan-refused", None, e.to_string()))?;
    let request = rig
        .store
        .create(crate::plan_queue::plan_request(text, who.client.as_ref()))
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

/// An attention option, applied. `note` is required by `RetryWithGuidance`, `Replan`, `Answer`.
///
/// # Errors
/// Scope (`resolve`, or `plan` for the epic-level options), `TaskNotFound`, `EmptyMessage`
/// when a required note is missing, ledger failures.
pub async fn apply_option(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    item_id: &str,
    option: AttentionOption,
    note: &str,
) -> Result<Sent, RemoteError> {
    let needs_note = matches!(
        option,
        AttentionOption::RetryWithGuidance | AttentionOption::Replan | AttentionOption::Answer
    );
    // `ResumeBranch` reopens like a retry but tells the next session to start from the task's
    // own branch (the Worker reads the marker from the notes).
    if needs_note && note.trim().is_empty() {
        return Err(RemoteError::EmptyMessage);
    }
    let bead_id = BeadId::try_new(item_id).map_err(|_| RemoteError::TaskNotFound {
        id: item_id.to_owned(),
    })?;
    let item = match rig.store.show(&bead_id).await {
        Ok(b) => b,
        Err(StoreError::NotFound { .. }) => {
            return Err(RemoteError::TaskNotFound {
                id: item_id.to_owned(),
            });
        }
        Err(e) => return Err(e.into()),
    };
    let task = incident_task_bead(rig, &item).await?;
    let epic = task
        .as_ref()
        .and_then(|t| t.parent.clone())
        .or_else(|| item.parent.clone());
    let text = if note.trim().is_empty() {
        format!("{} by operator", option.as_str())
    } else {
        note.to_owned()
    };
    match option {
        AttentionOption::RetryFresh | AttentionOption::Answer => {
            send_message(rig, clock, who, Some(item_id), &text).await
        }
        AttentionOption::ResumeBranch => {
            authorize(rig, clock, who, Scope::Resolve, "SendMessage")?;
            if let Some(t) = &task {
                let branch =
                    domain::BranchName::for_task(&t.id).map_err(|_| RemoteError::TaskNotFound {
                        id: t.id.to_string(),
                    })?;
                rig.store
                    .note(&t.id, &format!("{RESUME_MARKER}{branch}"))
                    .await?;
            }
            send_message(rig, clock, who, Some(item_id), &text).await
        }
        AttentionOption::RetryWithGuidance => {
            authorize(rig, clock, who, Scope::Resolve, "SendMessage")?;
            if let Some(t) = &task {
                rig.store
                    .note(&t.id, &format!("guidance: {}", note.trim()))
                    .await?;
            }
            send_message(rig, clock, who, Some(item_id), &text).await
        }
        AttentionOption::StopEpic => {
            let Some(epic) = epic else {
                return Err(RemoteError::TaskNotFound {
                    id: format!("epic of {item_id}"),
                });
            };
            cancel_task(rig, clock, who, epic.as_ref())
                .await
                .map(|task| Sent::Resolved {
                    task,
                    reopened: None,
                })
        }
        AttentionOption::Replan => {
            let Some(epic_id) = epic else {
                return Err(RemoteError::TaskNotFound {
                    id: format!("epic of {item_id}"),
                });
            };
            let epic_bead = rig.store.show(&epic_id).await?;
            cancel_task(rig, clock, who, epic_id.as_ref()).await?;
            let plan_text = format!(
                "{}\n\n{}\n\nOperator guidance after a failed attempt: {}",
                epic_bead.title,
                epic_bead.description,
                note.trim()
            );
            send_message(rig, clock, who, None, &plan_text).await
        }
    }
}

/// Per-rig counts for the landing page.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct Overview {
    pub rig: String,
    pub epics: usize,
    pub working: usize,
    pub attention: usize,
    pub done: usize,
}

/// Counts for one rig from its task list (a watcher's view).
///
/// # Errors
/// As `list_tasks`.
pub async fn overview(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
) -> Result<Overview, RemoteError> {
    let tasks = list_tasks(rig, clock, who).await?;
    let is_epic = |t: &Task| {
        t.metadata
            .get("factory")
            .and_then(|f| f.get("kind"))
            .and_then(serde_json::Value::as_str)
            == Some("epic")
    };
    Ok(Overview {
        rig: rig.name.to_string(),
        epics: tasks.iter().filter(|t| is_epic(t)).count(),
        working: tasks
            .iter()
            .filter(|t| is_epic(t) && t.status.state == A2aState::Working)
            .count(),
        attention: tasks
            .iter()
            .filter(|t| t.status.state == A2aState::InputRequired)
            .count(),
        done: tasks
            .iter()
            .filter(|t| is_epic(t) && t.status.state.is_terminal())
            .count(),
    })
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
    let closed = close_all_children(rig, &epic_id, &reason).await?;
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

/// Close every open child, dependents before their blockers (the ledger refuses to close a
/// bead whose blockers are open), then any incident filed on one of them. Returns the count.
async fn close_all_children(rig: &Rig, epic: &BeadId, reason: &str) -> Result<usize, RemoteError> {
    let mut pending: Vec<BeadId> = rig
        .store
        .children(epic)
        .await?
        .into_iter()
        .filter(|c| c.status != BeadStatus::Closed)
        .map(|c| c.id)
        .collect();
    let mut closed = 0usize;
    // Each pass closes at least one bead (the ones nothing open depends on), so this ends.
    while !pending.is_empty() {
        let before = pending.len();
        let mut blocked = Vec::new();
        for id in pending {
            match rig.store.close(&id, reason).await {
                Ok(()) => closed += 1,
                Err(StoreError::Blocked { .. }) => blocked.push(id),
                Err(e) => return Err(e.into()),
            }
        }
        if blocked.len() == before {
            return Err(RemoteError::Store(StoreError::Rejected {
                op: crate::ports::StoreOp::Close,
                detail: format!(
                    "could not close {} task(s) under {epic}: dependency cycle",
                    blocked.len()
                ),
            }));
        }
        pending = blocked;
    }
    let members = context_members(rig, epic.as_ref()).await?;
    for item in crate::console::inbox(rig.store.as_ref()).await? {
        if item
            .title
            .strip_prefix("incident on ")
            .and_then(|t| BeadId::try_new(t.trim()).ok())
            .is_some_and(|t| members.contains(&t))
        {
            rig.store.close(&item.id, reason).await?;
            closed += 1;
        }
    }
    Ok(closed)
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
