//! Shared fixtures for the remote-control tests.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test support")]

use std::collections::{BTreeMap, BTreeSet};

use domain::{
    Attempts, BeadId, Budget, ClientId, Duration, FactoryMeta, Principal, Scope, Sha, TaskState,
    Timestamp, Tokens, Usage,
};

use super::Rig;
use crate::testing::remote::{FakePlanner, FakeTail, rig};
use crate::testing::{FakeStore, FixedClock, MemorySink};

pub(super) fn id(s: &str) -> BeadId {
    BeadId::try_new(s).expect("id")
}

pub(super) fn who(scopes: &[Scope]) -> Principal {
    Principal {
        client: ClientId::try_new("tester").expect("client"),
        grants: BTreeMap::from([(
            domain::RigName::try_new("toy").expect("rig"),
            scopes.iter().copied().collect::<BTreeSet<_>>(),
        )]),
    }
}

pub(super) fn meta(state: TaskState, tokens: u64) -> FactoryMeta {
    FactoryMeta {
        verify_bead: id("v-1"),
        base: Sha::try_new("0".repeat(40)).expect("sha"),
        budget: Budget {
            tokens: Tokens::new(1000),
            wall_clock: Duration::from_minutes(10),
            attempts: Attempts::new(3),
        },
        usage: Usage {
            tokens: Tokens::new(tokens),
            wall_clock: Duration::from_minutes(0),
            attempts: Attempts::new(1),
        },
        lease_expiries: Attempts::new(0),
        blocked_releases: Attempts::new(0),
        state,
    }
}

pub(super) fn clock() -> FixedClock {
    FixedClock(Timestamp::from_unix_seconds(1_700_000_000))
}

pub(super) async fn seeded() -> (
    Rig,
    std::sync::Arc<FakeStore>,
    std::sync::Arc<MemorySink>,
    std::sync::Arc<FakeTail>,
) {
    let (rig, store, sink, tail) = rig("toy", FakePlanner::returning("ep-1"));
    store.seed_epic(id("ep-1"), &[]).await;
    store
        .seed_task(
            id("ep-1.1"),
            meta(
                TaskState::Closed {
                    merged: Sha::try_new("1".repeat(40)).expect("sha"),
                },
                300,
            ),
        )
        .await;
    store
        .seed_task(id("ep-1.2"), meta(TaskState::Open, 0))
        .await;
    store.set_parent(&id("ep-1.1"), &id("ep-1")).await;
    store.set_parent(&id("ep-1.2"), &id("ep-1")).await;
    (rig, store, sink, tail)
}
