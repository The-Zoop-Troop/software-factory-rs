//! HTTP surface: Agent Cards, one JSON-RPC endpoint per rig, SSE for `SubscribeToTask`.
//! Auth is a bearer token on every request except the cards.

use std::sync::Arc;
use std::time::Duration;

use app::remote::a2a::{A2aState, AgentSkill, skills};
use app::{Authenticator, Clock, Rig, RigRegistry};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use domain::{Principal, RigName};
use futures::StreamExt as _;
use futures::stream::Stream;
use serde_json::Value;

use crate::rpc::{self, Call, RpcError, obj, val};

#[derive(Clone)]
pub(crate) struct AppState {
    pub auth: Arc<dyn Authenticator>,
    pub registry: Arc<dyn RigRegistry>,
    pub clock: Arc<dyn Clock>,
    pub public_url: String,
    /// How often SSE streams re-read the event log.
    pub poll: Duration,
}

impl core::fmt::Debug for AppState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AppState")
            .field("public_url", &self.public_url)
            .finish_non_exhaustive()
    }
}

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/.well-known/agent-card.json", get(root_card))
        .route("/rigs", get(list_rigs))
        .route("/whoami", get(whoami))
        .route("/events", get(all_events))
        .route("/rigs/{rig}/events", get(rig_events))
        .route("/rigs/{rig}/.well-known/agent-card.json", get(rig_card))
        .route("/rigs/{rig}/a2a", post(a2a))
        .merge(crate::ui::routes())
        .merge(crate::webapp::routes())
        .with_state(state)
}

/// A2A Agent Card (§8). `rigs` is our extension on the root card.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentCard {
    pub name: String,
    pub description: String,
    pub version: String,
    pub supported_interfaces: Vec<AgentInterface>,
    pub provider: Provider,
    pub capabilities: Capabilities,
    pub security_schemes: Value,
    pub security_requirements: Vec<Value>,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rigs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInterface {
    pub url: String,
    pub protocol_binding: String,
    pub protocol_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Provider {
    pub organization: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Capabilities {
    pub streaming: bool,
    pub push_notifications: bool,
    pub extended_agent_card: bool,
    pub extensions: Vec<Value>,
}

fn base_card(name: String, description: String) -> AgentCard {
    AgentCard {
        name,
        description,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        supported_interfaces: Vec::new(),
        provider: Provider {
            organization: "The Zoop Troop".to_owned(),
            url: "https://github.com/The-Zoop-Troop/software-factory-rs".to_owned(),
        },
        capabilities: Capabilities {
            streaming: true,
            push_notifications: false,
            extended_agent_card: false,
            extensions: vec![obj([
                ("uri", app::remote::a2ui::EXTENSION.into()),
                (
                    "description",
                    "GET /rigs/<rig>/ui returns the console as A2UI envelopes; POST /rigs/<rig>/ui/action takes A2UI actions".into(),
                ),
                ("required", Value::Bool(false)),
            ])],
        },
        security_schemes: obj([(
            "bearer",
            obj([("httpAuthSecurityScheme", obj([("scheme", "bearer".into())]))]),
        )]),
        security_requirements: vec![obj([("bearer", Value::Array(Vec::new()))])],
        default_input_modes: vec!["text/plain".to_owned()],
        default_output_modes: vec!["application/json".to_owned()],
        skills: Vec::new(),
        rigs: Vec::new(),
    }
}

/// The card for one rig: JSON-RPC binding, streaming, bearer auth, the factory skills.
pub(crate) fn agent_card(public_url: &str, rig: &RigName) -> AgentCard {
    AgentCard {
        supported_interfaces: vec![AgentInterface {
            url: format!("{public_url}/rigs/{rig}/a2a"),
            protocol_binding: "JSONRPC".to_owned(),
            protocol_version: "1.0".to_owned(),
        }],
        skills: skills(),
        ..base_card(
            format!("factory rig {rig}"),
            "Autonomous software factory rig: plan in, verified code out. Humans plan, watch, answer, resolve, stop.".to_owned(),
        )
    }
}

async fn root_card(State(s): State<AppState>) -> Json<AgentCard> {
    Json(AgentCard {
        rigs: s
            .registry
            .names()
            .iter()
            .map(|r| format!("{}/rigs/{r}/.well-known/agent-card.json", s.public_url))
            .collect(),
        ..base_card(
            "factory console".to_owned(),
            "Control plane over factory rigs. Each rig has its own Agent Card under /rigs/<name>/.well-known/agent-card.json.".to_owned(),
        )
    })
}

pub(crate) fn principal(s: &AppState, headers: &HeaderMap) -> Result<Principal, Box<Response>> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    bearer.and_then(|t| s.auth.authenticate(t)).ok_or_else(|| {
        Box::new(
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                Json(obj([("error", "missing or unknown bearer token".into())])),
            )
                .into_response(),
        )
    })
}

/// The token's identity and grants, so a client can shape itself to what it may do.
async fn whoami(State(s): State<AppState>, headers: HeaderMap) -> Response {
    match principal(&s, &headers) {
        Err(r) => *r,
        Ok(p) => {
            let grants: Vec<Value> = p
                .grants
                .iter()
                .map(|(rig, scopes)| {
                    obj([
                        ("rig", rig.to_string().into()),
                        (
                            "scopes",
                            Value::Array(scopes.iter().map(|sc| sc.as_str().into()).collect()),
                        ),
                    ])
                })
                .collect();
            Json(obj([
                ("client", p.client.to_string().into()),
                ("grants", Value::Array(grants)),
            ]))
            .into_response()
        }
    }
}

/// Visible rigs with counts (epics, working, attention, done); a rig that cannot be read
/// right now is listed with `error` instead of counts.
async fn list_rigs(State(s): State<AppState>, headers: HeaderMap) -> Response {
    let p = match principal(&s, &headers) {
        Err(r) => return *r,
        Ok(p) => p,
    };
    let mut names: Vec<Value> = Vec::new();
    let mut counts: Vec<Value> = Vec::new();
    for name in s
        .registry
        .names()
        .iter()
        .filter(|r| p.allows(r, domain::Scope::Watch))
    {
        names.push(Value::String(name.to_string()));
        let Some(rig) = s.registry.rig(name) else {
            continue;
        };
        match app::overview(&rig, s.clock.as_ref(), &p).await {
            Ok(o) => counts.push(val(&o)),
            Err(e) => counts.push(obj([
                ("rig", name.to_string().into()),
                ("error", e.to_string().into()),
            ])),
        }
    }
    Json(obj([
        ("rigs", Value::Array(names)),
        ("overview", Value::Array(counts)),
    ]))
    .into_response()
}

async fn rig_card(State(s): State<AppState>, Path(rig): Path<String>) -> Response {
    match RigName::try_new(&rig)
        .ok()
        .and_then(|n| s.registry.rig(&n).map(|_| n))
    {
        Some(name) => Json(agent_card(&s.public_url, &name)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response(),
    }
}

fn status_for(code: i32) -> StatusCode {
    if code == rpc::FORBIDDEN {
        StatusCode::FORBIDDEN
    } else if code == rpc::TASK_NOT_FOUND {
        StatusCode::NOT_FOUND
    } else if code == rpc::INTERNAL {
        StatusCode::INTERNAL_SERVER_ERROR
    } else if [rpc::METHOD_NOT_FOUND, rpc::INVALID_PARAMS, rpc::UNSUPPORTED].contains(&code) {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    }
}

pub(crate) fn rpc_response(id: &Value, result: Result<Value, RpcError>) -> Response {
    match result {
        Ok(v) => Json(rpc::ok(id, v)).into_response(),
        Err(e) => (status_for(e.code), Json(rpc::err(id, &e))).into_response(),
    }
}

async fn a2a(
    State(s): State<AppState>,
    Path(rig): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let who = match principal(&s, &headers) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let req: rpc::Request = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return rpc_response(&Value::Null, Err(RpcError::new(-32700, e.to_string())));
        }
    };
    // Unknown rigs read as "not found" whether or not the client could see them (A2A §13.1).
    let Some(rig) = RigName::try_new(&rig).ok().and_then(|n| s.registry.rig(&n)) else {
        return rpc_response(
            &req.id,
            Err(RpcError::new(rpc::TASK_NOT_FOUND, "no such rig")),
        );
    };
    let call = match rpc::decode(&req) {
        Ok(c) => c,
        Err(e) => return rpc_response(&req.id, Err(e)),
    };
    if let Call::Subscribe { id } = call {
        return subscribe(s, rig, who, req.id, id).await;
    }
    rpc_response(
        &req.id,
        rpc::execute(&rig, s.clock.as_ref(), &who, call).await,
    )
}

fn frame(v: &Value) -> Event {
    Event::default().data(v.to_string())
}

/// `SubscribeToTask`: the current `Task`, then one `TaskStatusUpdateEvent` per event-log
/// record in the task's context, closing once the task is terminal.
async fn subscribe(
    s: AppState,
    rig: Rig,
    who: Principal,
    req_id: Value,
    task_id: String,
) -> Response {
    let first = match app::get_task(&rig, s.clock.as_ref(), &who, &task_id).await {
        Ok(t) => t,
        Err(e) => return rpc_response(&req_id, Err(e.into())),
    };
    let done = first.status.state.is_terminal();
    let head = futures::stream::iter([Ok(frame(&rpc::ok(&req_id, obj([("task", val(&first))]))))]);
    let stream = head.chain(if done {
        futures::stream::empty().boxed()
    } else {
        updates(s, rig, who, req_id, task_id).boxed()
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn status_update(
    task_id: &str,
    is_final: bool,
    state: A2aState,
    at: &str,
    event: Option<Value>,
) -> Value {
    let mut fields = vec![
        ("taskId", Value::String(task_id.to_owned())),
        ("contextId", Value::String(task_id.to_owned())),
        ("final", Value::Bool(is_final)),
        (
            "status",
            obj([("state", val(&state)), ("timestamp", at.into())]),
        ),
    ];
    if let Some(ev) = event {
        fields.push(("metadata", obj([("event", ev)])));
    }
    Value::Object(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
        .pipe(|update| obj([("statusUpdate", update)]))
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl Pipe for Value {}

fn updates(
    s: AppState,
    rig: Rig,
    who: Principal,
    req_id: Value,
    task_id: String,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    futures::stream::unfold((0u64, false), move |(cursor, finished)| {
        let (s, rig, who, req_id, task_id) = (
            s.clone(),
            rig.clone(),
            who.clone(),
            req_id.clone(),
            task_id.clone(),
        );
        async move {
            if finished {
                return None;
            }
            let (records, next) =
                match app::events_after(&rig, s.clock.as_ref(), &who, cursor, Some(&task_id)).await
                {
                    Ok(r) => r,
                    Err(e) => {
                        return Some((
                            vec![Ok(frame(&rpc::err(&req_id, &e.into())))],
                            (cursor, true),
                        ));
                    }
                };
            let task = app::get_task(&rig, s.clock.as_ref(), &who, &task_id)
                .await
                .ok();
            let state = task.as_ref().map_or(A2aState::Failed, |t| t.status.state);
            let terminal = state.is_terminal();
            let mut events: Vec<Result<Event, std::convert::Infallible>> = records
                .iter()
                .map(|r| {
                    Ok(frame(&rpc::ok(
                        &req_id,
                        status_update(&task_id, false, state, &r.at, Some(val(r))),
                    )))
                })
                .collect();
            if terminal {
                let at = s.clock.now().to_string();
                events.push(Ok(frame(&rpc::ok(
                    &req_id,
                    status_update(&task_id, true, state, &at, None),
                ))));
            } else if events.is_empty() {
                s.clock
                    .sleep(domain::Duration::from_seconds(s.poll.as_secs()))
                    .await;
            }
            Some((events, (next, terminal)))
        }
    })
    .flat_map(futures::stream::iter)
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct EventsQuery {
    /// Byte cursor into the rig's log; omit to start from now (only new events).
    pub cursor: Option<u64>,
    /// Bearer token as a query parameter: browsers' `EventSource` cannot set headers.
    /// Accepted on the event-stream endpoints only.
    pub token: Option<String>,
    /// Without `cursor`: replay this many of the most recent records before going live.
    pub backlog: Option<usize>,
}

/// The stream's starting point and the records to replay first.
async fn stream_start(rig: &Rig, q: &EventsQuery) -> (u64, Vec<app::EventRecord>) {
    match q.cursor {
        Some(c) => (c, Vec::new()),
        None => match rig.events.read_from(0).await {
            Ok((all, end)) => {
                let len = all.len();
                let n = q.backlog.unwrap_or(0).min(len);
                (end, all.into_iter().skip(len - n).collect())
            }
            Err(_) => (0, Vec::new()),
        },
    }
}

fn principal_for_stream(
    s: &AppState,
    headers: &HeaderMap,
    q: &EventsQuery,
) -> Result<Principal, Box<Response>> {
    match q.token.as_deref().and_then(|t| s.auth.authenticate(t)) {
        Some(p) => Ok(p),
        None => principal(s, headers),
    }
}

fn event_frame(rig: &RigName, cursor: u64, record: &app::EventRecord, replay: bool) -> Event {
    Event::default().event("factory").data(
        obj([
            ("rig", rig.to_string().into()),
            ("cursor", cursor.into()),
            ("replay", Value::Bool(replay)),
            ("record", val(record)),
        ])
        .to_string(),
    )
}

/// One rig's event log as SSE (`event: factory`, data `{rig, cursor, record}`), forever.
/// Without `cursor` the stream starts at the current end of the log.
async fn rig_events(
    State(s): State<AppState>,
    Path(rig): Path<String>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> Response {
    let who = match principal_for_stream(&s, &headers, &q) {
        Ok(p) => p,
        Err(r) => return *r,
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
    let start = match q.cursor {
        Some(c) => c,
        None => rig
            .events
            .read_from(u64::MAX)
            .await
            .map_or(0, |(_, end)| end),
    };
    Sse::new(rig_stream(s, rig, who, start))
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn rig_stream(
    s: AppState,
    rig: Rig,
    who: Principal,
    start: u64,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    futures::stream::unfold(start, move |cursor| {
        let (s, rig, who) = (s.clone(), rig.clone(), who.clone());
        async move {
            let (records, next) =
                match app::events_after(&rig, s.clock.as_ref(), &who, cursor, None).await {
                    Ok(r) => r,
                    Err(e) => {
                        let ev = Event::default().event("error").data(e.to_string());
                        return Some((vec![Ok(ev)], cursor));
                    }
                };
            let events: Vec<Result<Event, std::convert::Infallible>> = records
                .iter()
                .map(|r| Ok(event_frame(&rig.name, next, r, false)))
                .collect();
            if events.is_empty() {
                s.clock
                    .sleep(domain::Duration::from_seconds(s.poll.as_secs()))
                    .await;
            }
            Some((events, next))
        }
    })
    .flat_map(futures::stream::iter)
}

/// Every visible rig's new events, merged into one SSE stream (`cursor` applies to all rigs;
/// omit it to start from now).
async fn all_events(
    State(s): State<AppState>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> Response {
    let who = match principal_for_stream(&s, &headers, &q) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let mut streams = Vec::new();
    for name in s.registry.names() {
        if !who.allows(&name, domain::Scope::Watch) {
            continue;
        }
        let Some(rig) = s.registry.rig(&name) else {
            continue;
        };
        let (start, backlog) = stream_start(&rig, &q).await;
        let replay: Vec<Result<Event, std::convert::Infallible>> = backlog
            .iter()
            .map(|r| Ok(event_frame(&rig.name, start, r, true)))
            .collect();
        streams.push(
            futures::stream::iter(replay)
                .chain(rig_stream(s.clone(), rig, who.clone(), start))
                .boxed(),
        );
    }
    Sse::new(futures::stream::select_all(streams))
        .keep_alive(KeepAlive::default())
        .into_response()
}
