//! Cross-rig dependency sweep tests: release on landed needs, a question on a canceled one.

use std::collections::BTreeMap;

use app::BeadStore as _;
use app::domain::{RigName, Timestamp};
use app::testing::FixedClock;
use app::testing::remote::{FakePlanner, FakeRegistry, rig};

use crate::server_tests::id;

#[tokio::test]
async fn deferred_requests_are_released_with_upstream_contracts_when_needs_close() {
    use app::domain::CrossRigNeed;
    let (up, up_store, _, _) = rig("up", FakePlanner::returning("x"));
    let (down, down_store, down_sink, _) = rig("down", FakePlanner::returning("y"));
    let registry = FakeRegistry(BTreeMap::from([
        (up.name.clone(), up.clone()),
        (down.name.clone(), down.clone()),
    ]));
    up_store.seed_epic(id("up-1"), &[]).await;
    let need = CrossRigNeed {
        rig: RigName::try_new("up").expect("r"),
        epic: id("up-1"),
    };
    let request = down_store
        .create(app::plan_request_with_needs(
            "Portal: use the new balance API",
            "phone",
            vec![need.clone()],
        ))
        .await
        .expect("create");
    assert_eq!(
        down_store.show(&request).await.expect("show").status,
        app::BeadStatus::Deferred
    );
    let clock = FixedClock(Timestamp::from_unix_seconds(5));
    // Upstream still open: nothing released, request untouched.
    assert!(
        crate::deps::sweep_rig(&registry, &down, &clock)
            .await
            .is_empty()
    );
    // Upstream closes with a contract child.
    up_store
        .create(app::NewBead {
            title: app::domain::Title::derived("contract: up-1"),
            description: "# Contract\npub fn balance()".into(),
            kind: app::domain::BeadKind::Contract,
            priority: app::domain::Priority::LOW,
            parent: Some(id("up-1")),
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await
        .expect("contract");
    up_store.close(&id("up-1"), "done").await.expect("close");
    let released = crate::deps::sweep_rig(&registry, &down, &clock).await;
    assert_eq!(released, vec![request.clone()]);
    let bead = down_store.show(&request).await.expect("show");
    assert_eq!(bead.status, app::BeadStatus::Open);
    assert!(
        bead.description.contains("## Upstream contracts")
            && bead.description.contains("pub fn balance()")
            && bead.description.contains("up/up-1")
    );
    assert!(down_sink.events().await.iter().any(
        |e| matches!(&e.kind, app::EventKind::Remote { action, .. } if action == "deps_ready")
    ));
    // A second sweep finds nothing deferred.
    assert!(
        crate::deps::sweep_rig(&registry, &down, &clock)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn a_canceled_upstream_epic_raises_one_question_on_the_dependent_rig() {
    use app::domain::CrossRigNeed;
    let (up, up_store, _, _) = rig("up", FakePlanner::returning("x"));
    let (down, down_store, down_sink, _) = rig("down", FakePlanner::returning("y"));
    let registry = FakeRegistry(BTreeMap::from([
        (up.name.clone(), up.clone()),
        (down.name.clone(), down.clone()),
    ]));
    up_store.seed_epic(id("up-1"), &[]).await;
    up_store
        .label(&id("up-1"), app::remote::a2a::CANCELED_LABEL)
        .await
        .expect("label");
    up_store.close(&id("up-1"), "stopped").await.expect("close");
    let need = CrossRigNeed {
        rig: RigName::try_new("up").expect("r"),
        epic: id("up-1"),
    };
    let request = down_store
        .create(app::plan_request_with_needs(
            "Portal after the backend",
            "phone",
            vec![need],
        ))
        .await
        .expect("create");
    let clock = FixedClock(Timestamp::from_unix_seconds(5));
    assert!(
        crate::deps::sweep_rig(&registry, &down, &clock)
            .await
            .is_empty()
    );
    assert!(
        crate::deps::sweep_rig(&registry, &down, &clock)
            .await
            .is_empty()
    );
    let questions = down_store
        .list_active(app::domain::BeadKind::Question)
        .await
        .expect("list");
    assert_eq!(questions.len(), 1, "raised once, not per sweep");
    let q = &questions[0];
    assert!(
        q.title
            .starts_with(app::remote::attention::UPSTREAM_FAILED_PREFIX)
    );
    assert_eq!(
        app::remote::attention::upstream_failure(q),
        Some((request.clone(), "up/up-1".to_owned()))
    );
    assert!(down_sink.events().await.iter().any(
        |e| matches!(&e.kind, app::EventKind::Remote { action, .. } if action == "deps_failed")
    ));
    // The request is still deferred; the operator decides through the inbox.
    assert_eq!(
        down_store.show(&request).await.expect("show").status,
        app::BeadStatus::Deferred
    );
}
