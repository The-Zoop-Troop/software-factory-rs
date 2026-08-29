//! The A2A and Telegram HTTP clients against a tiny canned-response server.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "tests: canned HTTP server with a leaf std Mutex"
)]

use std::sync::{Arc, Mutex};

use app::A2aApi as _;
use app::ClientError;
use infra::{A2aHttp, TelegramApi};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// Serves one canned `(status, body)` per request, in order; records request bodies.
async fn server(replies: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    tokio::spawn(async move {
        for (status, body) in replies {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 65536];
            let mut total = 0;
            loop {
                let n = sock.read(&mut buf[total..]).await.unwrap();
                total += n;
                let text = String::from_utf8_lossy(&buf[..total]).into_owned();
                if let Some(idx) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if total >= idx + 4 + len {
                        seen2.lock().unwrap().push(text);
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let resp = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.shutdown().await.ok();
        }
    });
    (format!("http://{addr}"), seen)
}

fn task_json(id: &str, state: &str) -> String {
    format!(
        r#"{{"id":"{id}","contextId":"{id}","status":{{"state":"{state}","timestamp":"t"}},"metadata":{{}}}}"#
    )
}

#[tokio::test]
async fn a2a_client_speaks_json_rpc() {
    let (base, seen) = server(vec![
        (
            200,
            format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"tasks":[{}]}}}}"#,
                task_json("ep-1", "TASK_STATE_WORKING")
            ),
        ),
        (
            200,
            format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{}}}"#,
                task_json("ep-1", "TASK_STATE_WORKING")
            ),
        ),
        (
            200,
            format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"task":{}}}}}"#,
                task_json("ep-2", "TASK_STATE_SUBMITTED")
            ),
        ),
        (
            200,
            format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{}}}"#,
                task_json("ep-2", "TASK_STATE_CANCELED")
            ),
        ),
        (
            403,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32040,"message":"no"}}"#.to_owned(),
        ),
        (502, "<html>bad gateway</html>".to_owned()),
        (200, r#"{"jsonrpc":"2.0","id":1}"#.to_owned()),
        (500, r#"{"jsonrpc":"2.0","id":1,"result":null}"#.to_owned()),
        (200, r#"{"name":"factory rig toy","skills":[]}"#.to_owned()),
        (404, r#"{"error":"no such rig"}"#.to_owned()),
    ])
    .await;
    let api = A2aHttp::new(&format!("{base}/rigs/toy/a2a/"), "tok").unwrap();
    assert!(format!("{api:?}").contains("/rigs/toy/a2a"));
    let tasks = api.list_tasks().await.unwrap();
    assert_eq!(tasks[0].id, "ep-1");
    assert_eq!(api.get_task("ep-1").await.unwrap().id, "ep-1");
    assert_eq!(api.send("plan", None).await.unwrap().id, "ep-2");
    assert_eq!(
        api.cancel("ep-2").await.unwrap().status.state,
        app::remote::a2a::A2aState::Canceled
    );
    assert_eq!(
        api.send("note", Some("inc-1")).await,
        Err(ClientError::Refused {
            status: 403,
            code: Some(-32040),
            message: "no".into()
        })
    );
    assert!(matches!(
        api.list_tasks().await,
        Err(ClientError::Decode { .. })
    ));
    assert!(matches!(
        api.list_tasks().await,
        Err(ClientError::Decode { .. })
    ));
    assert!(matches!(
        api.list_tasks().await,
        Err(ClientError::Refused {
            status: 500,
            code: None,
            ..
        })
    ));
    assert_eq!(api.card().await.unwrap()["name"], "factory rig toy");
    assert!(matches!(
        api.card().await,
        Err(ClientError::Refused { status: 404, .. })
    ));
    let requests = seen.lock().unwrap().clone();
    assert!(
        requests[0].contains("authorization: Bearer tok")
            || requests[0].contains("authorization: bearer tok")
    );
    assert!(requests[0].contains(r#""method":"ListTasks""#));
    assert!(requests[2].contains(r#""text":"plan""#));
    assert!(requests[4].contains(r#""taskId":"inc-1""#));
    assert!(requests[8].contains("GET /rigs/toy/.well-known/agent-card.json"));
    let down = A2aHttp::new("http://127.0.0.1:1/rigs/x", "t").unwrap();
    assert!(matches!(
        down.list_tasks().await,
        Err(ClientError::Transport { .. })
    ));
    assert!(matches!(
        down.card().await,
        Err(ClientError::Transport { .. })
    ));
}

#[tokio::test]
async fn telegram_client_polls_and_sends() {
    let (base, seen) = server(vec![
        (200, r#"{"ok":true,"result":[{"update_id":7,"message":{"chat":{"id":42},"text":"/watch"}},{"update_id":8,"message":{"chat":{"id":42}}},{"update_id":9}]}"#.to_owned()),
        (200, r#"{"ok":true,"result":{"message_id":1}}"#.to_owned()),
        (401, r#"{"ok":false,"description":"Unauthorized"}"#.to_owned()),
        (200, r#"{"ok":true}"#.to_owned()),
        (200, "nope".to_owned()),
    ])
    .await;
    let tg = TelegramApi::new(&base, "123:abc").unwrap();
    assert!(format!("{tg:?}").contains("TelegramApi"));
    let updates = tg.updates(0, 1).await.unwrap();
    assert_eq!(
        updates,
        vec![infra::Incoming {
            update_id: 7,
            chat_id: 42,
            text: "/watch".into()
        }]
    );
    tg.send(42, "hi").await.unwrap();
    assert_eq!(
        tg.send(42, "hi").await,
        Err(ClientError::Refused {
            status: 401,
            code: None,
            message: "Unauthorized".into()
        })
    );
    assert!(matches!(
        tg.updates(0, 1).await,
        Err(ClientError::Refused { status: 200, .. })
    ));
    assert!(matches!(
        tg.updates(0, 1).await,
        Err(ClientError::Decode { .. })
    ));
    let requests = seen.lock().unwrap().clone();
    assert!(requests[0].contains("POST /bot123:abc/getUpdates"));
    assert!(requests[1].contains(r#""chat_id":42"#));
    let down = TelegramApi::new("http://127.0.0.1:1", "t").unwrap();
    assert!(matches!(
        down.send(1, "x").await,
        Err(ClientError::Transport { .. })
    ));
}
