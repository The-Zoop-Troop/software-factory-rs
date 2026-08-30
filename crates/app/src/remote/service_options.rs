//! `apply_option`: what each inbox choice does to the ledger before the item is resolved.

use domain::{BeadId, Principal, Scope};

use super::super::Rig;
use super::super::a2a::CANCELED_LABEL;
use super::super::a2a::{A2aState, Task};
use super::super::attention::AttentionOption;
use super::{RESUME_MARKER, RemoteError, Sent, authorize, incident_task_bead, send_message};
use super::{audit, get_task, list_tasks};
use crate::bead::Bead;
use crate::bead::BeadStatus;
use crate::ports::{Clock, StoreError};

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
        AttentionOption::ReplanWithout | AttentionOption::CancelDependents => {
            resolve_upstream_failure(rig, clock, who, &item, option, item_id, &text).await
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
) -> Result<(Vec<super::super::EventRecord>, u64), RemoteError> {
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

pub(super) fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect()
}

/// An `upstream failed:` question: drop the need (and release the request when none remain) or
/// cancel the dependent request; either way the question is resolved.
async fn resolve_upstream_failure(
    rig: &Rig,
    clock: &dyn Clock,
    who: &Principal,
    item: &Bead,
    option: AttentionOption,
    item_id: &str,
    text: &str,
) -> Result<Sent, RemoteError> {
    authorize(rig, clock, who, Scope::Resolve, "SendMessage")?;
    let Some((request, failed)) = super::super::attention::upstream_failure(item) else {
        return Err(RemoteError::TaskNotFound {
            id: format!("upstream failure behind {item_id}"),
        });
    };
    let req = rig.store.show(&request).await?;
    if option == AttentionOption::CancelDependents {
        rig.store.label(&request, CANCELED_LABEL).await?;
        rig.store
            .close(&request, &format!("canceled: upstream {failed} failed"))
            .await?;
    } else {
        let kept: Vec<_> = req
            .cross_needs
            .unwrap_or_default()
            .into_iter()
            .filter(|n| format!("{}/{}", n.rig, n.epic) != failed)
            .collect();
        rig.store.set_needs(&request, &kept).await?;
        rig.store
            .note(&request, &format!("operator: continuing without {failed}"))
            .await?;
        if kept.is_empty() {
            rig.store.undefer(&request).await?;
        }
    }
    send_message(rig, clock, who, Some(item_id), text).await
}
