//! Web console and attention endpoints over the router with the app fakes.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "tests: json! literals; one scenario per endpoint"
)]

use app::BeadStore as _;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use domain::{FactoryMeta, TaskState};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;

use crate::server::{AppState, router};
use crate::server_tests::{call, get, id, open_meta, state};

#[tokio::test]
async fn web_console_page_surface_and_actions() {
    let (s, store, _) = state().await;
    let (st, _) = get(&s, "/", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        get(&s, "/rigs/toy/ui", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&s, "/rigs/nope/ui", Some("watcher")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&s, "/rigs/toy/ui", Some("stranger")).await.0,
        StatusCode::FORBIDDEN
    );
    let (st, env) = get(&s, "/rigs/toy/ui", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(env[0]["createSurface"]["surfaceId"], "console");
    let ids: Vec<String> = env[1]["updateComponents"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(ids.contains(&"ep-1_stop".to_owned()));
    let card = get(&s, "/rigs/toy/.well-known/agent-card.json", None)
        .await
        .1;
    assert_eq!(
        card["capabilities"]["extensions"][0]["uri"],
        app::remote::a2ui::EXTENSION
    );

    let post = |token: &'static str, body: Value| {
        let s = s.clone();
        async move {
            let mut req = Request::post("/rigs/toy/ui/action")
                .header(header::CONTENT_TYPE, "application/json");
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
            let resp = router(s)
                .oneshot(req.body(Body::from(body.to_string())).expect("req"))
                .await
                .expect("resp");
            let status = resp.status();
            let bytes = resp.into_body().collect().await.expect("body").to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };
    assert_eq!(
        post("admin", json!({"name": "refresh"})).await.0,
        StatusCode::OK
    );
    assert_eq!(
        post("admin", json!({"name": "dance"})).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post("admin", json!({"nope": 1})).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(
            "watcher",
            json!({"name": "plan", "context": {"text": "go"}})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (st, env) = post("admin", json!({"name": "plan", "context": {"text": "go"}})).await;
    assert_eq!(st, StatusCode::OK);
    assert!(env.is_array());
    let (st, env) = post("admin", json!({"name": "stop", "context": {"id": "ep-1"}})).await;
    assert_eq!(st, StatusCode::OK);
    let ids: Vec<String> = env[1]["updateComponents"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(!ids.contains(&"ep-1_stop".to_owned()));
    assert_eq!(
        store.show(&id("ep-1.1")).await.expect("b").status,
        app::BeadStatus::Closed
    );
    let resp = router(s.clone())
        .oneshot(
            Request::post("/rigs/zzz/ui/action")
                .header(header::AUTHORIZATION, "Bearer admin")
                .body(Body::from("{}"))
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn whoami_rig_counts_and_attention_options() {
    let (s, store, _) = state().await;
    let (st, me) = get(&s, "/whoami", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["client"], "phone");
    assert_eq!(me["grants"][0]["rig"], "toy");
    assert_eq!(me["grants"][0]["scopes"], json!(["watch"]));
    assert_eq!(get(&s, "/whoami", None).await.0, StatusCode::UNAUTHORIZED);
    let (_, rigs) = get(&s, "/rigs", Some("watcher")).await;
    assert_eq!(rigs["overview"][0]["epics"], 1);
    assert_eq!(rigs["overview"][0]["attention"], 0);
    // An incident on a task under the epic surfaces with evidence and options.
    store
        .seed_task(
            id("ep-1.2"),
            FactoryMeta {
                state: TaskState::Incident {
                    reason: app::domain::task::IncidentReason::MergeConflict {
                        detail: "lib.sh".into(),
                    },
                },
                ..open_meta()
            },
        )
        .await;
    store.set_parent(&id("ep-1.2"), &id("ep-1")).await;
    let inc = store
        .create(app::NewBead {
            title: app::domain::Title::try_new("incident on ep-1.2").expect("t"),
            description: "no merge".into(),
            kind: app::domain::BeadKind::Incident,
            priority: app::domain::Priority::CRITICAL,
            parent: Some(id("ep-1")),
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await
        .expect("inc");
    let (_, body) = call(
        &s,
        Some("watcher"),
        "GetTask",
        json!({"id": inc.to_string()}),
    )
    .await;
    let parts = body["result"]["status"]["message"]["parts"]
        .as_array()
        .unwrap();
    let data = parts.iter().find_map(|p| p.get("data")).unwrap();
    assert_eq!(data["reason"]["kind"], "merge_conflict");
    assert_eq!(data["options"].as_array().unwrap().len(), 4);
    // Applying an option over A2A with a data part.
    let msg = json!({"message": {"messageId": "m", "role": "ROLE_USER", "parts": [{"data": {"option": "retry_with_guidance"}}, {"text": "keep it POSIX"}], "taskId": inc.to_string()}});
    let (st, body) = call(&s, Some("admin"), "SendMessage", msg).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["task"]["status"]["state"],
        "TASK_STATE_COMPLETED"
    );
    assert!(
        store
            .show(&id("ep-1.2"))
            .await
            .unwrap()
            .notes
            .unwrap()
            .contains("guidance: keep it POSIX")
    );
    // And through the UI action endpoint.
    let inc2 = store
        .create(app::NewBead {
            title: app::domain::Title::try_new("incident on ep-1.2").expect("t"),
            description: "again".into(),
            kind: app::domain::BeadKind::Incident,
            priority: app::domain::Priority::CRITICAL,
            parent: Some(id("ep-1")),
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await
        .expect("inc");
    let post = |token: &'static str, body: Value| {
        let s = s.clone();
        async move {
            let req = Request::post("/rigs/toy/ui/action")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"));
            let resp = router(s)
                .oneshot(req.body(Body::from(body.to_string())).expect("req"))
                .await
                .expect("resp");
            let status = resp.status();
            let bytes = resp.into_body().collect().await.expect("body").to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };
    assert_eq!(
        post(
            "admin",
            json!({"name": "option", "context": {"id": inc2.to_string(), "option": "bogus"}})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(
            "admin",
            json!({"name": "option", "context": {"id": inc2.to_string()}})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(
            "watcher",
            json!({"name": "option", "context": {"id": inc2.to_string(), "option": "stop_epic"}})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (st, env) = post(
        "admin",
        json!({"name": "option", "context": {"id": inc2.to_string(), "option": "stop_epic"}}),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(env.is_array());
    assert_eq!(
        store.show(&id("ep-1")).await.unwrap().status,
        app::BeadStatus::Closed
    );
}

/// Status without reading the (possibly endless) body.
async fn status_only(s: &AppState, path: &str) -> StatusCode {
    router(s.clone())
        .oneshot(Request::get(path).body(Body::empty()).expect("req"))
        .await
        .expect("resp")
        .status()
}

async fn first_frames(
    s: &AppState,
    path: &str,
    token: &str,
    max: usize,
) -> (StatusCode, Vec<String>) {
    let resp = router(s.clone())
        .oneshot(
            Request::get(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
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
        if let Some(data) = frame.data_ref() {
            out.extend(
                String::from_utf8_lossy(data)
                    .lines()
                    .filter(|l| l.starts_with("data:"))
                    .map(|l| l.trim_start_matches("data:").trim().to_owned()),
            );
        }
    }
    (status, out)
}

#[tokio::test]
async fn rig_and_fan_in_event_streams() {
    let (s, _, tail) = state().await;
    tail.push("worker", Some(id("ep-1.1")), "claimed");
    tail.push("verifier", Some(id("ep-1.1")), "verified");
    assert_eq!(
        get(&s, "/rigs/toy/events", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&s, "/rigs/nope/events", Some("watcher")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&s, "/rigs/toy/events", Some("stranger")).await.0,
        StatusCode::FORBIDDEN
    );
    let (st, frames) = first_frames(&s, "/rigs/toy/events?cursor=0", "watcher", 2).await;
    assert_eq!(st, StatusCode::OK);
    let first: Value = serde_json::from_str(&frames[0]).unwrap();
    assert_eq!(first["rig"], "toy");
    assert_eq!(first["record"]["kind"], "claimed");
    assert_eq!(first["cursor"], 2);
    assert_eq!(first["replay"], false);
    // Fan-in over every visible rig, from a cursor.
    tail.push("integrator", Some(id("ep-1.1")), "integrated");
    let (st, frames) = first_frames(&s, "/events?cursor=2", "watcher", 1).await;
    assert_eq!(st, StatusCode::OK);
    let f: Value = serde_json::from_str(&frames[0]).unwrap();
    assert_eq!(f["record"]["kind"], "integrated");
    assert_eq!(status_only(&s, "/events").await, StatusCode::UNAUTHORIZED);
    // Browsers cannot set headers on EventSource: the token may ride in the query string.
    assert_eq!(
        status_only(&s, "/events?token=watcher").await,
        StatusCode::OK
    );
    assert_eq!(
        status_only(&s, "/rigs/toy/events?token=nope").await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn return_immediately_queues_a_plan_request() {
    let (s, store, _) = state().await;
    let msg = json!({"message": {"messageId": "m", "role": "ROLE_USER", "parts": [{"text": "build it later"}]}, "configuration": {"returnImmediately": true}});
    let (st, body) = call(&s, Some("admin"), "SendMessage", msg).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(
        body["result"]["task"]["status"]["state"],
        "TASK_STATE_SUBMITTED"
    );
    assert_eq!(
        body["result"]["task"]["metadata"]["factory"]["kind"],
        "plan_request"
    );
    let reqs = store
        .ready(app::domain::BeadKind::PlanRequest)
        .await
        .unwrap();
    assert_eq!(reqs.len(), 1);
    let (_, listed) = call(&s, Some("watcher"), "ListTasks", Value::Null).await;
    assert!(
        listed["result"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["metadata"]["factory"]["kind"] == "plan_request")
    );
}
