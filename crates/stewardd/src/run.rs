//! The steward loop, separated from `main` so it runs against fakes in tests.

use std::future::Future;

use infra::app::domain::Duration;
use infra::app::steward_contract::ContractSource;
use infra::app::{BeadStore, Clock, EventSink, SweepReport, sweep};

/// Sweep repeatedly until `stop` resolves or, if `once`, after a single sweep.
/// Returns the number of sweeps performed.
pub(crate) async fn steward_loop<S>(
    store: &dyn BeadStore,
    clock: &dyn Clock,
    log: &dyn EventSink,
    contracts: Option<ContractSource<'_>>,
    interval: Duration,
    once: bool,
    stop: S,
) -> u32
where
    S: Future<Output = ()>,
{
    let mut stop = std::pin::pin!(stop);
    let mut sweeps = 0u32;
    loop {
        match sweep(store, clock, log, "stewardd", contracts).await {
            Ok(report) => log_report(report),
            Err(e) => tracing::error!(error = %e, "sweep failed"),
        }
        sweeps += 1;
        if once {
            break;
        }
        tokio::select! {
            () = clock.sleep(interval) => {}
            () = &mut stop => break,
        }
    }
    tracing::info!(sweeps, "stewardd stopped");
    sweeps
}

fn log_report(report: SweepReport) {
    tracing::info!(?report, "sweep");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use infra::app::BeadStore;
    use infra::app::domain::BeadKind;
    use infra::app::domain::{
        AgentId, BeadId, Budget, FactoryMeta, Lease, Sha, TaskState, Timestamp, Usage,
    };
    use infra::app::testing::{FakeStore, FixedClock, MemorySink};

    use super::*;
    use infra::app::domain::Attempts;

    #[tokio::test]
    async fn once_sweeps_exactly_once_and_reaps() {
        let store = FakeStore::default();
        store
            .seed_task(
                BeadId::try_new("fac-1").unwrap(),
                FactoryMeta {
                    verify_bead: BeadId::try_new("fac-2").unwrap(),
                    base: Sha::try_new("a".repeat(40)).unwrap(),
                    budget: Budget::default(),
                    usage: Usage::default(),
                    lease_expiries: Attempts::new(0),
                    blocked_releases: Attempts::new(0),
                    state: TaskState::Leased {
                        lease: Lease::grant(
                            AgentId::try_new("w").unwrap(),
                            Timestamp::from_unix_seconds(0),
                            Duration::from_seconds(1),
                        ),
                    },
                },
            )
            .await;
        let log = MemorySink::default();
        let n = steward_loop(
            &store,
            &FixedClock(Timestamp::from_unix_seconds(100)),
            &log,
            None,
            Duration::from_seconds(1),
            true,
            std::future::pending(),
        )
        .await;
        assert_eq!(n, 1);
        assert_eq!(
            store.list_active(BeadKind::Task).await.unwrap()[0]
                .meta
                .as_ref()
                .unwrap()
                .state,
            TaskState::Open
        );
    }

    #[tokio::test]
    async fn stop_ends_the_loop() {
        let store = FakeStore::default();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let clock = FixedClock(Timestamp::from_unix_seconds(0));
        let log = MemorySink::default();
        let stop = async move {
            let _ = rx.await;
        };
        let handle = steward_loop(
            &store,
            &clock,
            &log,
            None,
            Duration::from_seconds(1),
            false,
            stop,
        );
        tx.send(()).unwrap();
        let n = handle.await;
        assert!(n >= 1);
    }
}
