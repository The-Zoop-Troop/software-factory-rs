//! A2A v1 wire shapes (`docs/references/a2a.md`: `ProtoJSON`, `camelCase`, `SCREAMING` enums) and
//! the pure mapping from ledger beads to them. An epic is an A2A task; an open incident or
//! question is an `INPUT_REQUIRED` task of its own.

use domain::{Attempts, BeadId, BeadKind, TaskState, Tokens};

use super::attention::attention_for;

use crate::bead::{Bead, BeadStatus};

/// `TaskState` as A2A spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum A2aState {
    #[cfg_attr(feature = "serde", serde(rename = "TASK_STATE_SUBMITTED"))]
    Submitted,
    #[cfg_attr(feature = "serde", serde(rename = "TASK_STATE_WORKING"))]
    Working,
    #[cfg_attr(feature = "serde", serde(rename = "TASK_STATE_INPUT_REQUIRED"))]
    InputRequired,
    #[cfg_attr(feature = "serde", serde(rename = "TASK_STATE_COMPLETED"))]
    Completed,
    #[cfg_attr(feature = "serde", serde(rename = "TASK_STATE_FAILED"))]
    Failed,
    #[cfg_attr(feature = "serde", serde(rename = "TASK_STATE_CANCELED"))]
    Canceled,
    #[cfg_attr(feature = "serde", serde(rename = "TASK_STATE_REJECTED"))]
    Rejected,
}

impl A2aState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        match self {
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected => true,
            Self::Submitted | Self::Working | Self::InputRequired => false,
        }
    }
}

/// One content part. Only `text` and `data` are produced here.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub enum Part {
    Text(String), // fp-allow: A2A wire shape, externally tagged `{"text": ...}`
    Data(serde_json::Value),
}

/// A message turn.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Message {
    pub message_id: String,
    pub role: String,
    pub parts: Vec<Part>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub task_id: Option<String>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub context_id: Option<String>,
    /// Free-form A2A metadata; the console reads `needs: [{rig, epic}]` for cross-rig plans.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Message {
    /// The concatenated text parts.
    #[must_use]
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(t.as_str()),
                Part::Data(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct TaskStatus {
    pub state: A2aState,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub message: Option<Message>,
    pub timestamp: String,
}

/// A2A `Task`. `metadata.factory` carries the per-state counts a console renders.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Task {
    pub id: String,
    pub context_id: String,
    pub status: TaskStatus,
    pub metadata: serde_json::Value,
}

/// Progress counts of an epic's task children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EpicProgress {
    pub total: usize,
    pub closed: usize,
    pub working: usize,
    pub incidents: usize,
}

/// Count children by factory state. Non-task children are ignored.
#[must_use]
pub fn epic_progress(children: &[Bead]) -> EpicProgress {
    children
        .iter()
        .filter(|c| c.kind == Some(BeadKind::Task))
        .fold(EpicProgress::default(), |p, c| {
            let state = c.meta.as_ref().map(|m| &m.state);
            EpicProgress {
                total: p.total + 1,
                closed: p.closed + usize::from(matches!(state, Some(TaskState::Closed { .. }))),
                working: p.working
                    + usize::from(matches!(
                        state,
                        Some(
                            TaskState::Leased { .. }
                                | TaskState::InVerify { .. }
                                | TaskState::Mergeable { .. }
                        )
                    )),
                incidents: p.incidents
                    + usize::from(matches!(state, Some(TaskState::Incident { .. }))),
            }
        })
}

/// The A2A state of an epic: closed → `COMPLETED`; any child in incident → `INPUT_REQUIRED`;
/// any child in flight → `WORKING`; otherwise `SUBMITTED` (queued, nothing claimed yet).
#[must_use]
pub fn epic_state(epic: &Bead, progress: EpicProgress) -> A2aState {
    if epic.status == BeadStatus::Closed {
        if epic.labels.iter().any(|l| l == CANCELED_LABEL) {
            A2aState::Canceled
        } else {
            A2aState::Completed
        }
    } else if progress.incidents > 0 {
        A2aState::InputRequired
    } else if progress.working > 0 || progress.closed > 0 {
        A2aState::Working
    } else {
        A2aState::Submitted
    }
}

/// Label the console puts on an epic it canceled, so the state reads back as `CANCELED`.
pub const CANCELED_LABEL: &str = "fac:canceled";

/// Render an epic and its children as an A2A task.
#[must_use]
pub fn epic_task(epic: &Bead, children: &[Bead], now: &str) -> Task {
    let progress = epic_progress(children);
    let state = epic_state(epic, progress);
    let incidents: Vec<String> = children
        .iter()
        .filter(|c| c.kind == Some(BeadKind::Task))
        .filter(|c| {
            c.meta
                .as_ref()
                .is_some_and(|m| matches!(m.state, TaskState::Incident { .. }))
        })
        .map(|c| c.id.to_string())
        .collect();
    let message = (state == A2aState::InputRequired).then(|| Message {
        message_id: format!("{}-input", epic.id),
        role: "ROLE_AGENT".to_owned(),
        parts: vec![Part::Text(format!(
            "{} task(s) in incident: {}. Resolve via the inbox.",
            incidents.len(),
            incidents.join(", ")
        ))],
        task_id: Some(epic.id.to_string()),
        context_id: Some(epic.id.to_string()),
        metadata: None,
    });
    Task {
        id: epic.id.to_string(),
        context_id: epic.id.to_string(),
        status: TaskStatus {
            state,
            message,
            timestamp: now.to_owned(),
        },
        metadata: serde_json::json!({
            "factory": {
                "kind": "epic",
                "title": epic.title,
                "tasks": progress.total,
                "closed": progress.closed,
                "working": progress.working,
                "incidents": progress.incidents,
                "children": children.iter().filter(|c| c.kind == Some(BeadKind::Task)).map(child_summary).collect::<Vec<_>>(),
            }
        }),
    }
}

/// One line per task child, for the epic detail view.
fn child_summary(c: &Bead) -> serde_json::Value {
    let (state, attempts, limit, tokens, branch) =
        c.meta
            .as_ref()
            .map_or(("unknown", 0u64, 0u64, 0u64, None), |m| {
                let branch = match &m.state {
                    TaskState::InVerify { branch, .. } | TaskState::Mergeable { branch, .. } => {
                        Some(branch.to_string())
                    }
                    TaskState::Leased { lease } => Some(format!("held by {}", lease.holder)),
                    TaskState::Open | TaskState::Closed { .. } | TaskState::Incident { .. } => None,
                };
                (
                    m.state.name(),
                    u64::from(Attempts::get(m.usage.attempts)),
                    u64::from(Attempts::get(m.budget.attempts)),
                    Tokens::get(m.usage.tokens),
                    branch,
                )
            });
    serde_json::json!({
        "id": c.id.to_string(),
        "title": c.title,
        "state": state,
        "attempts": attempts,
        "attemptLimit": limit,
        "tokens": tokens,
        "branch": branch,
        "closed": c.status == BeadStatus::Closed,
    })
}

/// Render an inbox item (incident or question) as an `INPUT_REQUIRED` task whose context is
/// the parent epic when known. Closed items read as `COMPLETED`.
#[must_use]
pub fn inbox_task(bead: &Bead, task: Option<&Bead>, now: &str) -> Task {
    let state = if bead.status == BeadStatus::Closed {
        A2aState::Completed
    } else {
        A2aState::InputRequired
    };
    let context = bead
        .parent
        .as_ref()
        .map_or_else(|| bead.id.to_string(), BeadId::to_string);
    Task {
        id: bead.id.to_string(),
        context_id: context,
        status: TaskStatus {
            state,
            message: (state == A2aState::InputRequired).then(|| Message {
                message_id: format!("{}-input", bead.id),
                role: "ROLE_AGENT".to_owned(),
                parts: vec![
                    Part::Text(format!("{}\n\n{}", bead.title, bead.description)),
                    Part::Data(
                        serde_json::to_value(attention_for(bead, task))
                            .unwrap_or(serde_json::Value::Null),
                    ),
                ],
                task_id: Some(bead.id.to_string()),
                context_id: None,
                metadata: None,
            }),
            timestamp: now.to_owned(),
        },
        metadata: serde_json::json!({
            "factory": {
                "kind": bead.kind.map_or("?", BeadKind::as_str),
                "title": bead.title,
            }
        }),
    }
}

/// A queued plan request as an A2A task: `SUBMITTED` while the rig's planner has not
/// answered, then `COMPLETED` (with `metadata.factory.epic`) or `FAILED` (with the reason).
#[must_use]
pub fn request_task(bead: &Bead, now: &str) -> Task {
    let outcome = crate::plan_queue::plan_outcome(bead);
    let (state, epic, failure) = match &outcome {
        None => (A2aState::Submitted, None, None),
        Some(Ok(epic)) => (A2aState::Completed, Some(epic.to_string()), None),
        Some(Err(e)) => (A2aState::Failed, None, Some(e.to_string())),
    };
    let needs: Vec<String> = bead
        .cross_needs
        .iter()
        .flatten()
        .map(|n| format!("{}/{}", n.rig, n.epic))
        .collect();
    let waiting = bead.status == crate::bead::BeadStatus::Deferred && !needs.is_empty();
    let progress = if waiting {
        format!("waiting for {}", needs.join(", "))
    } else {
        bead.notes
            .as_deref()
            .and_then(|n| n.lines().last())
            .unwrap_or("queued for the rig's planner")
            .to_owned()
    };
    Task {
        id: bead.id.to_string(),
        context_id: epic.clone().unwrap_or_else(|| bead.id.to_string()),
        status: TaskStatus {
            state,
            message: Some(Message {
                message_id: format!("{}-progress", bead.id),
                role: "ROLE_AGENT".to_owned(),
                parts: vec![Part::Text(failure.clone().unwrap_or(progress))],
                task_id: Some(bead.id.to_string()),
                context_id: None,
                metadata: None,
            }),
            timestamp: now.to_owned(),
        },
        metadata: serde_json::json!({
            "factory": {
                "kind": "plan_request",
                "title": bead.title,
                "epic": epic,
                "failure": failure,
                "needs": needs,
                "waiting": waiting,
            }
        }),
    }
}

/// A skill on the Agent Card.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// The skills every rig exposes, in the order the plan lists them.
#[must_use]
pub fn skills() -> Vec<AgentSkill> {
    let mk = |id: &str, name: &str, desc: &str, scope: &str| AgentSkill {
        id: id.to_owned(),
        name: name.to_owned(),
        description: desc.to_owned(),
        tags: vec!["factory".to_owned(), format!("scope:{scope}")],
    };
    vec![
        mk(
            "plan",
            "Plan",
            "SendMessage with plan text starts an epic; returns its Task.",
            "plan",
        ),
        mk(
            "watch",
            "Watch",
            "ListTasks / GetTask / SubscribeToTask over the ledger and event log.",
            "watch",
        ),
        mk(
            "inbox",
            "Inbox",
            "ListTasks filtered to INPUT_REQUIRED: incidents and questions.",
            "watch",
        ),
        mk(
            "resolve",
            "Resolve",
            "SendMessage with taskId of an inbox item closes it with the note.",
            "resolve",
        ),
        mk(
            "stop",
            "Stop",
            "CancelTask on an epic closes its open tasks.",
            "plan",
        ),
    ]
}
