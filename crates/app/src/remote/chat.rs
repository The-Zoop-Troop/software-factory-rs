//! Client side of remote control: the `A2aApi` port a thin client speaks through, plain-text
//! renderings of tasks, chat command parsing, and the notification diff a bot sends from.
//! Pure except for the port, so the CLI and the Telegram bot share one tested core.

use core::fmt::Write as _;
use std::collections::BTreeMap;

use async_trait::async_trait;

use super::a2a::{A2aState, Task};

/// A failed call to a console.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("console refused ({status}{}): {message}", code.map(|c| format!(", code {c}")).unwrap_or_default())]
    Refused {
        status: u16,
        code: Option<i32>,
        message: String,
    },
    #[error("console unreachable: {detail}")]
    Transport { detail: String },
    #[error("console reply not understood: {detail}")]
    Decode { detail: String },
}

/// What a client can do to one rig. Implemented over HTTP in `infra`; faked in tests.
#[async_trait]
pub trait A2aApi: Send + Sync {
    /// The rig's Agent Card (no auth needed).
    ///
    /// # Errors
    /// `Refused` (404 for an unknown rig), `Transport`, or `Decode`.
    async fn card(&self) -> Result<serde_json::Value, ClientError>;
    /// # Errors
    /// `Refused`, `Transport`, or `Decode`.
    async fn list_tasks(&self) -> Result<Vec<Task>, ClientError>;
    /// # Errors
    /// `Refused` (404 for unknown ids), `Transport`, or `Decode`.
    async fn get_task(&self, id: &str) -> Result<Task, ClientError>;
    /// Plan (no `task_id`) or resolve (`task_id` of an inbox item).
    ///
    /// # Errors
    /// `Refused`, `Transport`, or `Decode`.
    async fn send(&self, text: &str, task_id: Option<&str>) -> Result<Task, ClientError>;
    /// The rig's throughput report (`GET /rigs/<rig>/metrics?epic=`), as the console renders it.
    ///
    /// # Errors
    /// `Refused`, `Transport`, or `Decode`.
    async fn metrics(&self, epic: Option<&str>) -> Result<serde_json::Value, ClientError>;
    /// # Errors
    /// `Refused`, `Transport`, or `Decode`.
    async fn cancel(&self, id: &str) -> Result<Task, ClientError>;
}

fn short_state(s: A2aState) -> &'static str {
    match s {
        A2aState::Submitted => "queued",
        A2aState::Working => "working",
        A2aState::InputRequired => "NEEDS YOU",
        A2aState::Completed => "done",
        A2aState::Failed => "failed",
        A2aState::Canceled => "canceled",
        A2aState::Rejected => "rejected",
    }
}

fn factory_field<'a>(t: &'a Task, key: &str) -> Option<&'a serde_json::Value> {
    t.metadata.get("factory").and_then(|f| f.get(key))
}

fn is_epic(t: &Task) -> bool {
    factory_field(t, "kind").and_then(serde_json::Value::as_str) == Some("epic")
}

/// One line per epic: `id  title  [closed/total] state`, then inbox items.
#[must_use]
pub fn render_tasks(tasks: &[Task]) -> String {
    let mut out = String::new();
    for t in tasks.iter().filter(|t| is_epic(t)) {
        let title = factory_field(t, "title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let n = |k: &str| {
            factory_field(t, k)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        let _ = writeln!(
            out,
            "{}  {title}  [{}/{}] {}{}",
            t.id,
            n("closed"),
            n("tasks"),
            short_state(t.status.state),
            if n("incidents") > 0 {
                format!(", {} incident(s)", n("incidents"))
            } else {
                String::new()
            }
        );
    }
    let inbox = render_inbox(tasks);
    if out.is_empty() && inbox.is_empty() {
        return "nothing in flight\n".to_owned();
    }
    out.push_str(&inbox);
    out
}

/// Inbox items (`INPUT_REQUIRED` non-epics) with the agent's question.
#[must_use]
pub fn render_inbox(tasks: &[Task]) -> String {
    tasks
        .iter()
        .filter(|t| !is_epic(t) && t.status.state == A2aState::InputRequired)
        .fold(String::new(), |mut out, t| {
            let text = t
                .status
                .message
                .as_ref()
                .map(super::a2a::Message::text)
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{}  [{}] {}",
                t.id,
                factory_field(t, "kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("?"),
                text.lines().next().unwrap_or_default()
            );
            out
        })
}

/// Commands a chat client understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCommand {
    Plan { text: String },
    Watch,
    Inbox,
    Resolve { id: String, note: String },
    Stop { id: String },
    Help,
}

/// A chat line that is not a command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatParseError {
    #[error("not a command; try /help")]
    NotACommand,
    #[error("unknown command /{name}; try /help")]
    Unknown { name: String },
    #[error("/{name} needs {needs}")]
    MissingArgument { name: String, needs: &'static str },
}

/// Parse `/plan …`, `/watch`, `/inbox`, `/resolve <id> <note>`, `/stop <id>`, `/help`.
/// A trailing `@botname` on the command is ignored (Telegram group syntax).
///
/// # Errors
/// `NotACommand` unless the line starts with `/`; `Unknown`/`MissingArgument` otherwise.
pub fn parse_command(line: &str) -> Result<ChatCommand, ChatParseError> {
    let line = line.trim();
    let Some(rest) = line.strip_prefix('/') else {
        return Err(ChatParseError::NotACommand);
    };
    let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let name = name.split('@').next().unwrap_or(name).to_ascii_lowercase();
    let args = args.trim();
    let need = |needs| ChatParseError::MissingArgument {
        name: name.clone(),
        needs,
    };
    match name.as_str() {
        "plan" if !args.is_empty() => Ok(ChatCommand::Plan {
            text: args.to_owned(),
        }),
        "plan" => Err(need("the plan text")),
        "watch" | "status" => Ok(ChatCommand::Watch),
        "inbox" => Ok(ChatCommand::Inbox),
        "resolve" => match args.split_once(char::is_whitespace) {
            Some((id, note)) if !note.trim().is_empty() => Ok(ChatCommand::Resolve {
                id: id.to_owned(),
                note: note.trim().to_owned(),
            }),
            _ => Err(need("<id> <note>")), // fp-allow: splitting free text, not a domain enum
        },
        "stop" if !args.is_empty() => Ok(ChatCommand::Stop {
            id: args.to_owned(),
        }),
        "stop" => Err(need("<epic id>")),
        "help" | "start" => Ok(ChatCommand::Help),
        _ => Err(ChatParseError::Unknown { name }), // fp-allow: command names are free text
    }
}

pub const HELP: &str = "/plan <text> — start an epic\n/watch — epics and inbox\n/inbox — what needs you\n/resolve <id> <note> — answer an incident or question\n/stop <epic> — cancel an epic\n/help";

/// Run a command against the console and render the reply.
///
/// # Errors
/// The console's refusal or a transport/decode failure, for the caller to render.
pub async fn handle(api: &dyn A2aApi, cmd: ChatCommand) -> Result<String, ClientError> {
    match cmd {
        ChatCommand::Help => Ok(HELP.to_owned()),
        ChatCommand::Watch => Ok(render_tasks(&api.list_tasks().await?)),
        ChatCommand::Inbox => {
            let inbox = render_inbox(&api.list_tasks().await?);
            Ok(if inbox.is_empty() {
                "inbox empty\n".to_owned()
            } else {
                inbox
            })
        }
        ChatCommand::Plan { text } => {
            let t = api.send(&text, None).await?;
            Ok(format!(
                "planned: epic {} ({})\n",
                t.id,
                short_state(t.status.state)
            ))
        }
        ChatCommand::Resolve { id, note } => {
            let t = api.send(&note, Some(&id)).await?;
            Ok(format!(
                "resolved {} ({})\n",
                t.id,
                short_state(t.status.state)
            ))
        }
        ChatCommand::Stop { id } => {
            let t = api.cancel(&id).await?;
            Ok(format!(
                "stopped {} ({})\n",
                t.id,
                short_state(t.status.state)
            ))
        }
    }
}

/// `now` plus every task from `seen` that is no longer listed (a closed epic drops out of
/// `ListTasks`), fetched individually so its terminal state is observed once.
pub async fn with_vanished(api: &dyn A2aApi, seen: &Seen, now: Vec<Task>) -> Vec<Task> {
    let listed: std::collections::BTreeSet<&str> = now.iter().map(|t| t.id.as_str()).collect();
    let mut all = now.clone();
    for id in seen
        .iter()
        .filter(|(id, state)| !listed.contains(id.as_str()) && !state.is_terminal())
        .map(|(id, _)| id)
    {
        if let Ok(t) = api.get_task(id).await {
            all.push(t);
        }
    }
    all
}

/// Known task states, keyed by id, for change detection between polls.
pub type Seen = BTreeMap<String, A2aState>;

/// Messages worth pushing given what was seen before: a task that now needs a human, or
/// that reached a terminal state. First observation of an already-terminal task is silent.
#[must_use]
pub fn notifications(seen: &Seen, now: &[Task]) -> (Vec<String>, Seen) {
    let next: Seen = now.iter().map(|t| (t.id.clone(), t.status.state)).collect();
    let messages = now
        .iter()
        .filter_map(|t| {
            let before = seen.get(&t.id).copied();
            let state = t.status.state;
            let title = factory_field(t, "title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let verb = match state {
                A2aState::InputRequired => Some("needs you"),
                A2aState::Completed => Some("done"),
                A2aState::Canceled => Some("canceled"),
                A2aState::Failed | A2aState::Rejected => Some("failed"),
                A2aState::Submitted | A2aState::Working => None,
            };
            let unchanged = before == Some(state);
            let first_sight_of_finished = before.is_none() && state.is_terminal();
            if unchanged || first_sight_of_finished {
                return None;
            }
            verb.map(|v| format!("{} {v}: {title}", t.id))
        })
        .collect();
    (messages, next)
}
