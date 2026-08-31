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

#[derive(Debug, serde::Deserialize)]
pub(super) struct DetailQuery {
    /// `1`: no truncation of note tails.
    pub full: Option<u8>,
}

/// `GET /rigs/{rig}/beads/{id}`: everything an operator reads on one bead — fields, task
/// metadata, the paired verify commands, and the notes parsed into segments.
pub(super) async fn bead_detail(
    State(s): State<AppState>,
    Path((rig, id)): Path<(String, String)>,
    Query(q): Query<DetailQuery>,
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
    let Ok(bead_id) = app::domain::BeadId::try_new(&id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such bead".into())])),
        )
            .into_response();
    };
    let bead = match rig.store.show(&bead_id).await {
        Ok(b) => b,
        Err(app::StoreError::NotFound { .. }) => {
            return (
                StatusCode::NOT_FOUND,
                Json(obj([("error", "no such bead".into())])),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(obj([("error", e.to_string().into())])),
            )
                .into_response();
        }
    };
    let verify = verify_json(&rig, &bead).await;
    let mut notes = crate::detail::parse_notes(bead.notes.as_deref().unwrap_or(""));
    if q.full != Some(1) {
        crate::detail::truncate(&mut notes, 2000);
    }
    let meta = task_meta_json(&bead);
    Json(obj([
        ("id", bead.id.to_string().into()),
        ("kind", bead.kind.map_or(Value::Null, |k| k.as_str().into())),
        ("title", bead.title.clone().into()),
        ("status", format!("{:?}", bead.status).to_lowercase().into()),
        (
            "parent",
            bead.parent
                .as_ref()
                .map_or(Value::Null, |p| p.to_string().into()),
        ),
        ("description", bead.description.clone().into()),
        (
            "acceptance",
            bead.acceptance.clone().map_or(Value::Null, Value::String),
        ),
        ("task", meta.unwrap_or(Value::Null)),
        ("verify", verify.unwrap_or(Value::Null)),
        ("notes", serde_json::to_value(&notes).unwrap_or(Value::Null)),
        (
            "needs",
            bead.cross_needs.as_ref().map_or(Value::Null, |n| {
                Value::Array(
                    n.iter()
                        .map(|x| format!("{}/{}", x.rig, x.epic).into())
                        .collect(),
                )
            }),
        ),
    ]))
    .into_response()
}

/// The verify commands paired with a task (or carried by a verify bead itself).
async fn verify_json(rig: &app::remote::Rig, bead: &app::Bead) -> Option<Value> {
    let render = |v: &app::domain::VerifyMeta| {
        obj([
            (
                "commands",
                Value::Array(v.commands.iter().map(|c| c.to_string().into()).collect()),
            ),
            ("timeout_seconds", v.timeout.seconds().into()),
        ])
    };
    match bead.meta.as_ref() {
        Some(m) => rig
            .store
            .show(&m.verify_bead)
            .await
            .ok()
            .and_then(|v| v.verify)
            .map(|v| render(&v)),
        None => bead.verify.as_ref().map(render),
    }
}

/// Task metadata as the drawer shows it: state, lease, branch, landed sha, budget vs usage.
fn task_meta_json(bead: &app::Bead) -> Option<Value> {
    bead.meta.as_ref().map(|m| {
        let (landed, lease) = match &m.state {
            app::domain::TaskState::Closed { merged } => (Some(merged.to_string()), None),
            app::domain::TaskState::Leased { lease } => (
                None,
                Some(obj([
                    ("holder", lease.holder.to_string().into()),
                    ("expires", lease.expires.unix_seconds().into()),
                ])),
            ),
            app::domain::TaskState::Open
            | app::domain::TaskState::InVerify { .. }
            | app::domain::TaskState::Mergeable { .. }
            | app::domain::TaskState::Incident { .. } => (None, None),
        };
        obj([
            ("state", m.state.name().into()),
            ("base", m.base.to_string().into()),
            ("branch", format!("task/{}", bead.id).into()),
            ("landed", landed.map_or(Value::Null, Value::String)),
            ("lease", lease.unwrap_or(Value::Null)),
            (
                "budget",
                obj([
                    ("tokens", m.budget.tokens.get().into()),
                    (
                        "attempts",
                        u64::from(app::domain::Attempts::get(m.budget.attempts)).into(),
                    ),
                    ("wall_clock_seconds", m.budget.wall_clock.seconds().into()),
                ]),
            ),
            (
                "usage",
                obj([
                    ("tokens", m.usage.tokens.get().into()),
                    (
                        "attempts",
                        u64::from(app::domain::Attempts::get(m.usage.attempts)).into(),
                    ),
                    ("wall_clock_seconds", m.usage.wall_clock.seconds().into()),
                ]),
            ),
        ])
    })
}

/// Sum every epic's metrics fold into one lifetime rollup for the rig.
fn lifetime_rollup(log: &[app::EventRecord]) -> Value {
    let ids = app::metrics::epics_in(log);
    let reports: Vec<_> = ids.iter().map(|e| app::metrics::epic(e, log)).collect();
    obj([
        ("epics", reports.len().into()),
        (
            "tasks_landed",
            reports.iter().map(|m| m.landed).sum::<usize>().into(),
        ),
        (
            "tasks_planned",
            reports.iter().map(|m| m.tasks.len()).sum::<usize>().into(),
        ),
        (
            "first_pass",
            reports.iter().map(|m| m.first_pass).sum::<usize>().into(),
        ),
        (
            "tokens",
            reports.iter().map(|m| m.tokens).sum::<u64>().into(),
        ),
        (
            "work_seconds",
            reports.iter().map(|m| m.work).sum::<i64>().into(),
        ),
        (
            "retry_tax_seconds",
            reports.iter().map(|m| m.retry_tax).sum::<i64>().into(),
        ),
    ])
}

/// `GET /rigs/{rig}/detail`: host facts, posture (probe + ledger latency), event-log stats,
/// budget, and a lifetime rollup folded from the whole log.
pub(super) async fn rig_detail(
    State(s): State<AppState>,
    Path(rig): Path<String>,
    headers: HeaderMap,
) -> Response {
    let who = match principal(&s, &headers) {
        Err(r) => return *r,
        Ok(p) => p,
    };
    let Some(r) = RigName::try_new(&rig).ok().and_then(|n| s.registry.rig(&n)) else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response();
    };
    if let Err(e) = domain::require(&who, &r.name, domain::Scope::Watch) {
        return (
            StatusCode::FORBIDDEN,
            Json(obj([("error", e.to_string().into())])),
        )
            .into_response();
    }
    let facts = s.facts.get(&rig).map_or(Value::Null, |f| {
        serde_json::to_value(f).unwrap_or(Value::Null)
    });
    let posture = match r.probe.available() {
        Ok(()) => "available",
        Err(u) if u.reason.contains("never run") => "never-ran",
        Err(_) => "stopped",
    };
    // Ledger latency: one timed read; only meaningful when the probe says available.
    let started = std::time::Instant::now();
    let ledger_ms = if posture == "available" {
        match r.store.list_active(app::domain::BeadKind::Epic).await {
            Ok(_) => u64::try_from(started.elapsed().as_millis()).ok(),
            Err(_) => None,
        }
    } else {
        None
    };
    let (log, _) = r.events.read_from(0).await.unwrap_or_default();
    let last_at = log.iter().rev().find_map(|e| e.at.parse::<i64>().ok());
    let rollup = lifetime_rollup(&log);
    Json(obj([
        ("rig", rig.clone().into()),
        ("facts", facts),
        ("posture", posture.into()),
        ("ledger_ms", ledger_ms.map_or(Value::Null, Into::into)),
        (
            "events",
            obj([
                ("count", log.len().into()),
                ("last_at", last_at.map_or(Value::Null, Into::into)),
            ]),
        ),
        (
            "budget",
            obj([
                (
                    "max_tokens",
                    r.budget.max_tokens.map_or(Value::Null, |t| t.get().into()),
                ),
                (
                    "max_usd_micros",
                    r.budget.max_usd.map_or(Value::Null, |u| u.get().into()),
                ),
            ]),
        ),
        ("rollup", rollup),
    ]))
    .into_response()
}
