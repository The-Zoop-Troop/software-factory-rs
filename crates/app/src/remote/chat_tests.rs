#![allow(clippy::unwrap_used, reason = "tests")]
use super::a2a::{A2aState, Message, Part, Task, TaskStatus};
use super::chat::A2aApi as _;
use super::chat::{
    ChatCommand, ChatParseError, ClientError, Seen, handle, notifications, parse_command,
    render_inbox, render_tasks,
};
use crate::testing::remote::FakeApi;

fn task(id: &str, kind: &str, state: A2aState, closed: u64, total: u64, incidents: u64) -> Task {
    let message = (state == A2aState::InputRequired).then(|| Message {
        message_id: "m".into(),
        role: "ROLE_AGENT".into(),
        parts: vec![Part::Text("why?\nmore".into())],
        task_id: Some(id.into()),
        context_id: None,
    });
    Task {
        id: id.into(),
        context_id: id.into(),
        status: TaskStatus {
            state,
            message,
            timestamp: "t".into(),
        },
        metadata: serde_json::json!({"factory": {"kind": kind, "title": format!("T {id}"), "closed": closed, "tasks": total, "incidents": incidents}}),
    }
}

#[test]
fn renders_epics_and_inbox() {
    let tasks = vec![
        task("ep-1", "epic", A2aState::Working, 2, 5, 0),
        task("ep-2", "epic", A2aState::InputRequired, 1, 3, 1),
        task("inc-1", "incident", A2aState::InputRequired, 0, 0, 0),
        task("q-9", "question", A2aState::Completed, 0, 0, 0),
    ];
    let out = render_tasks(&tasks);
    assert!(out.contains("ep-1  T ep-1  [2/5] working\n"));
    assert!(out.contains("ep-2  T ep-2  [1/3] NEEDS YOU, 1 incident(s)\n"));
    assert!(out.contains("inc-1  [incident] why?\n"));
    assert!(!out.contains("q-9"));
    assert_eq!(render_tasks(&[]), "nothing in flight\n");
    assert_eq!(render_inbox(&tasks[..2]), "");
}

#[test]
fn parses_commands() {
    assert_eq!(
        parse_command("/plan build it now"),
        Ok(ChatCommand::Plan {
            text: "build it now".into()
        })
    );
    assert_eq!(parse_command("/Watch@factory_bot"), Ok(ChatCommand::Watch));
    assert_eq!(parse_command("/status"), Ok(ChatCommand::Watch));
    assert_eq!(parse_command("/inbox"), Ok(ChatCommand::Inbox));
    assert_eq!(
        parse_command("/resolve inc-1  fixed the thing"),
        Ok(ChatCommand::Resolve {
            id: "inc-1".into(),
            note: "fixed the thing".into()
        })
    );
    assert_eq!(
        parse_command("/stop ep-1"),
        Ok(ChatCommand::Stop { id: "ep-1".into() })
    );
    assert_eq!(parse_command("/help"), Ok(ChatCommand::Help));
    assert_eq!(parse_command("/start"), Ok(ChatCommand::Help));
    assert_eq!(parse_command("hello"), Err(ChatParseError::NotACommand));
    assert_eq!(
        parse_command("/plan"),
        Err(ChatParseError::MissingArgument {
            name: "plan".into(),
            needs: "the plan text"
        })
    );
    assert_eq!(
        parse_command("/resolve inc-1"),
        Err(ChatParseError::MissingArgument {
            name: "resolve".into(),
            needs: "<id> <note>"
        })
    );
    assert_eq!(
        parse_command("/stop"),
        Err(ChatParseError::MissingArgument {
            name: "stop".into(),
            needs: "<epic id>"
        })
    );
    assert_eq!(
        parse_command("/dance"),
        Err(ChatParseError::Unknown {
            name: "dance".into()
        })
    );
    assert!(
        parse_command("/dance")
            .unwrap_err()
            .to_string()
            .contains("/help")
    );
}

#[tokio::test]
async fn handles_every_command_against_the_api() {
    let api = FakeApi::with_tasks(vec![
        task("ep-1", "epic", A2aState::Working, 0, 2, 0),
        task("inc-1", "incident", A2aState::InputRequired, 0, 0, 0),
    ]);
    assert!(
        handle(&api, ChatCommand::Help)
            .await
            .unwrap()
            .contains("/plan")
    );
    assert!(
        handle(&api, ChatCommand::Watch)
            .await
            .unwrap()
            .contains("ep-1")
    );
    assert!(
        handle(&api, ChatCommand::Inbox)
            .await
            .unwrap()
            .contains("inc-1")
    );
    assert_eq!(
        handle(&api, ChatCommand::Plan { text: "go".into() })
            .await
            .unwrap(),
        "planned: epic ep-1 (working)\n"
    );
    assert_eq!(
        handle(
            &api,
            ChatCommand::Resolve {
                id: "inc-1".into(),
                note: "ok".into()
            }
        )
        .await
        .unwrap(),
        "resolved inc-1 (NEEDS YOU)\n"
    );
    assert_eq!(
        handle(&api, ChatCommand::Stop { id: "ep-1".into() })
            .await
            .unwrap(),
        "stopped ep-1 (working)\n"
    );
    assert_eq!(api.sent.lock().unwrap().len(), 2);
    assert_eq!(api.canceled.lock().unwrap().as_slice(), ["ep-1"]);
    let empty = FakeApi::default();
    assert_eq!(
        handle(&empty, ChatCommand::Inbox).await.unwrap(),
        "inbox empty\n"
    );
    assert!(matches!(
        handle(&empty, ChatCommand::Plan { text: "x".into() }).await,
        Err(ClientError::Decode { .. })
    ));
    assert!(matches!(
        empty.get_task("zz").await,
        Err(ClientError::Refused { status: 404, .. })
    ));
    let down = FakeApi {
        fail: Some(ClientError::Transport {
            detail: "down".into(),
        }),
        ..FakeApi::default()
    };
    let err = handle(&down, ChatCommand::Watch).await.unwrap_err();
    assert_eq!(err.to_string(), "console unreachable: down");
    assert!(
        ClientError::Refused {
            status: 403,
            code: Some(-32040),
            message: "no".into()
        }
        .to_string()
        .contains("code -32040")
    );
    assert!(matches!(
        down.get_task("x").await,
        Err(ClientError::Transport { .. })
    ));
    assert!(matches!(
        down.cancel("x").await,
        Err(ClientError::Transport { .. })
    ));
    assert!(matches!(
        down.send("x", None).await,
        Err(ClientError::Transport { .. })
    ));
}

#[test]
fn notifications_fire_on_changes_only() {
    let seen = Seen::new();
    let now = vec![
        task("ep-1", "epic", A2aState::Working, 0, 2, 0),
        task("ep-0", "epic", A2aState::Completed, 2, 2, 0),
        task("inc-1", "incident", A2aState::InputRequired, 0, 0, 0),
    ];
    let (msgs, seen) = notifications(&seen, &now);
    assert_eq!(msgs, vec!["inc-1 needs you: T inc-1".to_owned()]);
    let later = vec![
        task("ep-1", "epic", A2aState::Completed, 2, 2, 0),
        task("ep-0", "epic", A2aState::Completed, 2, 2, 0),
        task("inc-1", "incident", A2aState::Completed, 0, 0, 0),
        task("ep-3", "epic", A2aState::Submitted, 0, 1, 0),
    ];
    let (msgs, seen) = notifications(&seen, &later);
    assert_eq!(
        msgs,
        vec![
            "ep-1 done: T ep-1".to_owned(),
            "inc-1 done: T inc-1".to_owned()
        ]
    );
    let end = vec![
        task("ep-3", "epic", A2aState::Canceled, 0, 1, 0),
        task("ep-4", "epic", A2aState::Failed, 0, 1, 0),
    ];
    let mut seen2 = seen.clone();
    seen2.insert("ep-4".into(), A2aState::Working);
    let (msgs, _) = notifications(&seen2, &end);
    assert_eq!(
        msgs,
        vec![
            "ep-3 canceled: T ep-3".to_owned(),
            "ep-4 failed: T ep-4".to_owned()
        ]
    );
    let (quiet, _) = notifications(&seen, &end[..1]);
    assert_eq!(quiet, vec!["ep-3 canceled: T ep-3".to_owned()]);
}

#[tokio::test]
async fn vanished_tasks_are_fetched_once() {
    use super::chat::with_vanished;
    let api = FakeApi::with_tasks(vec![task("ep-2", "epic", A2aState::Completed, 1, 1, 0)]);
    let seen = Seen::from([
        ("ep-2".to_owned(), A2aState::Working),
        ("ep-9".to_owned(), A2aState::Working),
        ("old".to_owned(), A2aState::Completed),
    ]);
    let all = with_vanished(&api, &seen, vec![]).await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, "ep-2");
    let (msgs, _) = notifications(&seen, &all);
    assert_eq!(msgs, vec!["ep-2 done: T ep-2".to_owned()]);
}
