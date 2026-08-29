//! The Telegram bot: commands from allowed chats go to the console; state changes come back
//! as push messages. Long polling, so the bot needs no inbound port anywhere.

use app::remote::chat::{Seen, handle, notifications, parse_command};
use app::{A2aApi, ChatParseError, ClientError};
use async_trait::async_trait;
use infra::Incoming;

/// What the bot needs from Telegram (or a fake).
#[async_trait]
pub(crate) trait ChatTransport: Send + Sync {
    /// # Errors
    /// Transport failures; the loop logs and retries.
    async fn updates(&self, offset: i64, timeout_secs: u64) -> Result<Vec<Incoming>, ClientError>;
    /// # Errors
    /// Transport failures; the loop logs and continues.
    async fn send(&self, chat_id: i64, text: &str) -> Result<(), ClientError>;
}

#[async_trait]
impl ChatTransport for infra::TelegramApi {
    async fn updates(&self, offset: i64, timeout_secs: u64) -> Result<Vec<Incoming>, ClientError> {
        infra::TelegramApi::updates(self, offset, timeout_secs).await
    }
    async fn send(&self, chat_id: i64, text: &str) -> Result<(), ClientError> {
        infra::TelegramApi::send(self, chat_id, text).await
    }
}

/// Reply to one message. Unknown chats get nothing; non-commands get nothing.
pub(crate) async fn reply_for(api: &dyn A2aApi, allowed: &[i64], msg: &Incoming) -> Option<String> {
    if !allowed.contains(&msg.chat_id) {
        tracing::warn!(chat = msg.chat_id, "ignoring message from unlisted chat");
        return None;
    }
    match parse_command(&msg.text) {
        Ok(cmd) => Some(
            handle(api, cmd)
                .await
                .unwrap_or_else(|e| format!("error: {e}")),
        ),
        Err(ChatParseError::NotACommand) => None,
        Err(e) => Some(e.to_string()),
    }
}

/// The bot's state between polls.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct BotState {
    pub offset: i64,
    pub seen: Seen,
    pub primed: bool,
}

/// One iteration: answer new messages, then push state changes. Returns the next state.
pub(crate) async fn step(
    api: &dyn A2aApi,
    tg: &dyn ChatTransport,
    allowed: &[i64],
    state: BotState,
    long_poll_secs: u64,
) -> BotState {
    let mut next = state;
    match tg.updates(next.offset, long_poll_secs).await {
        Ok(msgs) => {
            for m in msgs {
                next.offset = next.offset.max(m.update_id + 1);
                if let Some(text) = reply_for(api, allowed, &m).await
                    && let Err(e) = tg.send(m.chat_id, &text).await
                {
                    tracing::warn!(error = %e, "telegram send failed");
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "telegram poll failed"),
    }
    match api.list_tasks().await {
        Ok(tasks) => {
            let (messages, seen) = notifications(&next.seen, &tasks);
            // The first poll only learns the current state; nothing is announced.
            if next.primed {
                for text in messages {
                    for chat in allowed {
                        if let Err(e) = tg.send(*chat, &text).await {
                            tracing::warn!(error = %e, "telegram push failed");
                        }
                    }
                }
            }
            next.seen = seen;
            next.primed = true;
        }
        Err(e) => tracing::warn!(error = %e, "console poll failed"),
    }
    next
}

pub(crate) async fn run_bot(
    api: &dyn A2aApi,
    tg: &dyn ChatTransport,
    allowed: &[i64],
    poll_secs: u64,
) -> anyhow::Result<()> {
    tracing::info!(chats = allowed.len(), "telegram bot running");
    let mut state = BotState::default();
    loop {
        state = step(api, tg, allowed, state, poll_secs.clamp(1, 50)).await;
    }
}
