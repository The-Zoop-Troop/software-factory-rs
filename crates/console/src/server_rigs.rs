//! `GET /rigs`: every visible rig's overview, read concurrently.

use super::*;

/// How long `/rigs` waits for one rig's overview before listing it as slow. Embedded `bd`
/// calls take seconds under contention; the page must not wait on the slowest ledger.
const OVERVIEW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Visible rigs with counts (epics, working, attention, done); a rig that cannot be read
/// right now is listed with `error` instead of counts. Rigs are read concurrently.
pub(super) async fn list_rigs(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let p = match principal(&s, &headers) {
        Err(r) => return *r,
        Ok(p) => p,
    };
    let visible: Vec<_> = s
        .registry
        .names()
        .into_iter()
        .filter(|r| p.allows(r, domain::Scope::Watch))
        .collect();
    let names: Vec<Value> = visible
        .iter()
        .map(|n| Value::String(n.to_string()))
        .collect();
    let reads = visible.iter().filter_map(|name| {
        let rig = s.registry.rig(name)?;
        let (clock, p) = (Arc::clone(&s.clock), p.clone());
        Some(async move {
            let read =
                tokio::time::timeout(OVERVIEW_TIMEOUT, app::overview(&rig, clock.as_ref(), &p))
                    .await;
            match read {
                Ok(Ok(o)) => val(&o),
                Ok(Err(e)) => obj([
                    ("rig", name.to_string().into()),
                    ("error", e.to_string().into()),
                ]),
                Err(_) => obj([
                    ("rig", name.to_string().into()),
                    (
                        "error",
                        format!(
                            "ledger slow: no answer within {}s",
                            OVERVIEW_TIMEOUT.as_secs()
                        )
                        .into(),
                    ),
                ]),
            }
        })
    });
    let counts = futures::future::join_all(reads).await;
    Json(obj([
        ("rigs", Value::Array(names)),
        ("overview", Value::Array(counts)),
    ]))
    .into_response()
}
