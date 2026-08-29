//! Adapter tests against fake `claude`/`codex` binaries (shell scripts in `tests/fakebin`).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

use std::path::PathBuf;

use infra::app::domain::Duration;
use infra::app::{Harness, HarnessError, HarnessRequest, ToolPolicy};
use infra::{ClaudeCli, CodexCli};

fn fakebin(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fakebin")
        .join(name)
}

fn req(prompt: &str, tools: ToolPolicy, schema: bool) -> HarnessRequest {
    let cwd = std::env::temp_dir().join(format!(
        "factory-fake-{}-{}",
        std::process::id(),
        prompt.len()
    ));
    std::fs::create_dir_all(&cwd).unwrap();
    HarnessRequest {
        cwd,
        system_prompt: "sys".into(),
        prompt: prompt.into(),
        schema: schema.then(|| serde_json::json!({"type":"object"})),
        tools,
        max_turns: 3,
        timeout: Duration::from_seconds(2),
    }
}

#[tokio::test]
async fn claude_text_structured_error_timeout_garbage() {
    let h = ClaudeCli::default()
        .with_bin(fakebin("claude"))
        .with_model("m")
        .with_max_budget_usd(1.0);
    let o = h
        .run(req("pong please", ToolPolicy::None, false))
        .await
        .unwrap();
    assert_eq!((o.text.as_str(), o.tokens, o.is_error), ("pong", 4, false));
    let o = h
        .run(req("plan", ToolPolicy::ReadOnly, true))
        .await
        .unwrap();
    assert_eq!(o.structured, Some(serde_json::json!({"ok": true})));
    assert_eq!(o.cost_micro_usd, 10_000);
    let o = h.run(req("ERROR", ToolPolicy::Full, false)).await.unwrap();
    assert!(o.is_error);
    assert!(matches!(
        h.run(req("SLOW", ToolPolicy::None, false)).await,
        Err(HarnessError::Timeout(2))
    ));
    assert!(matches!(
        h.run(req("GARBAGE", ToolPolicy::None, false)).await,
        Err(HarnessError::Decode(_))
    ));
    let missing = ClaudeCli::default().with_bin("/nonexistent/claude");
    assert!(matches!(
        missing.run(req("x", ToolPolicy::None, false)).await,
        Err(HarnessError::Spawn(_))
    ));
}

#[tokio::test]
async fn codex_text_structured_failure() {
    let h = CodexCli::default()
        .with_bin(fakebin("codex"))
        .with_model("gpt");
    let o = h
        .run(req("pong please", ToolPolicy::None, false))
        .await
        .unwrap();
    assert_eq!(
        (o.text.as_str(), o.tokens, o.turns, o.is_error),
        ("pong", 125, 1, false)
    );
    let o = h
        .run(req("structured", ToolPolicy::Full, true))
        .await
        .unwrap();
    assert_eq!(o.structured, Some(serde_json::json!({"ok": true, "n": 2})));
    let o = h
        .run(req("FAIL", ToolPolicy::ReadOnly, false))
        .await
        .unwrap();
    assert!(o.is_error);
    assert_eq!(o.text, "rate limited");
    let missing = CodexCli::default().with_bin("/nonexistent/codex");
    assert!(matches!(
        missing.run(req("x", ToolPolicy::None, false)).await,
        Err(HarnessError::Spawn(_))
    ));
}

#[tokio::test]
async fn opencode_server_paths() {
    use infra::OpencodeServer;
    let h = OpencodeServer::from_model_spec("prov/model")
        .unwrap()
        .with_bin(fakebin("opencode"))
        .with_config_content(r#"{"x":1}"#);
    let o = h
        .run(req("pong please", ToolPolicy::Full, false))
        .await
        .unwrap();
    assert_eq!(
        (o.text.as_str(), o.tokens, o.cost_micro_usd, o.is_error),
        ("pong", 42, 1000, false)
    );
    let o = h
        .run(req("structured", ToolPolicy::None, true))
        .await
        .unwrap();
    assert_eq!(o.structured, Some(serde_json::json!({"ok": true})));
    let o = h
        .run(req("ERROR", ToolPolicy::ReadOnly, false))
        .await
        .unwrap();
    assert!(o.is_error && o.text == "no route");
    assert!(matches!(
        h.run(req("HTTP500", ToolPolicy::None, false)).await,
        Err(HarnessError::Decode(_))
    ));
    assert!(matches!(
        h.run(req("SLOW", ToolPolicy::None, false)).await,
        Err(HarnessError::Timeout(2))
    ));
    let missing = OpencodeServer::new("p", "m").with_bin("/nonexistent/opencode");
    assert!(matches!(
        missing.run(req("x", ToolPolicy::None, false)).await,
        Err(HarnessError::Spawn(_))
    ));
}
