//! `Harness` over Codex CLI headless (`codex exec --json`).
//!
//! Codex has no system-prompt flag, so the system prompt is prepended to the message.
//! Tool policy maps to Codex's sandbox: read-only for `None`/`ReadOnly`, full bypass for
//! `Full` (only ever inside the rig). Structured output uses `--output-schema` + `-o`.
#![allow(
    clippy::doc_markdown,
    reason = "Codex is a product name, not an identifier"
)]

use std::path::PathBuf;
use std::process::Stdio;

use app::domain::{MicroUsd, Tokens, Turns};
use app::{Harness, HarnessError, HarnessOutcome, HarnessRequest, HarnessStage, ToolPolicy};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

/// Adapter configuration. Auth comes from the environment (`OPENAI_API_KEY`) or Codex's own login.
#[derive(Debug, Clone)]
pub struct CodexCli {
    bin: PathBuf,
    model: Option<String>,
}

impl Default for CodexCli {
    fn default() -> Self {
        Self {
            bin: PathBuf::from("codex"),
            model: None,
        }
    }
}

impl CodexCli {
    #[must_use]
    pub fn with_bin(self, bin: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            ..self
        }
    }

    #[must_use]
    pub fn with_model(self, model: impl Into<String>) -> Self {
        Self {
            model: Some(model.into()),
            ..self
        }
    }
}

/// One line of `codex exec --json`. Unknown event types are ignored.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Event {
    #[serde(rename = "item.completed")]
    ItemCompleted { item: Item },
    #[serde(rename = "turn.completed")]
    TurnCompleted {
        #[serde(default)]
        usage: Usage,
    },
    #[serde(rename = "turn.failed")]
    TurnFailed {
        #[serde(default)]
        error: serde_json::Value,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Item {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Default, Deserialize)]
#[allow(
    clippy::struct_field_names,
    reason = "mirrors the CLI's JSON field names"
)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
}

/// Pure: fold the event stream into an outcome. `last_message` overrides the text when the
/// CLI wrote it (it holds the schema-validated final answer under `--output-schema`).
fn fold(events: &str, last_message: Option<String>, schema_requested: bool) -> HarnessOutcome {
    let mut text = String::new();
    let mut tokens = 0u64;
    let mut turns = 0u32;
    let mut is_error = false;
    let mut error_text = String::new();
    for line in events.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Event>(line) {
            Ok(Event::ItemCompleted { item }) if item.kind == "agent_message" => text = item.text,
            Ok(Event::ItemCompleted { .. } | Event::Other) | Err(_) => {}
            Ok(Event::TurnCompleted { usage }) => {
                turns += 1;
                tokens += usage.input_tokens
                    + usage.cached_input_tokens
                    + usage.cache_write_input_tokens
                    + usage.output_tokens
                    + usage.reasoning_output_tokens;
            }
            Ok(Event::TurnFailed { error }) => {
                is_error = true;
                error_text = error
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| error.to_string(), str::to_owned);
            }
            Ok(Event::Error { message }) => {
                is_error = true;
                error_text = message;
            }
        }
    }
    let text = match last_message {
        Some(m) if !m.trim().is_empty() => m,
        _ => text,
    };
    let structured = if schema_requested {
        serde_json::from_str(&text).ok()
    } else {
        None
    };
    if is_error && text.is_empty() {
        text_or(error_text, tokens, turns, structured)
    } else {
        HarnessOutcome {
            text,
            structured,
            tokens: Tokens::new(tokens),
            cost_micro_usd: MicroUsd::new(0),
            turns: Turns::new(turns),
            is_error,
        }
    }
}

fn text_or(
    text: String,
    tokens: u64,
    turns: u32,
    structured: Option<serde_json::Value>,
) -> HarnessOutcome {
    HarnessOutcome {
        text,
        structured,
        tokens: Tokens::new(tokens),
        cost_micro_usd: MicroUsd::new(0),
        turns: Turns::new(turns),
        is_error: true,
    }
}

#[async_trait]
impl Harness for CodexCli {
    async fn run(&self, req: HarnessRequest) -> Result<HarnessOutcome, HarnessError> {
        let scratch = req.cwd.join(".factory-codex");
        tokio::fs::create_dir_all(&scratch)
            .await
            .map_err(|e| HarnessError::Spawn {
                bin: self.bin.clone(),
                cause: crate::classify_io(e.kind()),
                detail: e.to_string(),
            })?;
        let last_path = scratch.join("last-message.txt");
        let schema_path = scratch.join("output-schema.json");
        // fp-allow: removing a previous run's file; absence is the desired state either way
        let _ = tokio::fs::remove_file(&last_path).await;

        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(&req.cwd)
            .args([
                "exec",
                "--json",
                "--ephemeral",
                "--skip-git-repo-check",
                "--color",
                "never",
            ])
            .arg("-C")
            .arg(&req.cwd)
            .arg("-o")
            .arg(&last_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        match req.tools {
            ToolPolicy::None | ToolPolicy::ReadOnly => {
                cmd.args(["-s", "read-only"]);
            }
            ToolPolicy::Full => {
                cmd.arg("--dangerously-bypass-approvals-and-sandbox");
            }
        }
        if let Some(schema) = &req.schema {
            tokio::fs::write(&schema_path, schema.to_string())
                .await
                .map_err(|e| HarnessError::Spawn {
                    bin: self.bin.clone(),
                    cause: crate::classify_io(e.kind()),
                    detail: e.to_string(),
                })?;
            cmd.arg("--output-schema").arg(&schema_path);
        }
        if let Some(m) = &self.model {
            cmd.arg("-m").arg(m);
        }
        for o in req.mcp.to_codex_overrides() {
            cmd.arg("-c").arg(o);
        }
        let prompt = format!(
            "## Instructions\n{}\n\n## Task\n{}",
            req.system_prompt, req.prompt
        );
        cmd.arg(prompt);
        tracing::info!(cwd = %req.cwd.display(), tools = ?req.tools, "codex exec");

        let child = cmd.spawn().map_err(|e| HarnessError::Spawn {
            bin: self.bin.clone(),
            cause: crate::classify_io(e.kind()),
            detail: e.to_string(),
        })?;
        let limit = std::time::Duration::from_secs(req.timeout.seconds());
        let out = match tokio::time::timeout(limit, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                return Err(HarnessError::Spawn {
                    bin: self.bin.clone(),
                    cause: crate::classify_io(e.kind()),
                    detail: e.to_string(),
                });
            }
            Err(_elapsed) => {
                return Err(HarnessError::Timeout {
                    after: req.timeout,
                    stage: HarnessStage::Prompt,
                });
            }
        };
        let events = String::from_utf8_lossy(&out.stdout);
        let last = tokio::fs::read_to_string(&last_path).await.ok();
        if let Err(e) = tokio::fs::remove_dir_all(&scratch).await {
            tracing::warn!(error = %e, path = %scratch.display(), "scratch dir not removed");
        }
        let mut outcome = fold(&events, last, req.schema.is_some());
        if !out.status.success() && !outcome.is_error {
            outcome.is_error = true;
            if outcome.text.is_empty() {
                outcome.text = String::from_utf8_lossy(&out.stderr)
                    .trim()
                    .chars()
                    .take(500)
                    .collect();
            }
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENTS: &str = r#"{"type":"thread.started","thread_id":"t"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"pong"}}
{"type":"turn.completed","usage":{"input_tokens":12938,"cached_input_tokens":9984,"cache_write_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0}}
"#;

    #[test]
    fn folds_message_and_usage() {
        let o = fold(EVENTS, None, false);
        assert_eq!(o.text, "pong");
        assert_eq!(o.tokens.get(), 12938 + 9984 + 5);
        assert_eq!(o.turns.get(), 1);
        assert!(!o.is_error);
    }

    #[test]
    fn last_message_and_schema_give_structured() {
        let o = fold(EVENTS, Some("{\"ok\":true,\"n\":2}".into()), true);
        assert_eq!(o.structured, Some(serde_json::json!({"ok": true, "n": 2})));
        assert_eq!(o.text, "{\"ok\":true,\"n\":2}");
    }

    #[test]
    fn failure_events_are_errors() {
        let o = fold(
            "{\"type\":\"turn.failed\",\"error\":{\"message\":\"rate limited\"}}\n",
            None,
            false,
        );
        assert!(o.is_error);
        assert_eq!(o.text, "rate limited");
        let o = fold(
            "{\"type\":\"error\",\"message\":\"boom\"}\ngarbage line\n",
            None,
            false,
        );
        assert!(o.is_error);
        assert_eq!(o.text, "boom");
    }

    /// Needs a logged-in Codex on this machine; run with `--ignored`.
    #[tokio::test]
    #[ignore = "live: requires codex login"]
    async fn live_pong() {
        let cwd = std::env::temp_dir().join(format!("factory-cx-{}", std::process::id()));
        std::fs::create_dir_all(&cwd).unwrap();
        let o = CodexCli::default()
            .run(HarnessRequest {
                cwd,
                system_prompt: "You are terse.".into(),
                prompt: "Reply with exactly: pong".into(),
                schema: None,
                tools: ToolPolicy::None,
                mcp: app::McpConfig::default(),
                max_turns: Turns::new(1),
                timeout: app::domain::Duration::from_seconds(120),
            })
            .await
            .unwrap();
        assert!(!o.is_error, "{}", o.text);
        assert!(o.text.to_lowercase().contains("pong"));
        assert!(o.tokens.get() > 0);
    }
}
