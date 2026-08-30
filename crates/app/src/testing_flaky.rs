//! A store that fails after N writes — for atomicity tests (see `testing.rs`).
#![allow(
    clippy::disallowed_types,
    clippy::missing_panics_doc,
    reason = "test support"
)]

use async_trait::async_trait;
use domain::{BeadId, BeadKind, FactoryMeta, VerifyMeta};

use super::FakeStore;
use crate::bead::{Bead, NewBead};
use crate::ports::{BeadStore, StoreError};

/// A store whose write operations start failing after `writes_allowed` successes. For
/// atomicity tests: what does the ledger look like when the shell dies mid-effect?
#[derive(Debug)]
pub struct FlakyStore {
    pub inner: FakeStore,
    writes_allowed: std::sync::atomic::AtomicUsize,
}

impl FlakyStore {
    #[must_use]
    pub fn new(inner: FakeStore, writes_allowed: usize) -> Self {
        Self {
            inner,
            writes_allowed: std::sync::atomic::AtomicUsize::new(writes_allowed),
        }
    }

    fn write(&self, op: crate::ports::StoreOp) -> Result<(), StoreError> {
        let left = self
            .writes_allowed
            .load(std::sync::atomic::Ordering::SeqCst);
        if left == 0 {
            return Err(StoreError::Unavailable {
                op,
                cause: crate::ports::Unavailable::Database,
                detail: "flaky".into(),
            });
        }
        self.writes_allowed
            .store(left - 1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl BeadStore for FlakyStore {
    async fn show(&self, id: &BeadId) -> Result<Bead, StoreError> {
        self.inner.show(id).await
    }
    async fn ready(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.inner.ready(kind).await
    }
    async fn list_active(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.inner.list_active(kind).await
    }
    async fn list_closed(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.inner.list_closed(kind).await
    }
    async fn set_meta(&self, id: &BeadId, meta: &FactoryMeta) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Update)?;
        self.inner.set_meta(id, meta).await
    }
    async fn set_verify(&self, id: &BeadId, meta: &VerifyMeta) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Update)?;
        self.inner.set_verify(id, meta).await
    }
    async fn add_needs(&self, dependent: &BeadId, blocker: &BeadId) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Dep)?;
        self.inner.add_needs(dependent, blocker).await
    }
    async fn note(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Note)?;
        self.inner.note(id, text).await
    }
    async fn list_deferred(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.inner.list_deferred(kind).await
    }
    async fn undefer(&self, id: &BeadId) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Update)?;
        self.inner.undefer(id).await
    }
    async fn set_description(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Update)?;
        self.inner.set_description(id, text).await
    }
    async fn label(&self, id: &BeadId, label: &str) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Note)?;
        self.inner.label(id, label).await
    }
    async fn create(&self, new: NewBead) -> Result<BeadId, StoreError> {
        self.write(crate::ports::StoreOp::Create)?;
        self.inner.create(new).await
    }
    async fn close(&self, id: &BeadId, reason: &str) -> Result<(), StoreError> {
        self.write(crate::ports::StoreOp::Close)?;
        self.inner.close(id, reason).await
    }
    async fn children(&self, id: &BeadId) -> Result<Vec<Bead>, StoreError> {
        self.inner.children(id).await
    }
}
