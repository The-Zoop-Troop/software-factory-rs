//! `factory --rig <url>`: the same commands, executed against a console over A2A.

use app::domain;
use app::remote::chat::{ChatCommand, handle, render_inbox, render_tasks};
use app::{A2aApi, ClientError};

use crate::cli::Command;

/// The subset of commands that make sense against a console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteCommand {
    Watch {
        interval: Option<u64>,
    },
    Inbox {
        resolve: Option<String>,
        note: String,
    },
    Plan {
        text: String,
        needs: Vec<domain::CrossRigNeed>,
    },
    Stop {
        epic: String,
    },
    Doctor,
    Metrics {
        epic: Option<String>,
        json: bool,
        csv: bool,
    },
    Telegram {
        bot_token: String,
        chats: Vec<i64>,
        poll: u64,
        api_base: String,
    },
}

/// Why a command cannot run remotely.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RemoteUnsupported {
    #[error("`{name}` runs inside a rig, not over --rig")]
    LocalOnly { name: &'static str },
    #[error("give the plan with --text or --file")]
    NoPlanText,
    #[error("cannot read plan file: {detail}")]
    PlanFile { detail: String },
    #[error("--after wants `rig:epic`, got `{given}`")]
    BadNeed { given: String },
}

/// Map a CLI command to its remote form.
///
/// # Errors
/// `LocalOnly` for rig-internal roles; plan text problems.
pub(crate) fn remote_command(cmd: Command) -> Result<RemoteCommand, RemoteUnsupported> {
    match cmd {
        Command::Watch { interval } => Ok(RemoteCommand::Watch { interval }),
        Command::Inbox { resolve, note } => Ok(RemoteCommand::Inbox { resolve, note }),
        Command::Stop { epic } => Ok(RemoteCommand::Stop { epic }),
        Command::Doctor { .. } => Ok(RemoteCommand::Doctor),
        Command::Metrics {
            epic, json, csv, ..
        } => Ok(RemoteCommand::Metrics { epic, json, csv }),
        Command::Telegram {
            bot_token,
            chats,
            poll,
            api_base,
        } => Ok(RemoteCommand::Telegram {
            bot_token,
            chats,
            poll,
            api_base,
        }),
        Command::Plan(args) => {
            let crate::plan_cmd::PlanArgs {
                file,
                text,
                after,
                queued,
                ..
            } = args;
            if queued {
                return Err(RemoteUnsupported::LocalOnly {
                    name: "plan --queued",
                });
            }
            let text = match (file, text) {
                (Some(f), _) => {
                    std::fs::read_to_string(f).map_err(|e| RemoteUnsupported::PlanFile {
                        detail: e.to_string(),
                    })?
                }
                (None, Some(t)) => t,
                (None, None) => return Err(RemoteUnsupported::NoPlanText),
            };
            let needs = after
                .iter()
                .map(|s| parse_need(s))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(RemoteCommand::Plan { text, needs })
        }
        Command::Version => Err(RemoteUnsupported::LocalOnly { name: "version" }),
        Command::Bead { .. } => Err(RemoteUnsupported::LocalOnly { name: "bead" }),
        Command::Rig { .. } => Err(RemoteUnsupported::LocalOnly { name: "rig" }),
        Command::Work { .. } => Err(RemoteUnsupported::LocalOnly { name: "work" }),
        Command::Verify { .. } => Err(RemoteUnsupported::LocalOnly { name: "verify" }),
        Command::Integrate { .. } => Err(RemoteUnsupported::LocalOnly { name: "integrate" }),
    }
}

/// One remote command → the text to print. `Watch` with an interval and `Telegram` are
/// loops handled by [`run_remote`]; here they do a single pass.
///
/// # Errors
/// The console's refusal or a transport failure.
pub(crate) async fn execute(api: &dyn A2aApi, cmd: RemoteCommand) -> Result<String, ClientError> {
    match cmd {
        RemoteCommand::Watch { .. } => Ok(render_tasks(&api.list_tasks().await?)),
        RemoteCommand::Inbox { resolve, note } => {
            let mut out = String::new();
            if let Some(id) = resolve {
                out.push_str(&handle(api, ChatCommand::Resolve { id, note }).await?);
            }
            let inbox = render_inbox(&api.list_tasks().await?);
            out.push_str(if inbox.is_empty() {
                "inbox empty\n"
            } else {
                &inbox
            });
            Ok(out)
        }
        RemoteCommand::Plan { text, needs } => handle(api, ChatCommand::Plan { text, needs }).await,
        RemoteCommand::Stop { epic } => handle(api, ChatCommand::Stop { id: epic }).await,
        RemoteCommand::Metrics { epic, json, csv } => {
            let body = api.metrics(epic.as_deref()).await?;
            Ok(crate::metrics::render_value(&body, json, csv))
        }
        RemoteCommand::Doctor => {
            let card = api.card().await?;
            let name = card
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let tasks = api.list_tasks().await?;
            Ok(format!(
                "ok  console reachable: {name}\nok  token accepted: {} task(s) visible\n",
                tasks.len()
            ))
        }
        RemoteCommand::Telegram { .. } => Ok("telegram bot: use `factory telegram`\n".to_owned()),
    }
}

pub(crate) async fn run_remote(api: &infra::A2aHttp, cmd: Command) -> anyhow::Result<()> {
    let cmd = remote_command(cmd)?;
    match cmd {
        RemoteCommand::Watch {
            interval: Some(secs),
        } => loop {
            print!(
                "{}",
                execute(api, RemoteCommand::Watch { interval: None }).await?
            );
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
        },
        RemoteCommand::Telegram {
            bot_token,
            chats,
            poll,
            api_base,
        } => {
            let tg = infra::TelegramApi::new(&api_base, &bot_token)?;
            crate::telegram::run_bot(api, &tg, &chats, poll).await
        }
        single @ (RemoteCommand::Watch { interval: None }
        | RemoteCommand::Inbox { .. }
        | RemoteCommand::Plan { .. }
        | RemoteCommand::Stop { .. }
        | RemoteCommand::Metrics { .. }
        | RemoteCommand::Doctor) => {
            print!("{}", execute(api, single).await?);
            Ok(())
        }
    }
}

/// `rig:epic` → a cross-rig need.
pub(crate) fn parse_need(s: &str) -> Result<domain::CrossRigNeed, RemoteUnsupported> {
    let bad = || RemoteUnsupported::BadNeed {
        given: s.to_owned(),
    };
    let (rig, epic) = s.split_once(':').ok_or_else(bad)?;
    Ok(domain::CrossRigNeed {
        rig: domain::RigName::try_new(rig).map_err(|_| bad())?,
        epic: domain::BeadId::try_new(epic).map_err(|_| bad())?,
    })
}
