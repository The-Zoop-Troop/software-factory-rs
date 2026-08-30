//! HTTP client for the console's A2A JSON-RPC binding (`docs/generated/console-api.md`).

use app::remote::a2a::{Message, Part, Task};
use app::{A2aApi, ClientError};
use async_trait::async_trait;
use serde_json::{Map, Value};

fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<Map<_, _>>(),
    )
}

fn val<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

/// One rig's endpoint (`https://host/rigs/<name>`) plus the bearer token.
#[derive(Clone)]
pub struct A2aHttp {
    endpoint: String,
    token: String,
    client: reqwest::Client,
}

impl core::fmt::Debug for A2aHttp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("A2aHttp")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

/// JSON-RPC error body.
#[derive(Debug, serde::Deserialize)]
struct RpcErr {
    code: i32,
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct RpcReply {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcErr>,
}

impl A2aHttp {
    /// `rig_url` is the rig's base (`…/rigs/<name>`); a trailing `/a2a` or `/` is tolerated.
    ///
    /// # Errors
    /// `Transport` if the HTTP client cannot be built.
    pub fn new(rig_url: &str, token: &str) -> Result<Self, ClientError> {
        let base = rig_url.trim_end_matches('/').trim_end_matches("/a2a");
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(900))
            .build()
            .map_err(|e| ClientError::Transport {
                detail: e.to_string(),
            })?;
        Ok(Self {
            endpoint: format!("{base}/a2a"),
            token: token.to_owned(),
            client,
        })
    }

    async fn fetch_card(&self) -> Result<Value, ClientError> {
        let url = format!(
            "{}/.well-known/agent-card.json",
            self.endpoint.trim_end_matches("/a2a")
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.map_err(|e| decode(&e))?;
        if status >= 400 {
            return Err(ClientError::Refused {
                status,
                code: None,
                message: body.to_string(),
            });
        }
        Ok(body)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let body = obj([
            ("jsonrpc", "2.0".into()),
            ("id", 1.into()),
            ("method", method.into()),
            ("params", params),
        ]);
        let resp = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| transport(&e))?;
        let status = resp.status().as_u16();
        let text = resp.text().await.map_err(|e| transport(&e))?;
        let reply: RpcReply = serde_json::from_str(&text).map_err(|e| ClientError::Decode {
            detail: format!(
                "{e} (status {status}): {}",
                text.chars().take(200).collect::<String>()
            ),
        })?;
        match (reply.error, reply.result) {
            (Some(e), _) => Err(ClientError::Refused {
                status,
                code: Some(e.code),
                message: e.message,
            }),
            (None, _) if status >= 400 => Err(ClientError::Refused {
                status,
                code: None,
                message: text.chars().take(200).collect(),
            }),
            (None, Some(r)) => Ok(r),
            (None, None) => Err(ClientError::Decode {
                detail: "reply had neither result nor error".to_owned(),
            }),
        }
    }
}

fn transport(e: &reqwest::Error) -> ClientError {
    ClientError::Transport {
        detail: e.to_string(),
    }
}

fn decode(e: &reqwest::Error) -> ClientError {
    ClientError::Decode {
        detail: e.to_string(),
    }
}

fn task_from(v: Value) -> Result<Task, ClientError> {
    serde_json::from_value(v).map_err(|e| ClientError::Decode {
        detail: e.to_string(),
    })
}

#[async_trait]
impl A2aApi for A2aHttp {
    async fn card(&self) -> Result<Value, ClientError> {
        self.fetch_card().await
    }

    async fn metrics(&self, epic: Option<&str>) -> Result<Value, ClientError> {
        let base = self.endpoint.trim_end_matches("/a2a");
        let mut req = self
            .client
            .get(format!("{base}/metrics"))
            .bearer_auth(&self.token);
        if let Some(e) = epic {
            req = req.query(&[("epic", e)]);
        }
        let resp = req.send().await.map_err(|e| transport(&e))?;
        if !resp.status().is_success() {
            return Err(ClientError::Refused {
                status: resp.status().as_u16(),
                code: None,
                message: resp.text().await.unwrap_or_default(),
            });
        }
        resp.json::<Value>().await.map_err(|e| decode(&e))
    }

    async fn list_tasks(&self) -> Result<Vec<Task>, ClientError> {
        let r = self.call("ListTasks", Value::Null).await?;
        serde_json::from_value(r.get("tasks").cloned().unwrap_or(Value::Array(Vec::new()))).map_err(
            |e| ClientError::Decode {
                detail: e.to_string(),
            },
        )
    }

    async fn get_task(&self, id: &str) -> Result<Task, ClientError> {
        task_from(self.call("GetTask", obj([("id", id.into())])).await?)
    }

    async fn send(&self, text: &str, task_id: Option<&str>) -> Result<Task, ClientError> {
        let message = Message {
            message_id: format!("m-{}", text.len()),
            role: "ROLE_USER".to_owned(),
            parts: vec![Part::Text(text.to_owned())],
            task_id: task_id.map(str::to_owned),
            context_id: None,
        };
        let r = self
            .call("SendMessage", obj([("message", val(&message))]))
            .await?;
        task_from(r.get("task").cloned().unwrap_or(r))
    }

    async fn cancel(&self, id: &str) -> Result<Task, ClientError> {
        task_from(self.call("CancelTask", obj([("id", id.into())])).await?)
    }
}
