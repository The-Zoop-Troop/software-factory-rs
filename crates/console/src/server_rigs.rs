//! `GET /rigs`: every visible rig's overview, read concurrently.

use super::*;

/// How long `/rigs` waits for one rig's overview before listing it as slow. Embedded `bd`
/// calls take seconds under contention; the page must not wait on the slowest ledger.
const OVERVIEW_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

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
            if let Err(u) = rig.probe.available() {
                return obj([
                    ("rig", name.to_string().into()),
                    ("error", u.to_string().into()),
                    ("unavailable", true.into()),
                ]);
            }
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

/// `GET /rigs/{rig}/epics/{id}/events`: every record in the rig's log that belongs to the epic
/// (the epic itself or any bead under it), oldest first. History, not a stream — read from the
/// file each time, so a closed epic on a stopped rig still shows its whole timeline.
pub(super) async fn epic_events(
    State(s): State<AppState>,
    Path((rig, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let who = match principal(&s, &headers) {
        Err(r) => return *r,
        Ok(p) => p,
    };
    let Some(rig) = RigName::try_new(&rig).ok().and_then(|n| s.registry.rig(&n)) else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response();
    };
    if let Err(e) = domain::require(&who, &rig.name, domain::Scope::Watch) {
        return (
            StatusCode::FORBIDDEN,
            Json(obj([("error", e.to_string().into())])),
        )
            .into_response();
    }
    let under = format!("{id}.");
    match rig.events.read_from(0).await {
        Ok((all, _)) => {
            let events: Vec<Value> = all
                .iter()
                .filter(|r| {
                    r.bead
                        .as_ref()
                        .is_some_and(|b| b.as_ref() == id || b.as_ref().starts_with(&under))
                })
                .map(val)
                .collect();
            Json(obj([("epic", id.into()), ("events", Value::Array(events))])).into_response()
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(obj([("error", e.to_string().into())])),
        )
            .into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct MetricsQuery {
    pub epic: Option<String>,
}

/// `GET /rigs/{rig}/metrics?epic=<id>`: the throughput report for one epic, or for every epic in
/// the log when `epic` is omitted (`app::metrics`).
pub(super) async fn metrics(
    State(s): State<AppState>,
    Path(rig): Path<String>,
    Query(q): Query<MetricsQuery>,
    headers: HeaderMap,
) -> Response {
    let who = match principal(&s, &headers) {
        Err(r) => return *r,
        Ok(p) => p,
    };
    let Some(rig) = RigName::try_new(&rig).ok().and_then(|n| s.registry.rig(&n)) else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response();
    };
    if let Err(e) = domain::require(&who, &rig.name, domain::Scope::Watch) {
        return (
            StatusCode::FORBIDDEN,
            Json(obj([("error", e.to_string().into())])),
        )
            .into_response();
    }
    match rig.events.read_from(0).await {
        Ok((log, _)) => {
            let ids = match q.epic {
                Some(e) => vec![e],
                None => app::metrics::epics_in(&log),
            };
            let epics: Vec<Value> = ids
                .iter()
                .map(|e| val(&app::metrics::epic(e, &log)))
                .collect();
            Json(obj([
                ("rig", rig.name.to_string().into()),
                ("epics", Value::Array(epics)),
            ]))
            .into_response()
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(obj([("error", e.to_string().into())])),
        )
            .into_response(),
    }
}

/// Event kinds after which a task object differs; mirror of the client's old refresh set.
const TASK_CHANGING: [&str; 10] = [
    "claimed",
    "submitted",
    "released",
    "verified",
    "verify_blocked",
    "integrated",
    "lease_reaped",
    "epic_closed",
    "task_planned",
    "merge_bead_repaired",
];

/// The epic (or plan request) a record's bead belongs to.
fn owner_of(bead: &str) -> &str {
    bead.rsplit_once('.').map_or(bead, |(epic, _)| epic)
}

/// A `task_update` frame carrying the fresh task read model after `record`, when it changed one.
/// `remote` events point at plan requests; everything else resolves to the bead's epic.
pub(crate) async fn task_update_frame(
    rig: &app::remote::Rig,
    clock: &dyn app::Clock,
    who: &app::domain::Principal,
    cursor: u64,
    record: &app::remote::EventRecord,
) -> Option<Event> {
    let bead = record.bead.as_ref()?;
    let id = if record.kind == "remote" {
        bead.as_ref().to_owned()
    } else if TASK_CHANGING.contains(&record.kind.as_str()) {
        owner_of(bead.as_ref()).to_owned()
    } else {
        return None;
    };
    let task = app::get_task(rig, clock, who, &id).await.ok()?;
    Some(
        Event::default().event("factory").data(
            obj([
                ("rig", rig.name.to_string().into()),
                ("cursor", cursor.into()),
                ("replay", Value::Bool(false)),
                (
                    "record",
                    obj([
                        ("at", record.at.clone().into()),
                        ("actor", "console".into()),
                        ("bead", id.into()),
                        ("kind", "task_update".into()),
                        ("task", val(&task)),
                    ]),
                ),
            ])
            .to_string(),
        ),
    )
}
