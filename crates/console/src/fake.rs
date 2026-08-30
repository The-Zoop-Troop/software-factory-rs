//! `console serve --fake`: one in-memory rig for UI work without docker or credentials.
//! Token `fake` has admin on rig `toy`; token `watcher` may only watch.

use std::collections::{BTreeMap, BTreeSet};

use app::BeadStore as _;
use app::testing::remote::{FakeAuth, FakePlanner, FakeRegistry, rig};
use domain::task::IncidentReason;
use domain::{
    Attempts, BeadId, BeadKind, Budget, ClientId, Duration, FactoryMeta, Principal, RigName, Scope,
    Sha, TaskState, Tokens, Usage,
};

fn meta(state: TaskState, tokens: u64) -> anyhow::Result<FactoryMeta> {
    Ok(FactoryMeta {
        verify_bead: BeadId::try_new("v-1")?,
        base: Sha::try_new("0".repeat(40))?,
        budget: Budget {
            tokens: Tokens::new(400_000),
            wall_clock: Duration::from_minutes(45),
            attempts: Attempts::new(3),
        },
        usage: Usage {
            tokens: Tokens::new(tokens),
            wall_clock: Duration::from_minutes(3),
            attempts: Attempts::new(1),
        },
        lease_expiries: Attempts::new(0),
        state,
    })
}

fn principal(client: &str, scopes: &[Scope]) -> anyhow::Result<Principal> {
    Ok(Principal {
        client: ClientId::try_new(client)?,
        grants: BTreeMap::from([(
            RigName::try_new("toy")?,
            scopes.iter().copied().collect::<BTreeSet<_>>(),
        )]),
    })
}

/// # Errors
/// Only if the hard-coded seed data were invalid.
pub(crate) async fn world() -> anyhow::Result<(FakeAuth, FakeRegistry)> {
    let (r, store, _, tail) = rig("toy", FakePlanner::returning("toy-new"));
    tail.push("worker-1", Some(BeadId::try_new("toy-abc.1")?), "claimed");
    tail.push("verifier", Some(BeadId::try_new("toy-abc.1")?), "verified");
    tail.push(
        "integrator",
        Some(BeadId::try_new("toy-abc.1")?),
        "integrated",
    );
    let epic = BeadId::try_new("toy-abc")?;
    store.seed_epic(epic.clone(), &[]).await;
    let merged = Sha::try_new("1".repeat(40))?;
    let conflict = IncidentReason::MergeConflict {
        detail: "conflicts in lib.sh".to_owned(),
    };
    for (t, state, tokens) in [
        ("toy-abc.1", TaskState::Closed { merged }, 12_000),
        ("toy-abc.2", TaskState::Open, 0),
        (
            "toy-abc.3",
            TaskState::Incident {
                reason: conflict.clone(),
            },
            60_000,
        ),
    ] {
        let id = BeadId::try_new(t)?;
        store.seed_task(id.clone(), meta(state, tokens)?).await;
        store.set_parent(&id, &epic).await;
    }
    store.seed_epic(BeadId::try_new("toy-new")?, &[]).await;
    store
        .create(app::NewBead {
            title: domain::Title::derived("incident on toy-abc.3"),
            description: conflict.to_string(),
            kind: BeadKind::Incident,
            priority: domain::Priority::CRITICAL,
            parent: Some(epic),
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await?;
    let auth = FakeAuth(BTreeMap::from([
        ("fake".to_owned(), principal("fake", &[Scope::Admin])?),
        ("watcher".to_owned(), principal("watcher", &[Scope::Watch])?),
    ]));
    Ok((auth, FakeRegistry(BTreeMap::from([(r.name.clone(), r)]))))
}
