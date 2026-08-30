//! The web console: A2UI envelopes over the same read models, one action endpoint, and a
//! static renderer page. Agents can consume `/rigs/<rig>/ui` exactly like the browser does.

use app::remote::a2ui::{UiAction, console_surface, parse_action};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use domain::{Principal, RigName};
use serde_json::Value;

use crate::rpc::{self, RpcError, obj};
use crate::server::{AppState, principal, rpc_response};

const PAGE: &str = include_str!("../static/console.html");

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/a2ui", get(page))
        .route("/rigs/{rig}/ui", get(surface))
        .route("/rigs/{rig}/ui/action", post(action))
}

async fn page() -> Response {
    ([(header::CACHE_CONTROL, "no-store")], Html(PAGE)).into_response()
}

fn resolve_rig(s: &AppState, rig: &str) -> Option<(RigName, app::Rig)> {
    RigName::try_new(rig)
        .ok()
        .and_then(|n| s.registry.rig(&n).map(|r| (n, r)))
}

async fn envelopes(
    s: &AppState,
    rig: &app::Rig,
    name: &RigName,
    who: &Principal,
) -> Result<Vec<Value>, RpcError> {
    let tasks = app::list_tasks(rig, s.clock.as_ref(), who).await?;
    Ok(console_surface(name.as_ref(), &tasks))
}

fn ui_response(id: &Value, result: Result<Vec<Value>, RpcError>) -> Response {
    rpc_response(id, result.map(Value::Array)).into_response()
}

async fn surface(
    State(s): State<AppState>,
    Path(rig): Path<String>,
    headers: HeaderMap,
) -> Response {
    let who = match principal(&s, &headers) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let Some((name, rig)) = resolve_rig(&s, &rig) else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response();
    };
    match envelopes(&s, &rig, &name, &who).await {
        Ok(v) => Json(Value::Array(v)).into_response(),
        Err(e) => ui_response(&Value::Null, Err(e)),
    }
}

#[derive(Debug, serde::Deserialize)]
struct ClientAction {
    name: String,
    #[serde(default)]
    context: Value,
}

/// Apply one client action, then answer with the refreshed surface.
async fn action(
    State(s): State<AppState>,
    Path(rig): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let who = match principal(&s, &headers) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    let Some((name, rig)) = resolve_rig(&s, &rig) else {
        return (
            StatusCode::NOT_FOUND,
            Json(obj([("error", "no such rig".into())])),
        )
            .into_response();
    };
    let act: ClientAction = match serde_json::from_slice(&body) {
        Ok(a) => a,
        Err(e) => {
            return ui_response(
                &Value::Null,
                Err(RpcError::new(rpc::INVALID_PARAMS, e.to_string())),
            );
        }
    };
    let ui = match parse_action(&act.name, &act.context) {
        Ok(u) => u,
        Err(e) => {
            return ui_response(
                &Value::Null,
                Err(RpcError::new(rpc::INVALID_PARAMS, e.to_string())),
            );
        }
    };
    let clock = s.clock.as_ref();
    let outcome = match ui {
        UiAction::Plan { text } => app::send_message(&rig, clock, &who, None, &text)
            .await
            .map(|_| ()),
        UiAction::Resolve { id, note } => app::send_message(&rig, clock, &who, Some(&id), &note)
            .await
            .map(|_| ()),
        UiAction::Stop { id } => app::cancel_task(&rig, clock, &who, &id).await.map(|_| ()),
        UiAction::Refresh => Ok(()),
    };
    if let Err(e) = outcome {
        return ui_response(&Value::Null, Err(e.into()));
    }
    match envelopes(&s, &rig, &name, &who).await {
        Ok(v) => Json(Value::Array(v)).into_response(),
        Err(e) => ui_response(&Value::Null, Err(e)),
    }
}
