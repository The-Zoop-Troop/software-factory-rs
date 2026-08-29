//! Remote control (docs/exec-plans/completed/remote-control.md): the ports a control plane
//! needs to operate rigs, the A2A read models over the ledger, and the workflows that
//! authorize, act, and audit. The console crate is a thin HTTP binding of this module.

pub mod a2a;
pub mod a2ui;
pub mod chat;
#[cfg(test)]
mod chat_tests;
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
    pub at: String,
    pub actor: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub bead: Option<BeadId>,
    pub kind: String,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub detail: serde_json::Map<String, serde_json::Value>,
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

/// Everything the control plane holds for one rig. No credentials, no Dolt handle.
#[derive(Clone)]
pub struct Rig {
    pub name: RigName,
    pub store: Arc<dyn BeadStore>,
    pub sink: Arc<dyn EventSink>,
    pub events: Arc<dyn EventTail>,
    pub planner: Arc<dyn PlanSubmitter>,
    pub budget: RigBudget,
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
