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
