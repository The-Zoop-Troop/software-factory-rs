//! Cross-rig dependencies (`docs/exec-plans/active/cross-rig-dependencies.md`): a plan request
//! created with `needs` waits, deferred, until every epic it names has closed on its rig; then
//! its contracts are appended to the request and it is un-deferred for that rig's planner.
//! Only the console reads across rigs; rigs stay isolated.

use std::fmt::Write as _;
use std::sync::Arc;

use app::domain::{BeadId, BeadKind, CrossRigNeed, Duration, RigName};
use app::remote::{Rig, RigRegistry};
use app::{BeadStatus, Clock, EventKind, FactoryEvent};

/// What a need resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NeedState {
    /// Closed, with its contract text (empty when the epic has none).
    Landed {
        contract: String,
    },
    Waiting,
    /// The rig is unknown to this console or the epic does not exist.
    Missing,
}

async fn resolve(registry: &dyn RigRegistry, need: &CrossRigNeed) -> NeedState {
    let Some(rig) = registry.rig(&need.rig) else {
        return NeedState::Missing;
    };
    let Ok(epic) = rig.store.show(&need.epic).await else {
        return NeedState::Missing;
    };
    if epic.status != BeadStatus::Closed {
        return NeedState::Waiting;
    }
    let contract = rig
        .store
        .children(&need.epic)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.kind == Some(BeadKind::Contract))
        .map(|c| c.description)
        .collect::<Vec<_>>()
        .join("\n\n");
    NeedState::Landed { contract }
}

/// The text appended to a request once its needs landed.
#[must_use]
pub(crate) fn injected(original: &str, landed: &[(CrossRigNeed, String)]) -> String {
    let mut out = original.trim_end().to_owned();
    out.push_str("\n\n## Upstream contracts (landed; build on these)\n");
    for (need, contract) in landed {
        let _ = write!(
            out,
            "\n### {}/{}\n{}\n",
            need.rig,
            need.epic,
            if contract.is_empty() {
                "(closed; no contract artifact)"
            } else {
                contract
            }
        );
    }
    out
}

/// One pass over one rig's deferred plan requests. Returns the ids un-deferred.
pub(crate) async fn sweep_rig(
    registry: &dyn RigRegistry,
    rig: &Rig,
    clock: &dyn Clock,
) -> Vec<BeadId> {
    let mut released = Vec::new();
    let Ok(requests) = rig.store.list_deferred(BeadKind::PlanRequest).await else {
        return released;
    };
    for req in requests {
        let Some(needs) = req.cross_needs.as_ref().filter(|n| !n.is_empty()) else {
            continue;
        };
        let mut landed = Vec::with_capacity(needs.len());
        let mut ready = true;
        for need in needs {
            match resolve(registry, need).await {
                NeedState::Landed { contract } => landed.push((need.clone(), contract)),
                NeedState::Waiting | NeedState::Missing => {
                    ready = false;
                    break;
                }
            }
        }
        if !ready {
            continue;
        }
        let text = injected(&req.description, &landed);
        if rig.store.set_description(&req.id, &text).await.is_err() {
            continue;
        }
        if rig.store.undefer(&req.id).await.is_err() {
            continue;
        }
        rig.sink.record(&FactoryEvent {
            at: clock.now(),
            actor: "console".to_owned(),
            bead: Some(req.id.clone()),
            kind: EventKind::Remote {
                action: "deps_ready".to_owned(),
                detail: landed
                    .iter()
                    .map(|(n, _)| format!("{}/{}", n.rig, n.epic))
                    .collect::<Vec<_>>()
                    .join(", "),
            },
        });
        released.push(req.id);
    }
    released
}

/// Every rig, forever, every `interval`.
pub(crate) async fn run(registry: Arc<dyn RigRegistry>, clock: Arc<dyn Clock>, interval: Duration) {
    loop {
        for name in registry.names() {
            let Some(rig) = registry.rig(&name) else {
                continue;
            };
            if rig.probe.available().is_err() {
                continue;
            }
            let n = sweep_rig(registry.as_ref(), &rig, clock.as_ref())
                .await
                .len();
            if n > 0 {
                tracing::info!(rig = %name, released = n, "cross-rig needs landed; plan requests released");
            }
        }
        clock.sleep(interval).await;
    }
}

#[allow(dead_code, reason = "named for the log line and tests")]
fn _rig_name(n: &RigName) -> String {
    n.to_string()
}
