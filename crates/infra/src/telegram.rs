//! Telegram Bot API, long polling only (no inbound port). Just the two calls a bot needs.

use app::ClientError;
use serde_json::{Map, Value};

fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<Map<_, _>>(),
    )
}

/// One incoming text message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub update_id: i64,
    pub chat_id: i64,
    pub text: String,
}

#[derive(Clone)]
pub struct TelegramApi {
    base: String,
    client: reqwest::Client,
}

impl core::fmt::Debug for TelegramApi {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TelegramApi").finish_non_exhaustive()
    }
}

#[derive(Debug, serde::Deserialize)]
struct Envelope<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct Update {
    update_id: i64,
    #[serde(default)]
    message: Option<Msg>,
}

#[derive(Debug, serde::Deserialize)]
struct Msg {
    chat: Chat,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct Chat {
    id: i64,
}

impl TelegramApi {
    /// `api_base` is `https://api.telegram.org` in production; tests point it at a local server.
    ///
    /// # Errors
    /// `Transport` if the HTTP client cannot be built.
    pub fn new(api_base: &str, bot_token: &str) -> Result<Self, ClientError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| ClientError::Transport {
                detail: e.to_string(),
            })?;
        Ok(Self {
            base: format!("{}/bot{bot_token}", api_base.trim_end_matches('/')),
            client,
        })
    }

    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        body: &serde_json::Value,
    ) -> Result<T, ClientError> {
        let resp = self
            .client
            .post(format!("{}/{method}", self.base))
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Transport {
                detail: e.to_string(),
            })?;
        let status = resp.status().as_u16();
        let env: Envelope<T> = resp.json().await.map_err(|e| ClientError::Decode {
            detail: format!("{method}: {e}"),
        })?;
        match (env.ok, env.result) {
            (true, Some(r)) => Ok(r),
            (ok, _) => Err(ClientError::Refused {
                status,
                code: None,
                message: env
                    .description
                    .unwrap_or_else(|| format!("{method}: ok={ok} without result")),
            }),
        }
    }

    /// Text messages after `offset`, waiting up to `timeout_secs` for one.
    ///
    /// # Errors
    /// `Refused` (bad token), `Transport`, or `Decode`.
    pub async fn updates(
        &self,
        offset: i64,
        timeout_secs: u64,
    ) -> Result<Vec<Incoming>, ClientError> {
        let updates: Vec<Update> = self
            .post(
                "getUpdates",
                &obj([
                    ("offset", offset.into()),
                    ("timeout", timeout_secs.into()),
                    ("allowed_updates", Value::Array(vec!["message".into()])),
                ]),
            )
            .await?;
        Ok(updates
            .into_iter()
            .filter_map(|u| {
                let m = u.message?;
                Some(Incoming {
                    update_id: u.update_id,
                    chat_id: m.chat.id,
                    text: m.text?,
                })
            })
            .collect())
    }

    /// # Errors
    /// `Refused`, `Transport`, or `Decode`.
    pub async fn send(&self, chat_id: i64, text: &str) -> Result<(), ClientError> {
        let _: serde_json::Value = self
            .post(
                "sendMessage",
                &obj([("chat_id", chat_id.into()), ("text", text.into())]),
            )
            .await?;
        Ok(())
    }
}
