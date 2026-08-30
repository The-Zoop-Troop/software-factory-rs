//! JSON-RPC 2.0 binding of the A2A operations (docs/references/a2a.md §3, §9). Decodes a
//! request into a typed call, runs the matching `app::remote` workflow, encodes the result.

use app::remote::a2a::{Message, Part};
use app::{AttentionOption, RemoteError, Rig, Sent};
use domain::Principal;
use serde_json::Value;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct Request {
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// A2A / JSON-RPC error codes we emit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub(crate) const INVALID_PARAMS: i32 = -32602;
pub(crate) const METHOD_NOT_FOUND: i32 = -32601;
pub(crate) const INTERNAL: i32 = -32603;
pub(crate) const TASK_NOT_FOUND: i32 = -32001;
pub(crate) const NOT_CANCELABLE: i32 = -32002;
pub(crate) const UNSUPPORTED: i32 = -32004;
/// Not in A2A; the HTTP status carries the refusal, this names it in the body.
pub(crate) const FORBIDDEN: i32 = -32040;
pub(crate) const BUDGET: i32 = -32041;

impl RpcError {
    pub(crate) fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

impl From<RemoteError> for RpcError {
    fn from(e: RemoteError) -> Self {
        let msg = e.to_string();
        match e {
            RemoteError::Forbidden(_) => Self::new(FORBIDDEN, msg),
            RemoteError::TaskNotFound { .. } => Self::new(TASK_NOT_FOUND, msg),
            RemoteError::Terminal { .. } => Self::new(NOT_CANCELABLE, msg),
            RemoteError::EmptyMessage => Self::new(INVALID_PARAMS, msg),
            RemoteError::Budget(_) => Self::new(BUDGET, msg),
            RemoteError::Submit(_) | RemoteError::Store(_) | RemoteError::Tail(_) => {
                Self::new(INTERNAL, msg)
            }
        }
    }
}

/// The typed operations. `Subscribe` is answered with SSE by the server, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Call {
    SendMessage {
        task_id: Option<String>,
        text: String,
        option: Option<AttentionOption>,
        /// A2A `configuration.returnImmediately`: queue the plan and return the request.
        return_immediately: bool,
    },
    GetTask {
        id: String,
    },
    ListTasks {
        input_required_only: bool,
    },
    CancelTask {
        id: String,
    },
    Subscribe {
        id: String,
    },
}

#[derive(Debug, serde::Deserialize)]
struct SendParams {
    message: Message,
    #[serde(default)]
    configuration: SendConfig,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendConfig {
    #[serde(default)]
    return_immediately: bool,
}

#[derive(Debug, serde::Deserialize)]
struct IdParams {
    id: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListParams {
    #[serde(default)]
    status: Option<String>,
}

/// Decode method + params into a [`Call`].
///
/// # Errors
/// `METHOD_NOT_FOUND` for unknown methods; `INVALID_PARAMS` for malformed params.
pub(crate) fn decode(req: &Request) -> Result<Call, RpcError> {
    let bad = |e: serde_json::Error| RpcError::new(INVALID_PARAMS, e.to_string());
    match req.method.as_str() {
        "SendMessage" => {
            let p: SendParams = serde_json::from_value(req.params.clone()).map_err(bad)?;
            let option = p
                .message
                .parts
                .iter()
                .find_map(|part| match part {
                    Part::Data(v) => v.get("option").and_then(serde_json::Value::as_str),
                    Part::Text(_) => None,
                })
                .map(|o| {
                    AttentionOption::parse(o)
                        .map_err(|e| RpcError::new(INVALID_PARAMS, e.to_string()))
                })
                .transpose()?;
            Ok(Call::SendMessage {
                option,
                return_immediately: p.configuration.return_immediately,
                task_id: p.message.task_id,
                text: p
                    .message
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        Part::Text(t) => Some(t.as_str()),
                        Part::Data(_) => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            })
        }
        "GetTask" => Ok(Call::GetTask {
            id: serde_json::from_value::<IdParams>(req.params.clone())
                .map_err(bad)?
                .id,
        }),
        "CancelTask" => Ok(Call::CancelTask {
            id: serde_json::from_value::<IdParams>(req.params.clone())
                .map_err(bad)?
                .id,
        }),
        "SubscribeToTask" => Ok(Call::Subscribe {
            id: serde_json::from_value::<IdParams>(req.params.clone())
                .map_err(bad)?
                .id,
        }),
        "ListTasks" => {
            let p: ListParams = if req.params.is_null() {
                ListParams::default()
            } else {
                serde_json::from_value(req.params.clone()).map_err(bad)?
            };
            Ok(Call::ListTasks {
                input_required_only: p.status.as_deref() == Some("TASK_STATE_INPUT_REQUIRED"),
            })
        }
        "SendStreamingMessage"
        | "CreateTaskPushNotificationConfig"
        | "GetTaskPushNotificationConfig"
        | "ListTaskPushNotificationConfig"
        | "DeleteTaskPushNotificationConfig"
        | "GetExtendedAgentCard" => Err(RpcError::new(
            UNSUPPORTED,
            format!("{} is not supported by this console", req.method),
        )),
        other => Err(RpcError::new(
            METHOD_NOT_FOUND,
            format!("unknown method {other}"),
        )),
    }
}

/// Run a non-streaming call against one rig.
///
/// # Errors
/// Mapped from the workflow's `RemoteError`.
pub(crate) async fn execute(
    rig: &Rig,
    clock: &dyn app::Clock,
    who: &Principal,
    call: Call,
) -> Result<Value, RpcError> {
    match call {
        Call::SendMessage {
            task_id,
            text,
            option,
            return_immediately,
        } => {
            let sent = match (option, task_id.as_deref()) {
                (Some(opt), Some(id)) => app::apply_option(rig, clock, who, id, opt, &text).await?,
                (None, None) if return_immediately => {
                    Sent::Planned(app::enqueue_plan(rig, clock, who, &text).await?)
                }
                (_, id) => app::send_message(rig, clock, who, id, &text).await?,
            };
            match sent {
                Sent::Planned(task) | Sent::Resolved { task, .. } => {
                    Ok(obj([("task", val(&task))]))
                }
            }
        }
        Call::GetTask { id } => Ok(val(&app::get_task(rig, clock, who, &id).await?)),
        Call::ListTasks {
            input_required_only,
        } => {
            let tasks = app::list_tasks(rig, clock, who).await?;
            let tasks: Vec<_> = tasks
                .into_iter()
                .filter(|t| {
                    !input_required_only
                        || t.status.state == app::remote::a2a::A2aState::InputRequired
                })
                .collect();
            Ok(obj([("tasks", val(&tasks))]))
        }
        Call::CancelTask { id } => Ok(val(&app::cancel_task(rig, clock, who, &id).await?)),
        Call::Subscribe { .. } => Err(RpcError::new(
            INTERNAL,
            "SubscribeToTask is served as an event stream",
        )),
    }
}

/// Serialize into a JSON value; a type that cannot serialize becomes `null` (never happens
/// for our own serde types, and the alternative would be a panic in a handler).
pub(crate) fn val<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

/// An object from key/value pairs; `serde_json::json!` is avoided because it unwraps.
pub(crate) fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

pub(crate) fn ok(id: &Value, result: Value) -> Value {
    obj([
        ("jsonrpc", "2.0".into()),
        ("id", id.clone()),
        ("result", result),
    ])
}

pub(crate) fn err(id: &Value, e: &RpcError) -> Value {
    obj([
        ("jsonrpc", "2.0".into()),
        ("id", id.clone()),
        ("error", val(e)),
    ])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests: json! literals")]
    use serde_json::json;

    use super::*;

    fn req(method: &str, params: Value) -> Request {
        Request {
            id: json!(1),
            method: method.into(),
            params,
        }
    }

    #[test]
    fn decodes_every_method_and_rejects_the_rest() {
        let send = req(
            "SendMessage",
            json!({"message": {"messageId": "m", "role": "ROLE_USER", "parts": [{"text": "a"}, {"data": {}}, {"text": "b"}], "taskId": "t-1"}}),
        );
        assert_eq!(
            decode(&send),
            Ok(Call::SendMessage {
                task_id: Some("t-1".into()),
                text: "a\nb".into(),
                option: None,
                return_immediately: false
            })
        );
        let queued = req(
            "SendMessage",
            json!({"message": {"messageId": "m", "role": "ROLE_USER", "parts": [{"text": "plan"}]}, "configuration": {"returnImmediately": true}}),
        );
        assert!(matches!(
            decode(&queued),
            Ok(Call::SendMessage {
                return_immediately: true,
                task_id: None,
                ..
            })
        ));
        let with_option = req(
            "SendMessage",
            json!({"message": {"messageId": "m", "role": "ROLE_USER", "parts": [{"data": {"option": "retry_with_guidance"}}, {"text": "use sh"}], "taskId": "inc-1"}}),
        );
        assert_eq!(
            decode(&with_option),
            Ok(Call::SendMessage {
                task_id: Some("inc-1".into()),
                text: "use sh".into(),
                option: Some(AttentionOption::RetryWithGuidance),
                return_immediately: false
            })
        );
        let bad_option = req(
            "SendMessage",
            json!({"message": {"messageId": "m", "role": "ROLE_USER", "parts": [{"data": {"option": "explode"}}], "taskId": "inc-1"}}),
        );
        assert_eq!(decode(&bad_option).unwrap_err().code, INVALID_PARAMS);
        assert_eq!(
            decode(&req("GetTask", json!({"id": "x"}))),
            Ok(Call::GetTask { id: "x".into() })
        );
        assert_eq!(
            decode(&req("CancelTask", json!({"id": "x"}))),
            Ok(Call::CancelTask { id: "x".into() })
        );
        assert_eq!(
            decode(&req("SubscribeToTask", json!({"id": "x"}))),
            Ok(Call::Subscribe { id: "x".into() })
        );
        assert_eq!(
            decode(&req("ListTasks", Value::Null)),
            Ok(Call::ListTasks {
                input_required_only: false
            })
        );
        assert_eq!(
            decode(&req(
                "ListTasks",
                json!({"status": "TASK_STATE_INPUT_REQUIRED"})
            )),
            Ok(Call::ListTasks {
                input_required_only: true
            })
        );
        assert_eq!(
            decode(&req("GetTask", json!({}))).unwrap_err().code,
            INVALID_PARAMS
        );
        assert_eq!(
            decode(&req("ListTasks", json!(5))).unwrap_err().code,
            INVALID_PARAMS
        );
        assert_eq!(
            decode(&req("GetExtendedAgentCard", Value::Null))
                .unwrap_err()
                .code,
            UNSUPPORTED
        );
        assert_eq!(
            decode(&req("Nope", Value::Null)).unwrap_err().code,
            METHOD_NOT_FOUND
        );
    }

    #[test]
    fn remote_errors_map_to_codes() {
        let cases = [
            (RemoteError::TaskNotFound { id: "x".into() }, TASK_NOT_FOUND),
            (RemoteError::Terminal { id: "x".into() }, NOT_CANCELABLE),
            (RemoteError::EmptyMessage, INVALID_PARAMS),
            (
                RemoteError::Budget(domain::RigBudgetExceeded::Usd {
                    spent: domain::MicroUsd::new(1),
                    cap: domain::MicroUsd::new(1),
                }),
                BUDGET,
            ),
            (
                RemoteError::Submit(app::SubmitError::Unreachable { detail: "d".into() }),
                INTERNAL,
            ),
            (
                RemoteError::Tail(app::TailError::Io { detail: "d".into() }),
                INTERNAL,
            ),
            (
                RemoteError::Forbidden(domain::Forbidden {
                    client: domain::ClientId::try_new("c").expect("c"),
                    rig: domain::RigName::try_new("r").expect("r"),
                    scope: domain::Scope::Plan,
                }),
                FORBIDDEN,
            ),
        ];
        for (e, code) in cases {
            assert_eq!(RpcError::from(e).code, code);
        }
        let body = err(&json!(7), &RpcError::new(INTERNAL, "x"));
        assert_eq!(body["error"]["code"], INTERNAL);
        assert_eq!(ok(&json!(7), json!(1))["result"], 1);
    }
}
