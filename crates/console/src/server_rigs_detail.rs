//! Detail endpoints: one bead in depth, one rig's facts and rollup, an epic's consumers.

use super::*;

#[derive(Debug, serde::Deserialize)]
pub(super) struct DetailQuery {
    /// `1`: no truncation of note tails.
    pub full: Option<u8>,
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
    let (context, origin) = epic_extras(&rig, &bead).await;
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
        ("context", context),
        ("origin", origin),
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

/// Reference/contract children of an epic and the plan request that created it.
/// Non-epics get `(Null, Null)`.
async fn epic_extras(rig: &app::remote::Rig, bead: &app::Bead) -> (Value, Value) {
    if bead.kind != Some(app::domain::BeadKind::Epic) {
        return (Value::Null, Value::Null);
    }
    let context: Vec<Value> = rig
        .store
        .children(&bead.id)
        .await
        .unwrap_or_default()
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                Some(app::domain::BeadKind::Reference | app::domain::BeadKind::Contract)
            )
        })
        .map(|c| {
            obj([
                ("id", c.id.to_string().into()),
                ("kind", c.kind.map_or(Value::Null, |k| k.as_str().into())),
                ("title", c.title.clone().into()),
                ("text", c.description.clone().into()),
            ])
        })
        .collect();
    let origin = rig
        .store
        .list_closed(app::domain::BeadKind::PlanRequest)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|r| matches!(app::plan_outcome(r), Some(Ok(epic)) if epic == bead.id))
        .map_or(Value::Null, |r| {
            obj([
                ("id", r.id.to_string().into()),
                ("title", r.title.clone().into()),
                ("text", r.description.clone().into()),
            ])
        });
    (Value::Array(context), origin)
}

/// `GET /rigs/{rig}/epics/{id}/consumers`: plan requests on every rig whose cross-rig needs
/// name this epic — who is building on it. Rigs whose ledger cannot answer are skipped.
pub(super) async fn epic_consumers(
    State(s): State<AppState>,
    Path((rig, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let who = match principal(&s, &headers) {
        Err(r) => return *r,
        Ok(p) => p,
    };
    let Some(rig_name) = RigName::try_new(&rig).ok() else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response();
    };
    if s.registry.rig(&rig_name).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response();
    }
    if let Err(e) = domain::require(&who, &rig_name, domain::Scope::Watch) {
        return (
            StatusCode::FORBIDDEN,
            Json(obj([("error", e.to_string().into())])),
        )
            .into_response();
    }
    let Ok(epic_id) = app::domain::BeadId::try_new(&id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such bead".into())])),
        )
            .into_response();
    };
    let mut consumers = Vec::new();
    for name in s.registry.names() {
        let Some(other) = s.registry.rig(&name) else {
            continue;
        };
        if other.probe.available().is_err() {
            continue;
        }
        let mut requests = Vec::new();
        let kind = app::domain::BeadKind::PlanRequest;
        for listed in [
            other.store.list_active(kind).await,
            other.store.list_deferred(kind).await,
            other.store.list_closed(kind).await,
        ] {
            requests.extend(listed.unwrap_or_default());
        }
        for req in requests {
            let names_epic = req
                .cross_needs
                .as_ref()
                .is_some_and(|needs| needs.iter().any(|n| n.rig == rig_name && n.epic == epic_id));
            if names_epic {
                consumers.push(obj([
                    ("rig", name.to_string().into()),
                    ("id", req.id.to_string().into()),
                    ("title", req.title.clone().into()),
                    ("status", format!("{:?}", req.status).to_lowercase().into()),
                ]));
            }
        }
    }
    Json(obj([
        ("epic", id.into()),
        ("consumers", Value::Array(consumers)),
    ]))
    .into_response()
}
