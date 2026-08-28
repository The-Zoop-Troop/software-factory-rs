//! Hand-written fakes for the ports. No mocking framework.

use std::collections::BTreeMap;

use async_trait::async_trait;
use domain::{BeadId, BeadKind, FactoryMeta, Timestamp};
use tokio::sync::Mutex;

use crate::bead::{Bead, BeadStatus, NewBead};
use crate::ports::{BeadStore, Clock, StoreError};

/// In-memory bead store. Ready == every active bead of the kind (no dependency graph).
#[derive(Debug, Default)]
pub struct FakeStore {
    beads: Mutex<BTreeMap<BeadId, Bead>>,
    next: Mutex<u32>,
}

impl FakeStore {
    /// Insert a factory task bead in whatever state `meta` says.
    pub async fn seed_task(&self, id: BeadId, meta: FactoryMeta) {
        self.beads.lock().await.insert(
            id.clone(),
            Bead {
                id,
                title: "task".into(),
                description: String::new(),
                acceptance: None,
                notes: None,
                status: BeadStatus::Open,
                labels: vec![BeadKind::Task.label()],
                parent: None,
                kind: Some(BeadKind::Task),
                meta: Some(meta),
            },
        );
    }

    /// Insert a bead the factory does not own.
    pub async fn seed_plain(&self, id: BeadId, title: &str) {
        self.beads.lock().await.insert(
            id.clone(),
            Bead {
                id,
                title: title.into(),
                description: String::new(),
                acceptance: None,
                notes: None,
                status: BeadStatus::Open,
                labels: vec![],
                parent: None,
                kind: None,
                meta: None,
            },
        );
    }
}

#[async_trait]
impl BeadStore for FakeStore {
    async fn show(&self, id: &BeadId) -> Result<Bead, StoreError> {
        self.beads
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(id.clone()))
    }

    async fn ready(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.list_active(kind).await
    }

    async fn list_active(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        Ok(self
            .beads
            .lock()
            .await
            .values()
            .filter(|b| b.kind == Some(kind) && b.status != BeadStatus::Closed)
            .cloned()
            .collect())
    }

    async fn set_meta(&self, id: &BeadId, meta: &FactoryMeta) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound(id.clone()))?;
        bead.meta = Some(meta.clone());
        Ok(())
    }

    async fn note(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound(id.clone()))?;
        bead.notes = Some(match bead.notes.take() {
            Some(existing) => format!("{existing}\n{text}"),
            None => text.to_owned(),
        });
        Ok(())
    }

    async fn create(&self, new: NewBead) -> Result<BeadId, StoreError> {
        let mut next = self.next.lock().await;
        *next += 1;
        let id = BeadId::try_new(format!("fake-{}", *next))
            .map_err(|e| StoreError::Rejected(e.to_string()))?;
        self.beads.lock().await.insert(
            id.clone(),
            Bead {
                id: id.clone(),
                title: new.title,
                description: new.description,
                acceptance: new.acceptance,
                notes: None,
                status: BeadStatus::Open,
                labels: vec![new.kind.label()],
                parent: new.parent,
                kind: Some(new.kind),
                meta: new.meta,
            },
        );
        Ok(id)
    }

    async fn close(&self, id: &BeadId, _reason: &str) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound(id.clone()))?;
        bead.status = BeadStatus::Closed;
        Ok(())
    }
}

/// A clock that returns whatever it was set to.
#[derive(Debug)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}
