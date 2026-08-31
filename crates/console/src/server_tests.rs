//! End-to-end over the router with the app fakes: cards, auth, every RPC, and SSE.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "tests: json! literals; one scenario per endpoint"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use app::BeadStore as _;
use app::testing::FixedClock;
use app::testing::remote::{FakeAuth, FakePlanner, FakeRegistry, rig};
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use domain::{
    Attempts, BeadId, Budget, ClientId, Duration, FactoryMeta, Principal, RigName, Scope, Sha,
    TaskState, Timestamp, Tokens, Usage,
};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;

use crate::server::{AppState, agent_card, router};

pub(crate) fn id(s: &str) -> BeadId {
    BeadId::try_new(s).expect("id")
}

fn grant(rig: &str, scopes: &[Scope]) -> Principal {
    Principal {
        client: ClientId::try_new("phone").expect("c"),
        grants: BTreeMap::from([(
            RigName::try_new(rig).expect("r"),
            scopes.iter().copied().collect::<BTreeSet<_>>(),
        )]),
    }
}

pub(crate) fn open_meta() -> FactoryMeta {
    FactoryMeta {
        verify_bead: id("v-1"),
        base: Sha::try_new("0".repeat(40)).expect("sha"),
        budget: Budget {
            tokens: Tokens::new(10),
            wall_clock: Duration::from_minutes(1),
            attempts: Attempts::new(1),
        },
        usage: Usage {
            tokens: Tokens::new(0),
            wall_clock: Duration::from_minutes(0),
            attempts: Attempts::new(0),
        },
        lease_expiries: Attempts::new(0),
        state: TaskState::Open,
    }
}

pub(crate) async fn state() -> (
    AppState,
    Arc<app::testing::FakeStore>,
    Arc<app::testing::remote::FakeTail>,
) {
    let (r, store, _, tail) = rig("toy", FakePlanner::returning("ep-1"));
    store.seed_epic(id("ep-1"), &[]).await;
    store.seed_task(id("ep-1.1"), open_meta()).await;
    store.set_parent(&id("ep-1.1"), &id("ep-1")).await;
    let registry = FakeRegistry(BTreeMap::from([(r.name.clone(), r)]));
    let auth = FakeAuth(BTreeMap::from([
        ("admin".to_owned(), grant("toy", &[Scope::Admin])),
        ("watcher".to_owned(), grant("toy", &[Scope::Watch])),
        ("stranger".to_owned(), grant("other", &[Scope::Admin])),
    ]));
    let s = AppState {
        auth: Arc::new(auth),
        registry: Arc::new(registry),
        clock: Arc::new(FixedClock(Timestamp::from_unix_seconds(1_700_000_000))),
        public_url: "http://console.test".into(),
        poll: std::time::Duration::ZERO,
    };
    (s, store, tail)
}

pub(crate) async fn call(
    s: &AppState,
    token: Option<&str>,
    method: &str,
    params: Value,
) -> (StatusCode, Value) {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let mut req = Request::post("/rigs/toy/a2a").header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let resp = router(s.clone())
        .oneshot(req.body(Body::from(body.to_string())).expect("req"))
        .await
        .expect("resp");
    let status = resp.status();
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

pub(crate) async fn get(s: &AppState, path: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::get(path);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let resp = router(s.clone())
        .oneshot(req.body(Body::empty()).expect("req"))
        .await
        .expect("resp");
    let status = resp.status();
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn cards_are_public_and_rig_listing_is_scoped() {
    let (s, _, _) = state().await;
    let (st, root) = get(&s, "/.well-known/agent-card.json", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        root["rigs"][0],
        "http://console.test/rigs/toy/.well-known/agent-card.json"
    );
    let (st, card) = get(&s, "/rigs/toy/.well-known/agent-card.json", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        card["supportedInterfaces"][0]["url"],
        "http://console.test/rigs/toy/a2a"
    );
    assert_eq!(card["skills"].as_array().map(Vec::len), Some(5));
    assert_eq!(
        card,
        crate::rpc::val(&agent_card(
            "http://console.test",
            &RigName::try_new("toy").expect("r")
        ))
    );
    assert_eq!(
        get(&s, "/rigs/nope/.well-known/agent-card.json", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&s, "/rigs/BAD!/.well-known/agent-card.json", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(get(&s, "/rigs", None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(&s, "/rigs", Some("watcher")).await.1["rigs"],
        json!(["toy"])
    );
    assert_eq!(
        get(&s, "/rigs", Some("stranger")).await.1["rigs"],
        json!([])
    );
    assert!(format!("{s:?}").contains("console.test"));
}

#[tokio::test]
async fn auth_and_request_shape_errors() {
    let (s, _, _) = state().await;
    assert_eq!(
        call(&s, None, "ListTasks", Value::Null).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&s, Some("bogus"), "ListTasks", Value::Null).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (st, body) = call(&s, Some("watcher"), "CancelTask", json!({"id": "ep-1"})).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], crate::rpc::FORBIDDEN);
    let (st, body) = call(&s, Some("admin"), "Nope", Value::Null).await;
    assert_eq!(
        (st, body["error"]["code"].as_i64()),
        (
            StatusCode::BAD_REQUEST,
            Some(i64::from(crate::rpc::METHOD_NOT_FOUND))
        )
    );
    assert_eq!(
        call(&s, Some("admin"), "GetTask", json!({})).await.0,
        StatusCode::BAD_REQUEST
    );
    let (st, body) = call(&s, Some("stranger"), "ListTasks", Value::Null).await;
    assert_eq!(
        (st, body["error"]["code"].as_i64()),
        (
            StatusCode::FORBIDDEN,
            Some(i64::from(crate::rpc::FORBIDDEN))
        )
    );
    // Unknown rig and malformed JSON.
    let resp = router(s.clone())
        .oneshot(
            Request::post("/rigs/zzz/a2a")
                .header(header::AUTHORIZATION, "Bearer admin")
                .body(Body::from(
                    r#"{"jsonrpc":"2.0","id":3,"method":"ListTasks"}"#,
                ))
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let resp = router(s.clone())
        .oneshot(
            Request::post("/rigs/toy/a2a")
                .header(header::AUTHORIZATION, "Bearer admin")
                .body(Body::from("{nope"))
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    let v: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["error"]["code"], -32700);
}

#[tokio::test]
async fn list_get_send_cancel_roundtrip() {
    let (s, store, _) = state().await;
    let (st, body) = call(&s, Some("watcher"), "ListTasks", Value::Null).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["result"]["tasks"][0]["id"], "ep-1");
    assert_eq!(
        body["result"]["tasks"][0]["status"]["state"],
        "TASK_STATE_SUBMITTED"
    );
    let (_, body) = call(
        &s,
        Some("watcher"),
        "ListTasks",
        json!({"status": "TASK_STATE_INPUT_REQUIRED"}),
    )
    .await;
    assert_eq!(body["result"]["tasks"], json!([]));
    let (_, body) = call(&s, Some("watcher"), "GetTask", json!({"id": "ep-1"})).await;
    assert_eq!(body["result"]["contextId"], "ep-1");
    let (st, body) = call(&s, Some("watcher"), "GetTask", json!({"id": "ep-9"})).await;
    assert_eq!(
        (st, body["error"]["code"].as_i64()),
        (
            StatusCode::NOT_FOUND,
            Some(i64::from(crate::rpc::TASK_NOT_FOUND))
        )
    );
    let msg = json!({"message": {"messageId": "m1", "role": "ROLE_USER", "parts": [{"text": "build it"}]}});
    let (st, body) = call(&s, Some("admin"), "SendMessage", msg).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["result"]["task"]["id"], "ep-1");
    let empty =
        json!({"message": {"messageId": "m2", "role": "ROLE_USER", "parts": [{"data": {}}]}});
    assert_eq!(
        call(&s, Some("admin"), "SendMessage", empty).await.0,
        StatusCode::BAD_REQUEST
    );
    let (st, body) = call(&s, Some("admin"), "CancelTask", json!({"id": "ep-1"})).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["result"]["status"]["state"], "TASK_STATE_CANCELED");
    assert_eq!(
        store.show(&id("ep-1.1")).await.expect("b").status,
        app::BeadStatus::Closed
    );
    let (st, body) = call(&s, Some("admin"), "CancelTask", json!({"id": "ep-1"})).await;
    assert_eq!(
        (st, body["error"]["code"].as_i64()),
        (StatusCode::OK, Some(i64::from(crate::rpc::NOT_CANCELABLE)))
    );
}

async fn sse_frames(s: &AppState, token: &str, task: &str, max: usize) -> (StatusCode, Vec<Value>) {
    let body =
        json!({ "jsonrpc": "2.0", "id": 9, "method": "SubscribeToTask", "params": { "id": task } });
    let resp = router(s.clone())
        .oneshot(
            Request::post("/rigs/toy/a2a")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(body.to_string()))
                .expect("req"),
        )
        .await
        .expect("resp");
    let status = resp.status();
    let mut body = resp.into_body();
    let mut out = Vec::new();
    while out.len() < max {
        let Some(Ok(frame)) = body.frame().await else {
            break;
        };
        let Some(data) = frame.data_ref() else {
            continue;
        };
        for line in String::from_utf8_lossy(data).lines() {
            if let Some(json) = line.strip_prefix("data:") {
                out.push(serde_json::from_str(json.trim()).expect("frame json"));
            }
        }
    }
    (status, out)
}

#[tokio::test]
async fn subscribe_streams_task_then_context_events_until_terminal() {
    let (s, _, tail) = state().await;
    tail.push("worker", Some(id("ep-1.1")), "claimed");
    tail.push("worker", Some(id("zz-1")), "claimed");
    let (st, frames) = sse_frames(&s, "watcher", "ep-1", 2).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(frames[0]["result"]["task"]["id"], "ep-1");
    assert_eq!(
        frames[1]["result"]["statusUpdate"]["metadata"]["event"]["kind"],
        "claimed"
    );
    assert_eq!(frames[1]["result"]["statusUpdate"]["final"], false);
    // Terminal task: the initial Task and nothing more.
    let _ = call(&s, Some("admin"), "CancelTask", json!({"id": "ep-1"})).await;
    let (_, frames) = sse_frames(&s, "watcher", "ep-1", 5).await;
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0]["result"]["task"]["status"]["state"],
        "TASK_STATE_CANCELED"
    );
    let (st, frames) = sse_frames(&s, "watcher", "nope", 1).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(frames.len(), 0);
    let (st, _) = sse_frames(&s, "stranger", "ep-1", 1).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn subscribe_reports_final_frame_when_task_becomes_terminal_and_tail_errors() {
    let (s, store, tail) = state().await;
    // Working task with an event, then it gets canceled between polls: expect a final frame.
    tail.push("worker", Some(id("ep-1.1")), "claimed");
    let (r, ..) = rig("toy", FakePlanner::returning("ep-1"));
    let failing = app::Rig {
        events: Arc::new(app::testing::remote::FakeTail {
            fail: true,
            ..Default::default()
        }),
        store: store.clone(),
        ..r
    };
    let s2 = AppState {
        registry: Arc::new(FakeRegistry(BTreeMap::from([(
            failing.name.clone(),
            failing,
        )]))),
        ..s.clone()
    };
    let (_, frames) = sse_frames(&s2, "watcher", "ep-1", 3).await;
    assert_eq!(frames.len(), 2);
    assert!(frames[1]["error"]["code"].is_number());
    store.close(&id("ep-1"), "done").await.expect("closed");
    let (_, frames) = sse_frames(&s, "watcher", "ep-1", 1).await;
    assert_eq!(
        frames[0]["result"]["task"]["status"]["state"],
        "TASK_STATE_COMPLETED"
    );
}

#[tokio::test]
async fn history_flag_lists_closed_epics_and_epic_events_replays_the_log() {
    let (s, store, tail) = state().await;
    store.seed_epic(id("ep-0"), &[]).await;
    store.close(&id("ep-0"), "done").await.expect("close");
    let (_, body) = call(&s, Some("watcher"), "ListTasks", json!({"history": true})).await;
    let ids: Vec<_> = body["result"]["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|t| t["id"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(ids, ["ep-0"]);
    assert_eq!(
        body["result"]["tasks"][0]["status"]["state"],
        "TASK_STATE_COMPLETED"
    );

    tail.push("planner", Some(id("ep-0")), "task_planned");
    tail.push("worker", Some(id("ep-0.1")), "claimed");
    tail.push("worker", Some(id("ep-1.1")), "claimed");
    tail.push("stewardd", None, "sweep_done");
    let (st, body) = get(&s, "/rigs/toy/epics/ep-0/events", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    let kinds: Vec<_> = body["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|e| e["kind"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        kinds,
        ["task_planned", "claimed"],
        "the epic and its children only"
    );
    let (st, _) = get(&s, "/rigs/toy/epics/ep-0/events", Some("stranger")).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = get(&s, "/rigs/nope/epics/ep-0/events", Some("watcher")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn metrics_reports_every_epic_in_the_log_or_one() {
    let (s, _store, tail) = state().await;
    tail.push("planner", Some(id("ep-1.1")), "task_planned");
    tail.push("worker", Some(id("ep-1.1")), "claimed");
    tail.push("worker", Some(id("zz-2.1")), "claimed");
    let (st, body) = get(&s, "/rigs/toy/metrics", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    let ids: Vec<_> = body["epics"]
        .as_array()
        .expect("epics")
        .iter()
        .map(|e| e["epic"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(ids, ["ep-1", "zz-2"]);
    let (_, body) = get(&s, "/rigs/toy/metrics?epic=ep-1", Some("watcher")).await;
    assert_eq!(body["epics"].as_array().map(Vec::len), Some(1));
    // The fake tail stamps records with ISO text; the fold keys on unix seconds, so the shape
    // is asserted here and the numbers in app::metrics_tests.
    assert!(body["epics"][0]["tasks"].is_array());
    assert_eq!(body["epics"][0]["epic"], "ep-1");
    let (st, _) = get(&s, "/rigs/toy/metrics", Some("stranger")).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_rig_that_cannot_answer_is_refused_cheaply_and_deep_links_serve_the_app() {
    let (s, _store, _tail) = state().await;
    let rig = s
        .registry
        .rig(&RigName::try_new("toy").expect("r"))
        .expect("rig");
    let probe = rig.probe.clone();
    let down = probe
        .as_any()
        .downcast_ref::<app::testing::remote::FakeProbe>()
        .expect("fake probe");
    if let Ok(mut d) = down.down.lock() {
        *d = Some("no ledger yet: the rig has never run".to_owned());
    }
    let (st, body) = call(&s, Some("watcher"), "ListTasks", json!({})).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], -32001);
    let (st, body) = get(&s, "/rigs", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["overview"][0]["unavailable"], true);
    assert!(
        body["overview"][0]["error"]
            .as_str()
            .unwrap_or("")
            .contains("never run")
    );
    for path in ["/rigs/toy/epics/ep-1", "/rigs/toy/epics/ep-1/throughput"] {
        let resp = router(s.clone())
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), StatusCode::OK, "{path}");
    }
}

#[tokio::test]
async fn state_changing_events_carry_a_task_update_frame_and_noise_does_not() {
    let (s, _store, _tail) = state().await;
    let rig = s
        .registry
        .rig(&RigName::try_new("toy").expect("r"))
        .expect("rig");
    let who = grant("toy", &[Scope::Watch]);
    let clock = FixedClock(Timestamp::from_unix_seconds(1));
    let rec = |bead: Option<&str>, kind: &str| app::remote::EventRecord {
        at: "1".into(),
        actor: "w".into(),
        bead: bead.map(|b| app::domain::BeadId::try_new(b).expect("id")),
        kind: kind.into(),
        detail: serde_json::Map::new(),
    };
    let frame = crate::server::server_rigs_test_hook_inner(
        &rig,
        &clock,
        &who,
        7,
        &rec(Some("ep-1.1"), "claimed"),
    )
    .await;
    let text = format!("{frame:?}");
    assert!(
        text.contains("task_update") && text.contains("ep-1"),
        "{text}"
    );
    assert!(
        crate::server::server_rigs_test_hook_inner(
            &rig,
            &clock,
            &who,
            7,
            &rec(Some("ep-1.1"), "sweep_done")
        )
        .await
        .is_none()
    );
    assert!(
        crate::server::server_rigs_test_hook_inner(
            &rig,
            &clock,
            &who,
            7,
            &rec(Some("ep-1.1"), "progress")
        )
        .await
        .is_none()
    );
    assert!(
        crate::server::server_rigs_test_hook_inner(&rig, &clock, &who, 7, &rec(None, "claimed"))
            .await
            .is_none()
    );
}
