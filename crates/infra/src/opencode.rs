//! `Harness` over OpenCode's headless server (`opencode serve` + HTTP API).
#![allow(
    clippy::doc_markdown,
    reason = "OpenCode is a product name, not an identifier"
)]
//!
//! The CLI (`opencode run`) buffers stdout and never exits when headless, so each request
//! gets its own short-lived server: spawn in `req.cwd`, wait for `/global/health`, create a
//! session, `POST /session/{id}/message`, read `tokens`/`cost`/`error`/`structured`, kill.

use std::path::PathBuf;
use std::process::Stdio;

use app::{Harness, HarnessError, HarnessOutcome, HarnessRequest, HarnessStage, ToolPolicy};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::{Child, Command};

/// `--model` was not `provider/model`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("model `{spec}` must be provider/model")]
pub struct ModelSpecError {
    pub spec: String,
}

/// Adapter configuration. `provider_id`/`model_id` name a provider configured in the
/// OpenCode config the server loads (user config, project `opencode.json`, or
/// `OPENCODE_CONFIG_CONTENT`).
#[derive(Debug, Clone)]
pub struct OpencodeServer {
    bin: PathBuf,
    provider_id: String,
    model_id: String,
    /// Extra JSON merged into the server's config via `OPENCODE_CONFIG_CONTENT`.
    config_content: Option<String>,
}

impl OpencodeServer {
    #[must_use]
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            bin: PathBuf::from("opencode"),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            config_content: None,
        }
    }

    /// Parse `provider/model` (OpenCode's `--model` format).
    ///
    /// # Errors
    /// `ModelSpecError` if the spec is not `provider/model` with both parts non-empty.
    pub fn from_model_spec(spec: &str) -> Result<Self, ModelSpecError> {
        let (p, m) = spec.split_once('/').ok_or_else(|| ModelSpecError {
            spec: spec.to_owned(),
        })?;
        if p.is_empty() || m.is_empty() {
            return Err(ModelSpecError {
                spec: spec.to_owned(),
            });
        }
        Ok(Self::new(p, m))
    }

    #[must_use]
    pub fn with_bin(self, bin: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            ..self
        }
    }

    /// Provider block etc. to inject at runtime (JSON object).
    #[must_use]
    pub fn with_config_content(self, json: impl Into<String>) -> Self {
        Self {
            config_content: Some(json.into()),
            ..self
        }
    }
}

/// `info` of an assistant message as returned by `POST /session/{id}/message`.
#[derive(Debug, Deserialize)]
struct MessageResponse {
    info: Info,
    #[serde(default)]
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize)]
struct Info {
    #[serde(default)]
    tokens: Tokens,
    #[serde(default)]
    cost: f64,
    #[serde(default)]
    finish: Option<String>,
    #[serde(default)]
    error: Option<serde_json::Value>,
    #[serde(default)]
    structured: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct Tokens {
    #[serde(default)]
    total: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Part {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(other)]
    Other,
}

impl From<MessageResponse> for HarnessOutcome {
    fn from(r: MessageResponse) -> Self {
        let text = r
            .parts
            .into_iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text),
                Part::Other => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let is_error = r.info.error.is_some();
        let text = match &r.info.error {
            Some(e) if text.is_empty() => error_text(e),
            _ => text,
        };
        Self {
            text,
            structured: r.info.structured.filter(|v| !v.is_null()),
            tokens: r.info.tokens.total,
            cost_micro_usd: micro_usd(r.info.cost),
            // OpenCode doesn't report turn count; `finish` present means one completed step.
            turns: u32::from(r.info.finish.is_some()),
            is_error,
        }
    }
}

fn error_text(e: &serde_json::Value) -> String {
    e.get("data")
        .and_then(|d| d.get("message"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| e.to_string(), str::to_owned)
}

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

/// Tool map for the message body: OpenCode enables everything not listed.
fn tools_for(policy: ToolPolicy) -> serde_json::Value {
    const ALL: [&str; 13] = [
        "bash",
        "read",
        "glob",
        "grep",
        "edit",
        "write",
        "task",
        "webfetch",
        "todowrite",
        "websearch",
        "skill",
        "apply_patch",
        "question",
    ];
    let allowed: &[&str] = match policy {
        ToolPolicy::None => &[],
        ToolPolicy::ReadOnly => &["read", "glob", "grep"],
        ToolPolicy::Full => &[
            "bash",
            "read",
            "glob",
            "grep",
            "edit",
            "write",
            "apply_patch",
            "todowrite",
        ],
    };
    ALL.iter()
        .map(|t| {
            (
                (*t).to_owned(),
                serde_json::Value::Bool(allowed.contains(t)),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

/// A running server, killed on drop.
struct Server {
    child: Child,
    base: String,
}

impl Server {
    async fn start(cfg: &OpencodeServer, cwd: &std::path::Path) -> Result<Self, HarnessError> {
        let port = free_port().await?;
        let mut cmd = Command::new(&cfg.bin);
        cmd.args([
            "serve",
            "--pure",
            "--hostname",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        // Permission "allow" is what makes the run headless; merged over user/project config.
        let content = match &cfg.config_content {
            Some(extra) => merge_permission(extra),
            None => r#"{"permission":"allow"}"#.to_owned(),
        };
        cmd.env("OPENCODE_CONFIG_CONTENT", content);
        let child = cmd.spawn().map_err(|e| HarnessError::Spawn {
            bin: cfg.bin.clone(),
            cause: crate::classify_io(e.kind()),
            detail: e.to_string(),
        })?;
        let base = format!("http://127.0.0.1:{port}");
        let client = local_client()?;
        // fp-allow: monotonic local deadline for a process health probe, not domain time
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let probe = client
                .get(format!("{base}/global/health"))
                .timeout(std::time::Duration::from_secs(3))
                .send()
                .await;
            if probe.is_ok_and(|r| r.status().is_success()) {
                break;
            }
            // fp-allow: local monotonic deadline for a process health probe, not domain time
            if tokio::time::Instant::now() > deadline {
                // fp-allow: local monotonic deadline for a health probe
                // fp-allow: see above
                return Err(HarnessError::Timeout {
                    after: app::domain::Duration::from_seconds(30),
                    stage: HarnessStage::Health,
                });
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        Ok(Self { child, base })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // fp-allow: Drop cannot report; the server is disposable
        let _ = self.child.start_kill();
    }
}

fn merge_permission(extra: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(extra) {
        Ok(serde_json::Value::Object(mut m)) => {
            m.entry("permission")
                .or_insert(serde_json::Value::String("allow".into()));
            serde_json::Value::Object(m).to_string()
        }
        _ => r#"{"permission":"allow"}"#.to_owned(),
    }
}

/// The server is always on loopback; never let `HTTP(S)_PROXY` env (set inside the rig)
/// route these calls through the egress proxy.
fn local_client() -> Result<reqwest::Client, HarnessError> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(5))
        // A connection accepted during server boot may never be serviced; don't reuse any.
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| HarnessError::Spawn { bin: PathBuf::from("opencode"), cause: app::Unavailable::Io, detail: e.to_string() })
}

async fn free_port() -> Result<u16, HarnessError> {
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| HarnessError::Spawn {
            bin: PathBuf::from("opencode"),
            cause: app::Unavailable::Io,
            detail: e.to_string(),
        })?;
    l.local_addr()
        .map(|a| a.port())
        .map_err(|e| HarnessError::Spawn {
            bin: PathBuf::from("opencode"),
            cause: app::Unavailable::Io,
            detail: e.to_string(),
        })
}

#[derive(Deserialize)]
struct Created {
    id: String,
}

#[async_trait]
impl Harness for OpencodeServer {
    async fn run(&self, req: HarnessRequest) -> Result<HarnessOutcome, HarnessError> {
        tracing::info!(cwd = %req.cwd.display(), tools = ?req.tools, model = %format!("{}/{}", self.provider_id, self.model_id), "opencode serve");
        let server = Server::start(self, &req.cwd).await?;
        tracing::debug!(base = %server.base, "opencode serve healthy");
        let client = local_client()?;
        let http = |stage: HarnessStage| {
            move |e: reqwest::Error| HarnessError::Http {
                stage,
                status: e.status().map_or(0, |s| s.as_u16()),
                detail: e.to_string(),
            }
        };
        let created: Created = client
            .post(format!("{}/session", server.base))
            .json(&serde_json::json!({ "title": "factory" }))
            .send()
            .await
            .map_err(http(HarnessStage::Session))?
            .error_for_status()
            .map_err(http(HarnessStage::Session))?
            .json()
            .await
            .map_err(http(HarnessStage::Session))?;

        tracing::debug!(session = %created.id, "session created; sending prompt");
        let mut body = serde_json::json!({
            "model": { "providerID": self.provider_id, "modelID": self.model_id },
            "system": req.system_prompt,
            "tools": tools_for(req.tools),
            "parts": [ { "type": "text", "text": req.prompt } ],
        });
        if let Some(schema) = &req.schema {
            body["format"] =
                serde_json::json!({ "type": "json_schema", "schema": schema, "retryCount": 2 });
        }

        let limit = std::time::Duration::from_secs(req.timeout.seconds());
        let send = client
            .post(format!("{}/session/{}/message", server.base, created.id))
            .json(&body)
            .send();
        let resp = match tokio::time::timeout(limit, send).await {
            Ok(r) => r.map_err(http(HarnessStage::Prompt))?,
            Err(_elapsed) => {
                // fp-allow: best-effort abort; the timeout is the error the caller acts on
                let _ = client
                    .post(format!("{}/session/{}/abort", server.base, created.id))
                    .send()
                    .await;
                return Err(HarnessError::Timeout {
                    after: req.timeout,
                    stage: HarnessStage::Prompt,
                });
            }
        };
        let status = resp.status();
        tracing::debug!(%status, "prompt answered");
        let text = resp.text().await.map_err(http(HarnessStage::Prompt))?;
        if !status.is_success() {
            return Err(HarnessError::Http {
                stage: HarnessStage::Prompt,
                status: status.as_u16(),
                detail: text.chars().take(500).collect(),
            });
        }
        let msg: MessageResponse =
            serde_json::from_str(&text).map_err(|e| HarnessError::Decode {
                stage: HarnessStage::Envelope,
                detail: format!("{e}: {}", text.chars().take(300).collect::<String>()),
            })?;
        drop(server);
        Ok(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use app::domain::Duration;

    use super::*;

    #[test]
    fn decodes_message_response() {
        let json = r#"{"info":{"id":"msg","role":"assistant","tokens":{"total":9095,"input":9093,"output":2,"reasoning":0,"cache":{"write":0,"read":0}},"cost":0.0036396,"finish":"stop","modelID":"Qwen3.6-27B","providerID":"blueclaw"},
            "parts":[{"type":"step-start"},{"type":"text","text":"pong"},{"type":"step-finish","reason":"stop"}]}"#;
        let o: HarnessOutcome = serde_json::from_str::<MessageResponse>(json)
            .unwrap()
            .into();
        assert_eq!(o.text, "pong");
        assert_eq!(o.tokens, 9095);
        assert_eq!(o.cost_micro_usd, 3640);
        assert_eq!(o.turns, 1);
        assert!(!o.is_error);
        assert_eq!(o.structured, None);
    }

    #[test]
    fn decodes_error_and_structured() {
        let json = r#"{"info":{"role":"assistant","tokens":{"total":1},"cost":0,"error":{"name":"APIError","data":{"message":"no route"}},"structured":{"ok":true}},"parts":[]}"#;
        let o: HarnessOutcome = serde_json::from_str::<MessageResponse>(json)
            .unwrap()
            .into();
        assert!(o.is_error);
        assert_eq!(o.text, "no route");
        assert_eq!(o.structured, Some(serde_json::json!({"ok": true})));
    }

    #[test]
    fn tool_maps() {
        let none = tools_for(ToolPolicy::None);
        assert!(none.as_object().unwrap().values().all(|v| v == false));
        let full = tools_for(ToolPolicy::Full);
        assert_eq!(full["bash"], true);
        assert_eq!(full["webfetch"], false);
        assert_eq!(full["question"], false);
    }

    #[test]
    fn model_spec() {
        let h = OpencodeServer::from_model_spec("blueclaw/Qwen3.6-27B").unwrap();
        assert_eq!(
            (h.provider_id.as_str(), h.model_id.as_str()),
            ("blueclaw", "Qwen3.6-27B")
        );
        assert!(OpencodeServer::from_model_spec("nope").is_err());
    }

    #[test]
    fn merge_permission_keeps_extra() {
        let s = merge_permission(r#"{"provider":{"x":{}}}"#);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["permission"], "allow");
        assert!(v["provider"]["x"].is_object());
    }

    /// Needs a configured OpenCode provider on this machine; run with `--ignored`.
    #[tokio::test]
    #[ignore = "live: requires opencode + a configured provider"]
    async fn live_pong() {
        let h = OpencodeServer::from_model_spec(
            &std::env::var("FACTORY_OPENCODE_MODEL")
                .unwrap_or_else(|_| "blueclaw/Qwen3.6-27B".into()),
        )
        .unwrap();
        // A fresh empty dir: OpenCode treats cwd as the project and would scan all of /tmp.
        let cwd = std::env::temp_dir().join(format!("factory-oc-{}", std::process::id()));
        std::fs::create_dir_all(&cwd).unwrap();
        let o = h
            .run(HarnessRequest {
                cwd,
                system_prompt: "You are terse.".into(),
                prompt: "Reply with exactly: pong".into(),
                schema: None,
                tools: ToolPolicy::None,
                max_turns: 1,
                timeout: Duration::from_seconds(120),
            })
            .await
            .unwrap();
        assert!(!o.is_error, "{}", o.text);
        assert!(o.text.to_lowercase().contains("pong"));
        assert!(o.tokens > 0);
    }
}
