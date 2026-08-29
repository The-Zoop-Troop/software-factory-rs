//! `Harness` over Claude Code headless (`claude -p`). One process per request; the JSON
//! result envelope is decoded once into `HarnessOutcome`.

use std::path::PathBuf;
use std::process::Stdio;

use app::domain::{MicroUsd, Tokens, Turns};
use app::{Harness, HarnessError, HarnessOutcome, HarnessRequest, HarnessStage, ToolPolicy};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

/// Adapter configuration.
#[derive(Debug, Clone)]
pub struct ClaudeCli {
    bin: PathBuf,
    model: Option<String>,
    /// Hard cap per run, in USD, passed as `--max-budget-usd`.
    max_budget_usd: Option<f64>,
}

impl Default for ClaudeCli {
    fn default() -> Self {
        Self {
            bin: PathBuf::from("claude"),
            model: None,
            max_budget_usd: None,
        }
    }
}

impl ClaudeCli {
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

    #[must_use]
    pub fn with_max_budget_usd(self, usd: f64) -> Self {
        Self {
            max_budget_usd: Some(usd),
            ..self
        }
    }
}

/// The `type: "result"` envelope of `claude -p --output-format json`. Unknown fields ignored.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    result: String,
    #[serde(default)]
    structured_output: Option<serde_json::Value>,
    #[serde(default)]
    num_turns: u32,
    #[serde(default)]
    total_cost_usd: f64,
    #[serde(default)]
    usage: Usage,
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
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

impl From<Envelope> for HarnessOutcome {
    fn from(e: Envelope) -> Self {
        let u = e.usage;
        Self {
            text: e.result,
            structured: e.structured_output,
            tokens: Tokens::new(
                u.input_tokens
                    + u.output_tokens
                    + u.cache_creation_input_tokens
                    + u.cache_read_input_tokens,
            ),
            cost_micro_usd: MicroUsd::new(micro_usd(e.total_cost_usd)),
            turns: Turns::new(e.num_turns),
            is_error: e.is_error,
        }
    }
}

/// USD → integer micro-dollars; the only place a float from the outside is touched.
#[allow(
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "boundary decode of a reported cost; rounded once into integer minor units"
)]
fn micro_usd(usd: f64) -> u64 {
    if usd.is_finite() && usd > 0.0 {
        (usd * 1_000_000.0).round() as u64 // fp-allow: float→integer minor units at the boundary; finite and positive checked above
    } else {
        0
    }
}

#[async_trait]
impl Harness for ClaudeCli {
    async fn run(&self, req: HarnessRequest) -> Result<HarnessOutcome, HarnessError> {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(&req.cwd)
            .arg("-p")
            .arg(&req.prompt)
            .args(["--output-format", "json", "--no-session-persistence"])
            .arg("--max-turns")
            .arg(req.max_turns.get().to_string())
            .arg("--system-prompt")
            .arg(&req.system_prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        match req.tools {
            ToolPolicy::None => {
                cmd.args(["--tools", ""]);
            }
            ToolPolicy::ReadOnly => {
                cmd.args(["--tools", "Read,Glob,Grep,LS", "--permission-mode", "plan"]);
            }
            ToolPolicy::Full => {
                cmd.args(["--tools", "default", "--dangerously-skip-permissions"]);
            }
        }
        if let Some(schema) = &req.schema {
            cmd.arg("--json-schema").arg(schema.to_string());
        }
        if !req.mcp.is_empty() {
            cmd.arg("--mcp-config")
                .arg(req.mcp.to_claude_json().to_string())
                .arg("--strict-mcp-config");
        }
        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(usd) = self.max_budget_usd {
            cmd.arg("--max-budget-usd").arg(usd.to_string());
        }
        tracing::info!(cwd = %req.cwd.display(), tools = ?req.tools, max_turns = req.max_turns.get(), "claude -p");

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
        let stdout = String::from_utf8_lossy(&out.stdout);
        // On a hard failure the CLI may print nothing parseable; surface stderr instead.
        let envelope: Envelope = serde_json::from_str(stdout.trim()).map_err(|e| {
            let stderr = String::from_utf8_lossy(&out.stderr);
            HarnessError::Decode {
                stage: HarnessStage::Envelope,
                detail: format!(
                    "{e}; exit {:?}; stderr: {}",
                    out.status.code(),
                    stderr.trim()
                ),
            }
        })?;
        Ok(envelope.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_decodes_and_sums_tokens() {
        let json = r#"{"type":"result","is_error":false,"result":"{\"ok\":true}","structured_output":{"ok":true},
            "num_turns":2,"total_cost_usd":0.0897,"usage":{"input_tokens":2,"output_tokens":82,
            "cache_creation_input_tokens":4231,"cache_read_input_tokens":0,"other":1},"extra":"ignored"}"#;
        let e: Envelope = serde_json::from_str(json).unwrap();
        let o = HarnessOutcome::from(e);
        assert_eq!(o.tokens.get(), 4315);
        assert_eq!(o.cost_micro_usd.get(), 89_700);
        assert_eq!(o.structured, Some(serde_json::json!({"ok": true})));
        assert!(!o.is_error);
    }

    #[test]
    fn error_envelope() {
        let json = r#"{"type":"result","is_error":true,"result":"Not logged in","num_turns":1}"#;
        let o: HarnessOutcome = serde_json::from_str::<Envelope>(json).unwrap().into();
        assert!(o.is_error);
        assert_eq!(o.tokens.get(), 0);
        assert_eq!(o.structured, None);
    }
}
