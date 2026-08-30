#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_types,
    reason = "tests: fakes with a leaf std Mutex"
)]
use std::sync::Mutex;

use app::ClientError;
use app::remote::a2a::{A2aState, Task, TaskStatus};
use app::testing::remote::FakeApi;
use async_trait::async_trait;
use infra::Incoming;

use crate::cli::Command;
use crate::remote::{RemoteCommand, RemoteUnsupported, execute, remote_command};
use crate::telegram::{BotState, ChatTransport, reply_for, step};

fn task(id: &str, kind: &str, state: A2aState) -> Task {
    Task {
        id: id.into(),
        context_id: id.into(),
        status: TaskStatus {
            state,
            message: None,
            timestamp: "t".into(),
        },
        metadata: serde_json::json!({"factory": {"kind": kind, "title": id, "closed": 0, "tasks": 1, "incidents": 0}}),
    }
}

#[test]
fn commands_map_or_are_refused() {
    assert_eq!(
        remote_command(Command::Watch { interval: Some(3) }),
        Ok(RemoteCommand::Watch { interval: Some(3) })
    );
    assert_eq!(
        remote_command(Command::Stop { epic: "e".into() }),
        Ok(RemoteCommand::Stop { epic: "e".into() })
    );
    assert_eq!(
        remote_command(Command::Doctor {
            repo: "r".into(),
            probe: false
        }),
        Ok(RemoteCommand::Doctor)
    );
    assert_eq!(
        remote_command(Command::Inbox {
            resolve: None,
            note: "n".into()
        }),
        Ok(RemoteCommand::Inbox {
            resolve: None,
            note: "n".into()
        })
    );
    let plan = |file, text| Command::Plan {
        repo: "r".into(),
        main: "main".into(),
        file,
        text,
        harness: crate::cli::HarnessKind::Claude,
        model: None,
        max_budget_usd: 1.0,
        queue: false,
        interval: None,
        events: "e".into(),
    };
    assert_eq!(
        remote_command(plan(None, Some("x".into()))),
        Ok(RemoteCommand::Plan { text: "x".into() })
    );
    assert_eq!(
        remote_command(plan(None, None)),
        Err(RemoteUnsupported::NoPlanText)
    );
    assert!(matches!(
        remote_command(plan(Some("/nonexistent/plan.md".into()), None)),
        Err(RemoteUnsupported::PlanFile { .. })
    ));
    assert_eq!(
        remote_command(Command::Version),
        Err(RemoteUnsupported::LocalOnly { name: "version" })
    );
    assert!(
        remote_command(Command::Telegram {
            bot_token: "t".into(),
            chats: vec![1],
            poll: 5,
            api_base: "b".into()
        })
        .is_ok()
    );
}

#[tokio::test]
async fn execute_renders_each_command() {
    let api = FakeApi::with_tasks(vec![
        task("ep-1", "epic", A2aState::Working),
        task("q-1", "question", A2aState::InputRequired),
    ]);
    assert!(
        execute(&api, RemoteCommand::Watch { interval: None })
            .await
            .unwrap()
            .contains("ep-1")
    );
    let inbox = execute(
        &api,
        RemoteCommand::Inbox {
            resolve: Some("q-1".into()),
            note: "yes".into(),
        },
    )
    .await
    .unwrap();
    assert!(inbox.starts_with("resolved q-1"));
    assert!(inbox.contains("q-1  [question]"));
    assert_eq!(
        execute(&api, RemoteCommand::Plan { text: "go".into() })
            .await
            .unwrap(),
        "planned: epic ep-1 (working)\n"
    );
    assert_eq!(
        execute(
            &api,
            RemoteCommand::Stop {
                epic: "ep-1".into()
            }
        )
        .await
        .unwrap(),
        "stopped ep-1 (working)\n"
    );
    let doctor = execute(&api, RemoteCommand::Doctor).await.unwrap();
    assert!(doctor.contains("console reachable") && doctor.contains("2 task(s)"));
    assert!(
        execute(
            &api,
            RemoteCommand::Telegram {
                bot_token: "t".into(),
                chats: vec![],
                poll: 1,
                api_base: "b".into()
            }
        )
        .await
        .unwrap()
        .contains("telegram")
    );
    let empty = FakeApi::default();
    assert_eq!(
        execute(
            &empty,
            RemoteCommand::Inbox {
                resolve: None,
                note: String::new()
            }
        )
        .await
        .unwrap(),
        "inbox empty\n"
    );
    let down = FakeApi {
        fail: Some(ClientError::Transport { detail: "x".into() }),
        ..FakeApi::default()
    };
    assert!(execute(&down, RemoteCommand::Doctor).await.is_err());
}

#[derive(Default)]
struct FakeTg {
    inbox: Mutex<Vec<Incoming>>,
    sent: Mutex<Vec<(i64, String)>>,
    fail_send: bool,
    fail_poll: bool,
}

#[async_trait]
impl ChatTransport for FakeTg {
    async fn updates(&self, offset: i64, _t: u64) -> Result<Vec<Incoming>, ClientError> {
        if self.fail_poll {
            return Err(ClientError::Transport {
                detail: "poll".into(),
            });
        }
        Ok(self
            .inbox
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.update_id >= offset)
            .cloned()
            .collect())
    }
    async fn send(&self, chat_id: i64, text: &str) -> Result<(), ClientError> {
        if self.fail_send {
            return Err(ClientError::Transport {
                detail: "send".into(),
            });
        }
        self.sent.lock().unwrap().push((chat_id, text.to_owned()));
        Ok(())
    }
}

#[tokio::test]
async fn bot_answers_allowed_chats_and_pushes_changes() {
    let api = FakeApi::with_tasks(vec![task("ep-1", "epic", A2aState::Working)]);
    let tg = FakeTg::default();
    tg.inbox.lock().unwrap().extend([
        Incoming {
            update_id: 1,
            chat_id: 7,
            text: "/watch".into(),
        },
        Incoming {
            update_id: 2,
            chat_id: 9,
            text: "/watch".into(),
        },
        Incoming {
            update_id: 3,
            chat_id: 7,
            text: "hello".into(),
        },
        Incoming {
            update_id: 4,
            chat_id: 7,
            text: "/bogus".into(),
        },
    ]);
    assert_eq!(
        reply_for(
            &api,
            &[7],
            &Incoming {
                update_id: 0,
                chat_id: 7,
                text: "/help".into()
            }
        )
        .await
        .map(|s| s.contains("/plan")),
        Some(true)
    );
    let s1 = step(&api, &tg, &[7], BotState::default(), 1).await;
    assert_eq!(s1.offset, 5);
    assert!(s1.primed);
    let sent = tg.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 2);
    assert!(sent[0].1.contains("ep-1") && sent[1].1.contains("/help"));
    // Task flips to done between polls → one push to each allowed chat.
    api.tasks.lock().unwrap()[0].status.state = A2aState::Completed;
    let s2 = step(&api, &tg, &[7, 8], s1, 1).await;
    let sent = tg.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 4);
    assert_eq!(sent[2], (7, "ep-1 done: ep-1".to_owned()));
    assert_eq!(sent[3].0, 8);
    let s3 = step(&api, &tg, &[7], s2.clone(), 1).await;
    assert_eq!(s3, s2);
    // Failures are logged, not fatal.
    let broken = FakeTg {
        fail_send: true,
        fail_poll: true,
        ..FakeTg::default()
    };
    let s4 = step(&api, &broken, &[7], BotState::default(), 1).await;
    assert!(s4.primed);
    let down = FakeApi {
        fail: Some(ClientError::Transport { detail: "x".into() }),
        ..FakeApi::default()
    };
    let s5 = step(&down, &tg, &[7], BotState::default(), 1).await;
    assert!(!s5.primed);
    let reply = reply_for(
        &down,
        &[7],
        &Incoming {
            update_id: 0,
            chat_id: 7,
            text: "/watch".into(),
        },
    )
    .await
    .unwrap();
    assert!(reply.starts_with("error:"));
    let sending_fails = FakeTg {
        fail_send: true,
        ..FakeTg::default()
    };
    sending_fails.inbox.lock().unwrap().push(Incoming {
        update_id: 1,
        chat_id: 7,
        text: "/watch".into(),
    });
    let s6 = step(&api, &sending_fails, &[7], BotState::default(), 1).await;
    assert_eq!(s6.offset, 2);
}
