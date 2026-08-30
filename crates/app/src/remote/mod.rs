//! Remote control (docs/exec-plans/completed/remote-control.md): the ports a control plane
//! needs to operate rigs, the A2A read models over the ledger, and the workflows that
//! authorize, act, and audit. The console crate is a thin HTTP binding of this module.

pub mod a2a;
pub mod a2ui;
pub mod attention;
#[cfg(test)]
mod attention_tests;
pub mod chat;
#[cfg(test)]
mod chat_tests;
#[cfg(test)]
mod remote_fixtures_tests;
#[cfg(test)]
mod remote_tests;
pub mod service;

use std::sync::Arc;

use async_trait::async_trait;
use domain::{BeadId, Principal, RigBudget, RigName};

use crate::ports::{BeadStore, EventSink};

/// A verified token becomes a [`Principal`]; the core never sees tokens.
pub trait Authenticator: Send + Sync {
    /// `None` for an unknown or revoked token.
    fn authenticate(&self, bearer: &str) -> Option<Principal>;
}

/// One record read back from a rig's event log. Kept raw (kind + free-form detail) because
/// the console displays events, it never acts on them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EventRecord {
    /// Unix seconds as written by the rigs; kept as text so old string-stamped logs still read.
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "at_from_number_or_string")
    )]
    pub at: String,
    pub actor: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub bead: Option<BeadId>,
    pub kind: String,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub detail: serde_json::Map<String, serde_json::Value>,
}

/// Rigs write `at` as unix seconds; older logs and fixtures wrote a string. Accept both.
#[cfg(feature = "serde")]
fn at_from_number_or_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Number(i64),
        Text(String),
    }
    Ok(match <Raw as serde::Deserialize>::deserialize(d)? {
        Raw::Number(n) => n.to_string(),
        Raw::Text(t) => t,
    })
}

/// Reading the event log failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TailError {
    #[error("event log unreadable: {detail}")]
    Io { detail: String },
}

/// Cursor-based reader over a rig's append-only event log.
#[async_trait]
pub trait EventTail: Send + Sync {
    /// Records appended after `cursor` (0 = from the beginning) and the new cursor.
    ///
    /// # Errors
    /// `Io` when the log cannot be read.
    async fn read_from(&self, cursor: u64) -> Result<(Vec<EventRecord>, u64), TailError>;
}

/// Plan submission failed. The rig does the planning (it holds the harness credential);
/// the console only relays the text and reads back the epic id.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubmitError {
    #[error("rig refused the plan: {detail}")]
    Rejected { detail: String },
    #[error("rig unreachable: {detail}")]
    Unreachable { detail: String },
}

/// Hands a plan to the rig's own planner.
#[async_trait]
pub trait PlanSubmitter: Send + Sync {
    /// # Errors
    /// `Rejected` when the rig's planner failed; `Unreachable` when it could not be run.
    async fn submit(&self, plan_text: &str) -> Result<BeadId, SubmitError>;
}

/// Why a rig cannot answer right now (no ledger yet, its server down).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("rig unavailable: {reason}")]
pub struct Unavailable {
    pub reason: String,
}

/// A cheap check that a rig can be asked at all — microseconds, no `bd` process — so the
/// console never spawns a store call that is bound to fail.
pub trait Probe: Send + Sync {
    /// # Errors
    /// `Unavailable` with the reason an operator can act on.
    fn available(&self) -> Result<(), Unavailable>;
    /// For tests that need to flip a fake.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Everything the control plane holds for one rig. No credentials, no Dolt handle.
#[derive(Clone)]
pub struct Rig {
    pub name: RigName,
    pub store: Arc<dyn BeadStore>,
    pub sink: Arc<dyn EventSink>,
    pub events: Arc<dyn EventTail>,
    pub planner: Arc<dyn PlanSubmitter>,
    pub budget: RigBudget,
    pub probe: Arc<dyn Probe>,
}

impl core::fmt::Debug for Rig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Rig")
            .field("name", &self.name)
            .field("budget", &self.budget)
            .finish_non_exhaustive()
    }
}

/// The rigs this control plane fronts.
pub trait RigRegistry: Send + Sync {
    fn names(&self) -> Vec<RigName>;
    fn rig(&self, name: &RigName) -> Option<Rig>;
}
