#![allow(
    clippy::disallowed_types,
    reason = "tests: a leaf std Mutex for the fake planner"
)]

use domain::{
    Attempts, BeadId, BeadKind, MicroUsd, RigBudget, RigBudgetExceeded, Scope, TaskState, Tokens,
};

use super::a2a::{A2aState, Message, Part, epic_progress, skills};
use super::remote_fixtures_tests::{clock, id, meta, seeded, who};
use super::service::{
    RemoteError, Sent, cancel_task, enqueue_plan, events_after, get_task, list_tasks,
    list_tasks_with_vanished, send_message, spend,
};
use super::{Rig, SubmitError};
use crate::testing::remote::{FakePlanner, FakeRegistry, FakeTail, rig};
use crate::{BeadStore, EventKind, RigRegistry};

fn planned(s: Sent) -> Option<super::a2a::Task> {
    match s {
        Sent::Planned(t) => Some(t),
        Sent::Resolved { .. } => None,
    }
}

fn resolved(s: Sent) -> Option<(super::a2a::Task, Option<BeadId>)> {
    match s {
        Sent::Resolved { task, reopened } => Some((task, reopened)),
        Sent::Planned(_) => None,
    }
}

#[tokio::test]
async fn history_lists_closed_epics_only() {
    let (rig, store, _, _) = seeded().await;
    store.seed_epic(id("old-1"), &[]).await;
    store.close(&id("old-1"), "done").await.unwrap();
    let live = list_tasks(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .unwrap();
    assert!(
        live.iter().all(|t| t.id != "old-1"),
        "closed epics leave the live list"
    );
    let past = super::service::list_history(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .unwrap();
    assert_eq!(
        past.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["old-1"]
    );
    assert_eq!(past[0].status.state, super::a2a::A2aState::Completed);
    assert!(
        super::service::list_history(&rig, &clock(), &who(&[Scope::Plan]))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn list_and_get_render_epic_progress() {
    let (rig, _, _, _) = seeded().await;
    let tasks = list_tasks(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .expect("ok");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status.state, A2aState::Working);
    assert_eq!(tasks[0].metadata["factory"]["closed"], 1);
    let one = get_task(&rig, &clock(), &who(&[Scope::Admin]), "ep-1")
        .await
        .expect("ok");
    assert_eq!(one.context_id, "ep-1");
    assert_eq!(
        get_task(&rig, &clock(), &who(&[Scope::Watch]), "ep-1.2").await,
        Err(RemoteError::TaskNotFound {
            id: "ep-1.2".into()
        })
    );
    assert_eq!(
        get_task(&rig, &clock(), &who(&[Scope::Watch]), "nope").await,
        Err(RemoteError::TaskNotFound { id: "nope".into() })
    );
    assert_eq!(
        get_task(&rig, &clock(), &who(&[Scope::Watch]), "!!").await,
        Err(RemoteError::TaskNotFound { id: "!!".into() })
    );
}

#[tokio::test]
async fn refusals_are_audited() {
    let (rig, _, sink, _) = seeded().await;
    let err = list_tasks(&rig, &clock(), &who(&[Scope::Plan]))
        .await
        .expect_err("forbidden");
    assert!(matches!(err, RemoteError::Forbidden(_)));
    let events = sink.events().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor, "remote:tester");
    assert!(matches!(&events[0].kind, EventKind::Remote { action, .. } if action == "refused"));
}

#[tokio::test]
async fn plan_submits_and_returns_the_epic() {
    let (rig, _, sink, _) = seeded().await;
    let sent = send_message(
        &rig,
        &clock(),
        &who(&[Scope::Plan, Scope::Watch]),
        None,
        "build a thing\nmore",
    )
    .await
    .expect("ok");
    let task = planned(sent).expect("planned");
    assert_eq!(task.id, "ep-1");
    assert!(sink.events().await.iter().any(|e| matches!(&e.kind, EventKind::Remote { action, detail } if action == "plan" && detail == "build a thing")));
    assert_eq!(
        send_message(&rig, &clock(), &who(&[Scope::Plan]), None, "  ").await,
        Err(RemoteError::EmptyMessage)
    );
    assert!(matches!(
        send_message(&rig, &clock(), &who(&[Scope::Watch]), None, "x").await,
        Err(RemoteError::Forbidden(_))
    ));
}

#[tokio::test]
async fn plan_is_refused_over_budget_and_on_submit_failure() {
    let (mut rig, _, sink, _) = seeded().await;
    rig.budget = RigBudget {
        max_tokens: Some(Tokens::new(300)),
        max_usd: None,
    };
    assert_eq!(spend(&rig).await.expect("ok").tokens, Tokens::new(300));
    let err = send_message(&rig, &clock(), &who(&[Scope::Admin]), None, "x")
        .await
        .expect_err("budget");
    assert_eq!(
        err,
        RemoteError::Budget(RigBudgetExceeded::Tokens {
            spent: Tokens::new(300),
            cap: Tokens::new(300)
        })
    );
    assert!(
        sink.events().await.iter().any(
            |e| matches!(&e.kind, EventKind::Remote { action, .. } if action == "plan-refused")
        )
    );
    rig.budget = RigBudget {
        max_tokens: None,
        max_usd: Some(MicroUsd::new(0)),
    };
    assert!(matches!(
        send_message(&rig, &clock(), &who(&[Scope::Admin]), None, "x").await,
        Err(RemoteError::Budget(RigBudgetExceeded::Usd { .. }))
    ));
    rig.budget = RigBudget::default();
    rig.planner = std::sync::Arc::new(FakePlanner {
        epic: Err(SubmitError::Rejected {
            detail: "bad".into(),
        }),
        submitted: std::sync::Mutex::new(Vec::new()),
    });
    assert!(matches!(
        send_message(&rig, &clock(), &who(&[Scope::Admin]), None, "x").await,
        Err(RemoteError::Submit(SubmitError::Rejected { .. }))
    ));
}

#[tokio::test]
async fn resolve_closes_inbox_item_and_reopens_task() {
    let (rig, store, _, _) = seeded().await;
    store
        .seed_task(
            id("ep-1.3"),
            meta(
                TaskState::Incident {
                    reason: domain::task::IncidentReason::LeaseStorm {
                        expiries: Attempts::new(3),
                    },
                },
                0,
            ),
        )
        .await;
    store.set_parent(&id("ep-1.3"), &id("ep-1")).await;
    let inc = store
        .create(crate::NewBead {
            title: domain::Title::try_new("incident on ep-1.3").expect("t"),
            description: "boom".into(),
            kind: BeadKind::Incident,
            priority: domain::Priority::try_from(1u8).expect("p"),
            parent: Some(id("ep-1")),
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await
        .expect("created");
    let tasks = list_tasks(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .expect("ok");
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].status.state, A2aState::InputRequired);
    assert_eq!(tasks[1].status.state, A2aState::InputRequired);
    assert!(
        tasks[1]
            .status
            .message
            .as_ref()
            .is_some_and(|m| m.text().contains("boom"))
    );
    let sent = send_message(
        &rig,
        &clock(),
        &who(&[Scope::Resolve, Scope::Watch]),
        Some(inc.as_ref()),
        "fixed it",
    )
    .await
    .expect("ok");
    let (task, reopened) = resolved(sent).expect("resolved");
    assert_eq!(task.status.state, A2aState::Completed);
    assert_eq!(reopened, Some(id("ep-1.3")));
    assert_eq!(
        send_message(
            &rig,
            &clock(),
            &who(&[Scope::Admin]),
            Some(inc.as_ref()),
            "again"
        )
        .await,
        Err(RemoteError::Terminal {
            id: inc.to_string()
        })
    );
    assert!(matches!(
        send_message(&rig, &clock(), &who(&[Scope::Plan]), Some("x"), "n").await,
        Err(RemoteError::Forbidden(_))
    ));
}

#[tokio::test]
async fn cancel_closes_open_children_and_marks_epic() {
    let (rig, store, _, _) = seeded().await;
    let task = cancel_task(&rig, &clock(), &who(&[Scope::Plan, Scope::Watch]), "ep-1")
        .await
        .expect("ok");
    assert_eq!(task.status.state, A2aState::Canceled);
    assert_eq!(
        store.show(&id("ep-1.2")).await.expect("b").status,
        crate::BeadStatus::Closed
    );
    assert_eq!(
        cancel_task(&rig, &clock(), &who(&[Scope::Admin]), "ep-1").await,
        Err(RemoteError::Terminal { id: "ep-1".into() })
    );
    assert!(matches!(
        cancel_task(&rig, &clock(), &who(&[Scope::Watch]), "ep-1").await,
        Err(RemoteError::Forbidden(_))
    ));
    let epic = store.show(&id("ep-1")).await.expect("b");
    let shown = super::a2a::epic_task(&epic, &[], "now");
    assert_eq!(shown.status.state, A2aState::Canceled);
}

#[tokio::test]
async fn events_filter_to_context() {
    let (rig, _, _, tail) = seeded().await;
    tail.push("worker", Some(id("ep-1.2")), "claimed");
    tail.push("worker", Some(id("zz-9")), "claimed");
    tail.push("steward", None, "sweep_done");
    let (all, next) = events_after(&rig, &clock(), &who(&[Scope::Watch]), 0, None)
        .await
        .expect("ok");
    assert_eq!((all.len(), next), (3, 3));
    let (mine, _) = events_after(&rig, &clock(), &who(&[Scope::Watch]), 0, Some("ep-1"))
        .await
        .expect("ok");
    assert_eq!(mine.len(), 1);
    let (later, _) = events_after(&rig, &clock(), &who(&[Scope::Watch]), 3, None)
        .await
        .expect("ok");
    assert!(later.is_empty());
    let (none, _) = events_after(&rig, &clock(), &who(&[Scope::Watch]), 0, Some("unknown"))
        .await
        .expect("ok");
    assert!(none.is_empty());
    assert_eq!(
        events_after(&rig, &clock(), &who(&[Scope::Watch]), 0, Some("!!")).await,
        Err(RemoteError::TaskNotFound { id: "!!".into() })
    );
    assert!(matches!(
        events_after(&rig, &clock(), &who(&[Scope::Plan]), 0, None).await,
        Err(RemoteError::Forbidden(_))
    ));
    let failing = Rig {
        events: std::sync::Arc::new(FakeTail {
            fail: true,
            ..FakeTail::default()
        }),
        ..rig
    };
    assert!(matches!(
        events_after(&failing, &clock(), &who(&[Scope::Watch]), 0, None).await,
        Err(RemoteError::Tail(_))
    ));
}

#[test]
fn pure_mapping_and_serde_shapes() {
    assert!(A2aState::Canceled.is_terminal() && !A2aState::Submitted.is_terminal());
    assert_eq!(epic_progress(&[]), super::a2a::EpicProgress::default());
    let m = Message {
        message_id: "m".into(),
        role: "ROLE_USER".into(),
        parts: vec![
            Part::Text("a".into()),
            Part::Data(serde_json::json!({"k": 1})),
            Part::Text("b".into()),
        ],
        task_id: None,
        context_id: None,
        metadata: None,
    };
    assert_eq!(m.text(), "a\nb");
    let json = serde_json::to_value(&m).expect("json");
    assert_eq!(json["parts"][0], serde_json::json!({"text": "a"}));
    assert_eq!(
        serde_json::to_value(A2aState::InputRequired).expect("json"),
        "TASK_STATE_INPUT_REQUIRED"
    );
    assert_eq!(skills().len(), 5);
    let registry = FakeRegistry::default();
    assert!(registry.names().is_empty());
    assert!(
        registry
            .rig(&domain::RigName::try_new("toy").expect("r"))
            .is_none()
    );
    let (r, ..) = rig("toy", FakePlanner::returning("e-1"));
    assert!(format!("{r:?}").contains("toy"));
}

#[tokio::test]
async fn closed_epics_reappear_for_watchers_that_saw_them() {
    let (rig, store, _, _) = seeded().await;
    store.close(&id("ep-1"), "done").await.expect("closed");
    let none = list_tasks(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .expect("ok");
    assert!(none.is_empty());
    let seen = std::collections::BTreeMap::from([
        ("ep-1".to_owned(), A2aState::Working),
        ("gone".to_owned(), A2aState::Working),
    ]);
    let tasks = list_tasks_with_vanished(&rig, &clock(), &who(&[Scope::Watch]), &seen)
        .await
        .expect("ok");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status.state, A2aState::Completed);
    assert!(matches!(
        list_tasks_with_vanished(&rig, &clock(), &who(&[Scope::Plan]), &seen).await,
        Err(RemoteError::Forbidden(_))
    ));
}

#[tokio::test]
async fn plans_can_be_queued_and_watched_as_tasks() {
    let (rig, store, sink, _) = seeded().await;
    let task = enqueue_plan(
        &rig,
        &clock(),
        &who(&[Scope::Plan, Scope::Watch]),
        "build\nmore",
    )
    .await
    .expect("queued");
    assert_eq!(task.status.state, A2aState::Submitted);
    assert_eq!(task.metadata["factory"]["kind"], "plan_request");
    assert!(
        sink.events().await.iter().any(
            |e| matches!(&e.kind, EventKind::Remote { action, .. } if action == "plan-queued")
        )
    );
    let listed = list_tasks(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .expect("ok");
    assert!(
        listed
            .iter()
            .any(|t| t.id == task.id && t.status.state == A2aState::Submitted)
    );
    let req = id(&task.id);
    store.note(&req, "epic ep-1").await.expect("note");
    store.close(&req, "epic ep-1").await.expect("closed");
    let done = get_task(&rig, &clock(), &who(&[Scope::Watch]), &task.id)
        .await
        .expect("ok");
    assert_eq!(done.status.state, A2aState::Completed);
    assert_eq!(done.metadata["factory"]["epic"], "ep-1");
    assert_eq!(done.context_id, "ep-1");
    let failed = enqueue_plan(&rig, &clock(), &who(&[Scope::Admin]), "again")
        .await
        .expect("queued");
    let fid = id(&failed.id);
    store.note(&fid, "failed: no model").await.expect("note");
    store.close(&fid, "failed").await.expect("closed");
    let f = get_task(&rig, &clock(), &who(&[Scope::Watch]), &failed.id)
        .await
        .expect("ok");
    assert_eq!(f.status.state, A2aState::Failed);
    assert!(
        f.status
            .message
            .as_ref()
            .is_some_and(|m| m.text().contains("no model"))
    );
    assert_eq!(
        enqueue_plan(&rig, &clock(), &who(&[Scope::Admin]), " ").await,
        Err(RemoteError::EmptyMessage)
    );
    assert!(matches!(
        enqueue_plan(&rig, &clock(), &who(&[Scope::Watch]), "x").await,
        Err(RemoteError::Forbidden(_))
    ));
}
