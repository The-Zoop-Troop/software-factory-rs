//! A scripted harness for tests: fixed outcome, recorded requests, optional yields.

use std::path::PathBuf;

use async_trait::async_trait;
use domain::{MicroUsd, Tokens, Turns};

use crate::ports::{Harness, HarnessError, HarnessOutcome, HarnessRequest};

/// Returns a canned outcome for every request and records the requests.
#[derive(Debug, Default)]
pub struct FakeHarness {
    pub outcome: Option<HarnessOutcome>,
    pub requests: std::sync::Mutex<Vec<HarnessRequest>>,
    /// How many times the session yields before finishing (lets heartbeats fire in tests).
    pub yields: usize,
    /// Write this as `FACTORY_BLOCKED.md` in the session's cwd (the agent declaring itself blocked).
    pub blocked: Option<String>,
}

impl FakeHarness {
    #[must_use]
    pub fn structured(value: serde_json::Value) -> Self {
        Self {
            outcome: Some(HarnessOutcome {
                text: value.to_string(),
                structured: Some(value),
                tokens: Tokens::new(100),
                cost_micro_usd: MicroUsd::new(1000),
                turns: Turns::new(1),
                is_error: false,
            }),
            requests: std::sync::Mutex::default(),
            yields: 0,
            blocked: None,
        }
    }
}

#[async_trait]
impl Harness for FakeHarness {
    async fn run(&self, req: HarnessRequest) -> Result<HarnessOutcome, HarnessError> {
        if let Some(text) = &self.blocked {
            let _ = std::fs::write(req.cwd.join("FACTORY_BLOCKED.md"), text);
        }
        self.requests.lock().expect("test mutex").push(req);
        for _ in 0..self.yields {
            tokio::task::yield_now().await;
        }
        self.outcome.clone().ok_or_else(|| HarnessError::Spawn {
            bin: PathBuf::from("fake"),
            cause: crate::ports::Unavailable::NotInstalled,
            detail: "unscripted".into(),
        })
    }
}
