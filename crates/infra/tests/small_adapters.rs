//! JSONL sink and system clock.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

use infra::app::domain::{BeadId, Duration, Timestamp};
use infra::app::{Clock, EventKind, EventSink, FactoryEvent};
use infra::{JsonlSink, SystemClock};

#[tokio::test]
async fn jsonl_appends_one_line_per_event() {
    let path = std::env::temp_dir().join(format!("factory-ev-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let sink = JsonlSink::open(&path).unwrap();
    for i in 0..3 {
        sink.record(&FactoryEvent {
            at: Timestamp::from_unix_seconds(i),
            actor: "t".into(),
            bead: Some(BeadId::try_new("fac-1").unwrap()),
            kind: EventKind::LeaseReaped,
        });
    }
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("\"kind\":\"lease_reaped\""));
    assert!(JsonlSink::open(std::path::Path::new("/nonexistent/dir/x.jsonl")).is_err());
}

#[tokio::test]
async fn system_clock_is_now_and_sleeps() {
    let c = SystemClock;
    let t0 = c.now();
    assert!(t0.unix_seconds() > 1_700_000_000);
    c.sleep(Duration::from_seconds(0)).await;
    assert!(c.now() >= t0);
}

#[tokio::test]
async fn docker_cli_shells_out_for_compose_volumes_and_tar() {
    use app::HostDocker as _;
    // The fake logs to $TMPDIR/fake-docker-<parent pid>.log; our pid is its parent.
    let log = std::env::temp_dir().join(format!("fake-docker-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);
    let docker = infra::DockerCli::default()
        .with_bin(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fakebin/docker"));
    let out = docker
        .compose(
            "factory-toy",
            std::path::Path::new("/e"),
            std::path::Path::new("/c.yaml"),
            &["ps", "--services"],
        )
        .await
        .unwrap();
    assert_eq!(out.trim(), "steward");
    assert!(matches!(
        docker.compose("factory-toy", std::path::Path::new("/e"), std::path::Path::new("/c.yaml"), &["fail"]).await,
        Err(app::HostError::Command { detail, .. }) if detail == "boom"
    ));
    assert!(docker.volume_exists("factory-toy_ledger").await.unwrap());
    assert!(!docker.volume_exists("nope").await.unwrap());
    let dest = std::env::temp_dir()
        .join(format!("fake-docker-{}", std::process::id()))
        .join("toy-ledger-1.tgz");
    docker
        .archive_volume("factory-toy_ledger", &dest)
        .await
        .unwrap();
    docker
        .restore_volume("factory-toy_ledger", &dest)
        .await
        .unwrap();
    assert!(
        docker
            .archive_volume("v", std::path::Path::new("/"))
            .await
            .is_err()
    );
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("compose -p factory-toy --env-file /e -f /c.yaml ps --services"));
    assert!(logged.contains("tar czf /b/toy-ledger-1.tgz"));
    assert!(logged.contains("tar xzf /b/toy-ledger-1.tgz"));
    let missing = infra::DockerCli::default().with_bin("/nonexistent/docker");
    assert!(matches!(
        missing.volume_exists("x").await,
        Err(app::HostError::Missing { .. })
    ));
    let _ = std::fs::remove_dir_all(dest.parent().unwrap());
}
