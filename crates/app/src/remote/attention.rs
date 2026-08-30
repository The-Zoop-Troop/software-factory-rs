//! Attention items: what a human sees when a task needs them — the reason, the evidence the
//! roles left on the ledger, and the options they can take. Pure: beads in, structure out.

use domain::task::IncidentReason;
use domain::{Attempts, BeadId, BeadKind, TaskState, Tokens};

use crate::bead::Bead;

/// Wire shape of the `DataPart` carried by an `INPUT_REQUIRED` message.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Attention {
    /// `incident` or `question`.
    pub kind: String,
    pub id: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub task_id: Option<String>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub epic_id: Option<String>,
    pub reason: Reason,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub attempts: Option<Counter>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub tokens: Option<Counter>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub branch: Option<String>,
    /// The last `verify FAILED …` block the Verifier left on the task, if any.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub last_verify: Option<String>,
    /// Guidance notes already given to workers on this task.
    pub guidance: Vec<String>,
    pub options: Vec<OptionSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct Reason {
    /// `budget` | `lease_storm` | `merge_conflict` | `manual` | `question`.
    pub kind: String,
    pub summary: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Counter {
    pub used: u64,
    pub limit: u64,
}

/// One thing the human can do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "camelCase")
)]
pub struct OptionSpec {
    pub id: AttentionOption,
    pub label: String,
    pub description: String,
    pub needs_note: bool,
    pub destructive: bool,
}

/// The options themselves; each is a workflow in `service`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum AttentionOption {
    /// Close the item; reopen the task with fresh attempts.
    RetryFresh,
    /// Same, plus a note the next worker session reads in its context packet.
    RetryWithGuidance,
    /// Cancel the whole epic.
    StopEpic,
    /// Cancel the epic and queue a new plan from its title plus the note.
    Replan,
    /// A question: record the answer and close it.
    Answer,
    /// Environment incident: reopen and continue from the task's existing branch.
    ResumeBranch,
}

impl AttentionOption {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RetryFresh => "retry_fresh",
            Self::RetryWithGuidance => "retry_with_guidance",
            Self::StopEpic => "stop_epic",
            Self::Replan => "replan",
            Self::Answer => "answer",
            Self::ResumeBranch => "resume_branch",
        }
    }

    /// # Errors
    /// Unknown option ids.
    pub fn parse(s: &str) -> Result<Self, UnknownOption> {
        match s {
            "retry_fresh" => Ok(Self::RetryFresh),
            "retry_with_guidance" => Ok(Self::RetryWithGuidance),
            "stop_epic" => Ok(Self::StopEpic),
            "replan" => Ok(Self::Replan),
            "answer" => Ok(Self::Answer),
            "resume_branch" => Ok(Self::ResumeBranch),
            other => Err(UnknownOption(other.to_owned())), // fp-allow: option ids arrive as free text
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown attention option `{0}`")]
pub struct UnknownOption(pub String);

fn spec(
    id: AttentionOption,
    label: &str,
    description: &str,
    needs_note: bool,
    destructive: bool,
) -> OptionSpec {
    OptionSpec {
        id,
        label: label.to_owned(),
        description: description.to_owned(),
        needs_note,
        destructive,
    }
}

/// The task an incident is about (`incident on <id>`).
#[must_use]
pub fn incident_task_id(bead: &Bead) -> Option<BeadId> {
    if bead.kind != Some(BeadKind::Incident) {
        return None;
    }
    bead.title
        .strip_prefix("incident on ")
        .and_then(|s| BeadId::try_new(s.trim()).ok())
}

/// The last verify block from a task's notes.
#[must_use]
pub fn last_verify_block(notes: &str) -> Option<String> {
    let start = notes.rfind("verify FAILED")?;
    let block = &notes[start..];
    let end = block.find("\nguidance:").unwrap_or(block.len());
    Some(block[..end].trim_end().to_owned())
}

/// Guidance lines a human added (`guidance: …`).
#[must_use]
pub fn guidance_notes(notes: &str) -> Vec<String> {
    notes
        .lines()
        .filter_map(|l| l.strip_prefix("guidance: "))
        .map(str::to_owned)
        .collect()
}

fn reason_of(task: Option<&Bead>, description: &str) -> Reason {
    let state = task.and_then(|t| t.meta.as_ref()).map(|m| &m.state);
    let (kind, summary) = match state {
        Some(TaskState::Incident { reason }) => match reason {
            IncidentReason::Budget { .. } => ("budget", "The task ran out of budget"),
            IncidentReason::LeaseStorm { .. } => ("lease_storm", "Workers keep losing this task"),
            IncidentReason::MergeConflict { .. } => {
                ("merge_conflict", "The branch no longer merges")
            }
            IncidentReason::Manual { .. } => ("manual", "Escalated by hand"),
            IncidentReason::Environment { .. } => {
                ("environment", "The rig could not run the checks")
            }
        },
        Some(
            TaskState::Open
            | TaskState::Leased { .. }
            | TaskState::InVerify { .. }
            | TaskState::Mergeable { .. }
            | TaskState::Closed { .. },
        ) => ("resolved", "The task has already moved on"),
        None => ("question", "An agent needs an answer"),
    };
    Reason {
        kind: kind.to_owned(),
        summary: summary.to_owned(),
        detail: description.trim().to_owned(),
    }
}

fn branch_of(task: &Bead) -> Option<String> {
    task.meta.as_ref().and_then(|m| match &m.state {
        TaskState::InVerify { branch, .. } | TaskState::Mergeable { branch, .. } => {
            Some(branch.to_string())
        }
        TaskState::Leased { lease } => Some(format!("held by {}", lease.holder)),
        TaskState::Open | TaskState::Closed { .. } | TaskState::Incident { .. } => None,
    })
}

/// Build the attention item for an inbox bead and (for incidents) its task.
#[must_use]
pub fn attention_for(item: &Bead, task: Option<&Bead>) -> Attention {
    let is_incident = item.kind == Some(BeadKind::Incident);
    let meta = task.and_then(|t| t.meta.as_ref());
    let notes = task.and_then(|t| t.notes.as_deref()).unwrap_or_default();
    let environment = matches!(
        task.and_then(|t| t.meta.as_ref()).map(|m| &m.state),
        Some(TaskState::Incident {
            reason: IncidentReason::Environment { .. }
        })
    );
    let options = if is_incident && environment {
        vec![
            spec(
                AttentionOption::ResumeBranch,
                "Resume from the branch",
                "The code was fine; the rig was not. Fix the rig, then continue from the task's existing branch.",
                false,
                false,
            ),
            spec(
                AttentionOption::RetryFresh,
                "Retry from scratch",
                "Reopen the task with fresh attempts and budget on a new branch.",
                false,
                false,
            ),
            spec(
                AttentionOption::StopEpic,
                "Stop the epic",
                "Cancel every open task under this epic.",
                false,
                true,
            ),
        ]
    } else if is_incident {
        vec![
            spec(
                AttentionOption::RetryFresh,
                "Retry",
                "Reopen the task with fresh attempts and budget.",
                false,
                false,
            ),
            spec(
                AttentionOption::RetryWithGuidance,
                "Retry with guidance",
                "Reopen it and tell the next worker what to do differently.",
                true,
                false,
            ),
            spec(
                AttentionOption::Replan,
                "Re-plan",
                "Stop this epic and queue a new plan from its goal plus your note.",
                true,
                true,
            ),
            spec(
                AttentionOption::StopEpic,
                "Stop the epic",
                "Cancel every open task under this epic.",
                false,
                true,
            ),
        ]
    } else {
        vec![spec(
            AttentionOption::Answer,
            "Answer",
            "Record the answer; the agent that asked reads it.",
            true,
            false,
        )]
    };
    Attention {
        kind: item.kind.map_or("?", BeadKind::as_str).to_owned(),
        id: item.id.to_string(),
        task_id: task.map(|t| t.id.to_string()),
        epic_id: task
            .and_then(|t| t.parent.as_ref())
            .or(item.parent.as_ref())
            .map(ToString::to_string),
        reason: reason_of(task, &item.description),
        attempts: meta.map(|m| Counter {
            used: u64::from(Attempts::get(m.usage.attempts)),
            limit: u64::from(Attempts::get(m.budget.attempts)),
        }),
        tokens: meta.map(|m| Counter {
            used: Tokens::get(m.usage.tokens),
            limit: Tokens::get(m.budget.tokens),
        }),
        branch: task.and_then(branch_of),
        last_verify: last_verify_block(notes),
        guidance: guidance_notes(notes),
        options,
    }
}
