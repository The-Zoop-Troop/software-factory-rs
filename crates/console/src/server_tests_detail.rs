//! Detail endpoints over the router: bead detail, rig detail, epic context, consumers.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "tests: json! literals; one scenario per endpoint"
)]

use app::BeadStore as _;
use axum::http::StatusCode;
use domain::RigName;

use crate::server_tests::{get, id, state};

#[tokio::test]
async fn bead_detail_returns_task_fields_verify_commands_and_parsed_notes() {
    let (s, store, _tail) = state().await;
    store
        .seed_verify(id("v-1"), id("ep-1.1"), &["sh tests/run.sh"])
        .await;
    store
        .note(
            &id("ep-1.1"),
            "verify FAILED\n$ sh tests/run.sh\n[exit 1]\nboom\nguidance: fix it",
        )
        .await
        .expect("note");
    let (st, body) = get(&s, "/rigs/toy/beads/ep-1.1", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["kind"], "task");
    assert_eq!(body["task"]["branch"], "task/ep-1.1");
    assert_eq!(body["verify"]["commands"][0], "sh tests/run.sh");
    assert_eq!(body["notes"][0]["kind"], "verify_block");
    assert_eq!(body["notes"][0]["passed"], false);
    assert_eq!(body["notes"][1]["kind"], "guidance");
    let (st, _) = get(&s, "/rigs/toy/beads/nope-1", Some("watcher")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = get(&s, "/rigs/toy/beads/ep-1.1", Some("stranger")).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rig_detail_reports_facts_posture_rollup_and_budget() {
    let (mut s, _store, tail) = state().await;
    s.facts = std::sync::Arc::new(std::collections::BTreeMap::from([(
        "toy".to_owned(),
        crate::config::RigFacts {
            repo_url: Some("https://x/y.git".into()),
            runtime: Some("web-e2e".into()),
            harness: Some("codex".into()),
            main: Some("feat/z".into()),
        },
    )]));
    tail.push("planner", Some(id("ep-1.1")), "task_planned");
    tail.push("worker", Some(id("ep-1.1")), "claimed");
    let (st, body) = get(&s, "/rigs/toy/detail", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["facts"]["runtime"], "web-e2e");
    assert_eq!(body["posture"], "available");
    assert_eq!(body["events"]["count"], 2);
    assert_eq!(body["rollup"]["epics"], 1);
    assert!(body["budget"].is_object());
    let (st, _) = get(&s, "/rigs/toy/detail", Some("stranger")).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn epic_bead_detail_carries_context_beads_and_originating_request() {
    let (s, store, _tail) = state().await;
    store
        .seed_reference(id("ep-1.0"), id("ep-1"), "Use POSIX sh.")
        .await;
    let mut contract = app::plan_request("range abc..def; surface: GET /x", "steward");
    contract.kind = app::domain::BeadKind::Contract;
    contract.parent = Some(id("ep-1"));
    let cid = store.create(contract).await.expect("contract");
    store.close(&cid, "artifact").await.expect("close contract");
    let origin = store
        .create(app::plan_request("Build the passthrough", "phone"))
        .await
        .expect("request");
    store.note(&origin, "epic ep-1").await.expect("note");
    store.close(&origin, "epic ep-1").await.expect("close");
    let (st, body) = get(&s, "/rigs/toy/beads/ep-1", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    let kinds: Vec<&str> = body["context"]
        .as_array()
        .expect("context")
        .iter()
        .filter_map(|c| c["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"reference") && kinds.contains(&"contract"));
    assert_eq!(body["origin"]["title"], "Build the passthrough");
    // A task bead is not an epic: no context, no origin.
    let (_, task) = get(&s, "/rigs/toy/beads/ep-1.1", Some("watcher")).await;
    assert!(task["context"].is_null() && task["origin"].is_null());
}

#[tokio::test]
async fn epic_consumers_lists_requests_that_name_it_across_statuses() {
    let (s, store, _tail) = state().await;
    let req = store
        .create(app::plan_request("Portal after the backend", "phone"))
        .await
        .expect("request");
    let need = app::domain::CrossRigNeed {
        rig: RigName::try_new("toy").expect("rig"),
        epic: id("ep-1"),
    };
    store.set_needs(&req, &[need]).await.expect("needs");
    let (st, body) = get(&s, "/rigs/toy/epics/ep-1/consumers", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    let rows = body["consumers"].as_array().expect("consumers");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["rig"], "toy");
    assert_eq!(rows[0]["status"], "open");
    let (st, other) = get(&s, "/rigs/toy/epics/ep-9/consumers", Some("watcher")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(other["consumers"].as_array().is_some_and(Vec::is_empty));
    let (st, _) = get(&s, "/rigs/toy/epics/ep-1/consumers", Some("stranger")).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}
