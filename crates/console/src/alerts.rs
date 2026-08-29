//! Alerting: every `interval`, list each rig's tasks and post state changes (needs a human,
//! finished, failed, canceled) to a webhook as `{"rig": …, "text": …}`. Works with Slack
//! incoming webhooks, ntfy, or anything that takes JSON.

use std::collections::BTreeMap;
use std::sync::Arc;

use app::remote::chat::{Seen, notifications};
use app::{Clock, RigRegistry};
use async_trait::async_trait;
use domain::{ClientId, Principal, RigName, Scope};

/// Where alerts go. HTTP in production; recorded in tests.
#[async_trait]
pub(crate) trait AlertSink: Send + Sync {
    async fn post(&self, rig: &RigName, text: &str) -> Result<(), String>;
}

#[derive(Debug)]
pub(crate) struct Webhook {
    url: String,
    client: reqwest::Client,
}

impl Webhook {
    pub(crate) fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AlertSink for Webhook {
    async fn post(&self, rig: &RigName, text: &str) -> Result<(), String> {
        let body = crate::rpc::obj([("rig", rig.to_string().into()), ("text", text.into())]);
        self.client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// The console's own identity for reading rigs; audited like any client.
fn watcher(names: &[RigName]) -> Principal {
    Principal {
        client: ClientId::try_new("console-alerts").unwrap_or_else(|_| unreachable_client()),
        grants: names
            .iter()
            .map(|r| (r.clone(), std::iter::once(Scope::Watch).collect()))
            .collect(),
    }
}

// `console-alerts` matches the ClientId pattern; this branch is dead but keeps the fn total.
fn unreachable_client() -> ClientId {
    ClientId::try_new("console").unwrap_or_else(|_| unreachable_client())
}

/// One sweep over every rig: what changed since `seen`, and the new `seen`.
pub(crate) async fn sweep(
    registry: &dyn RigRegistry,
    clock: &dyn Clock,
    seen: &BTreeMap<RigName, Seen>,
) -> (Vec<(RigName, String)>, BTreeMap<RigName, Seen>) {
    let names = registry.names();
    let who = watcher(&names);
    let mut out = Vec::new();
    let mut next = BTreeMap::new();
    for name in names {
        let Some(rig) = registry.rig(&name) else {
            continue;
        };
        let before = seen.get(&name).cloned().unwrap_or_default();
        match app::list_tasks_with_vanished(&rig, clock, &who, &before).await {
            Ok(tasks) => {
                let (messages, now) = notifications(&before, &tasks);
                // First sight of a rig only learns its state.
                if seen.contains_key(&name) {
                    out.extend(messages.into_iter().map(|m| (name.clone(), m)));
                }
                next.insert(name, now);
            }
            Err(e) => {
                tracing::warn!(rig = %name, error = %e, "alert sweep could not read rig");
                if let Some(prev) = seen.get(&name) {
                    next.insert(name, prev.clone());
                }
            }
        }
    }
    (out, next)
}

/// Run forever: sweep, post, sleep.
pub(crate) async fn run(
    registry: Arc<dyn RigRegistry>,
    clock: Arc<dyn Clock>,
    sink: Arc<dyn AlertSink>,
    interval: domain::Duration,
) {
    let mut seen = BTreeMap::new();
    loop {
        let (alerts, next) = sweep(registry.as_ref(), clock.as_ref(), &seen).await;
        seen = next;
        for (rig, text) in alerts {
            if let Err(e) = sink.post(&rig, &text).await {
                tracing::warn!(rig = %rig, error = %e, "alert delivery failed");
            }
        }
        clock.sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use app::BeadStore as _;
    use app::remote::a2a::A2aState;
    use app::testing::FixedClock;
    use app::testing::remote::{FakePlanner, FakeRegistry, rig};
    use domain::{BeadId, Timestamp};

    use super::*;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, String)>>);

    #[async_trait]
    impl AlertSink for Recorder {
        async fn post(&self, rig: &RigName, text: &str) -> Result<(), String> {
            self.0
                .lock()
                .map_err(|e| e.to_string())?
                .push((rig.to_string(), text.to_owned()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn sweep_reports_changes_after_the_first_look() {
        let (r, store, ..) = rig("toy", FakePlanner::returning("ep-1"));
        store
            .seed_epic(BeadId::try_new("ep-1").expect("id"), &[("ep-1.1", false)])
            .await;
        let registry = FakeRegistry(BTreeMap::from([(r.name.clone(), r)]));
        let clock = FixedClock(Timestamp::from_unix_seconds(0));
        let (alerts, seen) = sweep(&registry, &clock, &BTreeMap::new()).await;
        assert!(alerts.is_empty());
        assert_eq!(seen.len(), 1);
        store
            .close(&BeadId::try_new("ep-1").expect("id"), "done")
            .await
            .expect("closed");
        let (alerts, seen2) = sweep(&registry, &clock, &seen).await;
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].1.contains("ep-1 done"));
        assert_eq!(
            seen2[&RigName::try_new("toy").expect("r")]["ep-1"],
            A2aState::Completed
        );
        let rec = Recorder::default();
        for (rig, text) in &alerts {
            rec.post(rig, text).await.expect("posted");
        }
        assert_eq!(rec.0.lock().expect("lock").len(), 1);
        assert_eq!(watcher(&[]).client.as_ref(), "console-alerts");
        assert_eq!(unreachable_client().as_ref(), "console");
    }

    #[tokio::test]
    async fn sweep_keeps_previous_state_when_a_rig_is_unreadable() {
        let (broken, store, ..) = rig("toy", FakePlanner::returning("ep-1"));
        store
            .fail_reads
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let name = broken.name.clone();
        let registry = FakeRegistry(BTreeMap::from([(name.clone(), broken)]));
        let clock = FixedClock(Timestamp::from_unix_seconds(0));
        let prev = BTreeMap::from([(
            name.clone(),
            Seen::from([("x".to_owned(), A2aState::Working)]),
        )]);
        let (alerts, seen) = sweep(&registry, &clock, &prev).await;
        assert!(alerts.is_empty());
        assert_eq!(seen, prev);
        let (_, empty) = sweep(&registry, &clock, &BTreeMap::new()).await;
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn webhook_posts_json_or_reports_failure() {
        let hook = Webhook::new("http://127.0.0.1:1/hook".to_owned());
        assert!(format!("{hook:?}").contains("hook"));
        assert!(
            hook.post(&RigName::try_new("toy").expect("r"), "hi")
                .await
                .is_err()
        );
    }
}
