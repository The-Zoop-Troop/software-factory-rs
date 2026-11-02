//! OpenCode adapter tests: response decoding and config, plus a live test run with `--ignored`.

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
    assert_eq!(o.tokens.get(), 9095);
    assert_eq!(o.cost_micro_usd.get(), 3640);
    assert_eq!(o.turns.get(), 1);
    assert!(!o.is_error);
    assert_eq!(o.structured, None);
}

/// Shape as OpenCode 1.18.34 returned it against a stub provider answering each status
/// (docs/references/opencode-server.md). A real limit has not been observed.
#[test]
fn captured_refusals_are_classified() {
    let decode = |status: u64, retryable: bool| {
        let json = format!(
            r#"{{"info":{{"role":"assistant","tokens":{{"total":0}},"cost":0,"finish":null,"error":{{"name":"APIError","data":{{"message":"m","statusCode":{status},"isRetryable":{retryable},"responseHeaders":{{}},"responseBody":"{{}}"}}}}}},"parts":[]}}"#
        );
        let o = HarnessOutcome::from(serde_json::from_str::<MessageResponse>(&json).unwrap());
        assert!(o.is_error);
        o.provider_stop
    };
    assert_eq!(decode(429, true), Some(ProviderStop::RateLimit));
    assert_eq!(decode(402, false), Some(ProviderStop::SpendLimit));
    assert_eq!(decode(500, true), Some(ProviderStop::Outage));
    assert_eq!(decode(401, false), None);
    assert_eq!(decode(400, false), None);
    // An error with no status (a local failure) is not a provider refusal.
    let json = r#"{"info":{"role":"assistant","tokens":{"total":1},"cost":0,"error":{"name":"APIError","data":{"message":"no route"}}},"parts":[]}"#;
    let o = HarnessOutcome::from(serde_json::from_str::<MessageResponse>(json).unwrap());
    assert_eq!(o.provider_stop, None);
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
fn merge_mcp_adds_servers() {
    let mcp =
        app::McpConfig::parse(r#"{"mcpServers":{"fs":{"command":"npx","args":["x"]}}}"#).unwrap();
    let out = merge_mcp(r#"{"permission":"allow"}"#, &mcp);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["mcp"]["fs"]["type"], "local");
    assert_eq!(v["permission"], "allow");
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
        &std::env::var("FACTORY_OPENCODE_MODEL").unwrap_or_else(|_| "blueclaw/Qwen3.6-27B".into()),
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
            mcp: app::McpConfig::default(),
            max_turns: Turns::new(1),
            timeout: Duration::from_seconds(120),
            effort: None,
        })
        .await
        .unwrap();
    assert!(!o.is_error, "{}", o.text);
    assert!(o.text.to_lowercase().contains("pong"));
    assert!(o.tokens.get() > 0);
}
