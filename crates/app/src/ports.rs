//! Ports: the effects workflows are allowed to perform, as traits.

use async_trait::async_trait;
use domain::{BeadId, BeadKind, FactoryMeta, Timestamp};

use crate::bead::{Bead, NewBead};
use crate::events::FactoryEvent;

/// Failures crossing the bead-store boundary, already translated from the adapter.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("bead {0} not found")]
    NotFound(BeadId),
    #[error("bead store rejected the request: {0}")]
    Rejected(String),
    #[error("could not decode bead store output: {0}")]
    Decode(String),
    #[error("bead store unavailable: {0}")]
    Unavailable(String),
}

/// The beads ledger. Implemented by the `bd` CLI adapter in `infra` and by an in-memory fake.
#[async_trait]
pub trait BeadStore: Send + Sync {
    /// # Errors
    /// `NotFound` if `id` does not exist; other variants for transport/decode failures.
    async fn show(&self, id: &BeadId) -> Result<Bead, StoreError>;

    /// Claimable beads of `kind` (dependency-aware, i.e. `bd ready`).
    ///
    /// # Errors
    /// Transport/decode failures.
    async fn ready(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError>;

    /// All non-closed beads of `kind`, regardless of readiness.
    ///
    /// # Errors
    /// Transport/decode failures.
    async fn list_active(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError>;

    /// Replace the factory metadata on a bead.
    ///
    /// # Errors
    /// `NotFound` or transport failures.
    async fn set_meta(&self, id: &BeadId, meta: &FactoryMeta) -> Result<(), StoreError>;

    /// Append a note.
    ///
    /// # Errors
    /// `NotFound` or transport failures.
    async fn note(&self, id: &BeadId, text: &str) -> Result<(), StoreError>;

    /// # Errors
    /// `Rejected` if the store refuses the bead; transport failures.
    async fn create(&self, bead: NewBead) -> Result<BeadId, StoreError>;

    /// # Errors
    /// `NotFound` or transport failures.
    async fn close(&self, id: &BeadId, reason: &str) -> Result<(), StoreError>;

    /// Direct children of `id` (any status).
    ///
    /// # Errors
    /// Transport/decode failures.
    async fn children(&self, id: &BeadId) -> Result<Vec<Bead>, StoreError>;
}

/// Where factory events go. Recording must never fail the caller; sinks log their own trouble.
pub trait EventSink: Send + Sync {
    fn record(&self, event: &FactoryEvent);
}

/// Wall-clock source. The only place `now` comes from.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}
