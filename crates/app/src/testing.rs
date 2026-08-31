//! Hand-written fakes for the ports. No mocking framework.
#![allow(
    clippy::disallowed_types,
    clippy::missing_panics_doc,
    clippy::unused_async,
    reason = "test support: a leaf Mutex over a Vec is the simplest correct sink"
)]

use std::collections::BTreeMap;

use async_trait::async_trait;

use domain::{
    BeadId, BeadKind, BeadMeta, BranchName, Duration, FactoryMeta, NonEmpty, Sha, Timestamp,
    VerifyCommand, VerifyMeta,
};
use tokio::sync::Mutex;

use crate::bead::{Bead, BeadStatus, NewBead};
use crate::events::FactoryEvent;
use crate::ports::{BeadStore, Clock, EventSink, StoreError};

/// In-memory bead store. Ready == every active bead of the kind (no dependency graph).
#[derive(Debug, Default)]
pub struct FakeStore {
    beads: Mutex<BTreeMap<BeadId, Bead>>,
    next: Mutex<u32>,
    /// `blocks` edges recorded at create time: dependent → blockers.
    pub needs: Mutex<BTreeMap<BeadId, Vec<BeadId>>>,
    /// When set, every read fails as `Unavailable` (an unreachable ledger).
    pub fail_reads: std::sync::atomic::AtomicBool,
}

impl FakeStore {
    fn readable(&self) -> Result<(), StoreError> {
        if self.fail_reads.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable {
                op: crate::ports::StoreOp::Note,
                cause: crate::Unavailable::Io,
                detail: "fake: reads disabled".to_owned(),
            });
        }
        Ok(())
    }
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
                verify: None,
                merge: None,
                cross_needs: None,
            },
        );
    }

    /// Insert an epic with children; each `(id, closed)` pair becomes a task child.
    pub async fn seed_epic(&self, id: BeadId, children: &[(&str, bool)]) {
        let mut beads = self.beads.lock().await;
        beads.insert(
            id.clone(),
            plain(
                id.clone(),
                "epic",
                Some(BeadKind::Epic),
                None,
                BeadStatus::Open,
            ),
        );
        for (cid, closed) in children {
            let cid = BeadId::try_new(*cid).expect("test ids are valid");
            let status = if *closed {
                BeadStatus::Closed
            } else {
                BeadStatus::Open
            };
            beads.insert(
                cid.clone(),
                plain(cid, "child", None, Some(id.clone()), status),
            );
        }
    }

    /// Insert a verify bead paired with `task`.
    pub async fn seed_verify(&self, id: BeadId, task: BeadId, commands: &[&str]) {
        let mut bead = plain(id, "verify", Some(BeadKind::Verify), None, BeadStatus::Open);
        bead.verify = Some(VerifyMeta {
            task,
            commands: NonEmpty::try_from(
                commands
                    .iter()
                    .map(|c| VerifyCommand::try_new(*c).expect("test command"))
                    .collect::<Vec<_>>(),
            )
            .expect("test commands non-empty"),
            timeout: Duration::from_minutes(1),
        });
        self.beads.lock().await.insert(bead.id.clone(), bead);
    }

    /// Insert a merge bead for `task`.
    pub async fn seed_merge(&self, id: BeadId, task: BeadId, branch: &str, head: Sha) {
        let mut bead = plain(id, "merge", Some(BeadKind::Merge), None, BeadStatus::Open);
        bead.merge = Some(domain::MergeMeta {
            task,
            branch: BranchName::try_new(branch).expect("test branch is valid"),
            head,
        });
        self.beads.lock().await.insert(bead.id.clone(), bead);
    }

    /// Re-parent an existing bead (tests only).
    pub async fn set_parent(&self, id: &BeadId, parent: &BeadId) {
        if let Some(b) = self.beads.lock().await.get_mut(id) {
            b.parent = Some(parent.clone());
        }
    }

    /// Insert a reference bead under `parent`.
    pub async fn seed_reference(&self, id: BeadId, parent: BeadId, text: &str) {
        let mut bead = plain(
            id,
            "reference",
            Some(BeadKind::Reference),
            Some(parent),
            BeadStatus::Open,
        );
        bead.description = text.to_owned();
        self.beads.lock().await.insert(bead.id.clone(), bead);
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
                verify: None,
                merge: None,
                cross_needs: None,
            },
        );
    }
}

#[async_trait]
impl BeadStore for FakeStore {
    async fn show(&self, id: &BeadId) -> Result<Bead, StoreError> {
        self.readable()?;
        self.beads
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })
    }

    async fn ready(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.readable()?;
        self.list_active(kind).await
    }

    async fn list_active(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.readable()?;
        Ok(self
            .beads
            .lock()
            .await
            .values()
            .filter(|b| {
                b.kind == Some(kind)
                    && !matches!(b.status, BeadStatus::Closed | BeadStatus::Deferred)
            })
            .cloned()
            .collect())
    }

    async fn try_claim(&self, id: &BeadId) -> Result<bool, StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        if bead.status == BeadStatus::InProgress {
            return Ok(false);
        }
        bead.status = BeadStatus::InProgress;
        Ok(true)
    }

    async fn unclaim(&self, id: &BeadId) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        bead.status = BeadStatus::Open;
        Ok(())
    }

    async fn list_deferred(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.readable()?;
        Ok(self
            .beads
            .lock()
            .await
            .values()
            .filter(|b| b.kind == Some(kind) && b.status == BeadStatus::Deferred)
            .cloned()
            .collect())
    }

    async fn undefer(&self, id: &BeadId) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        bead.status = BeadStatus::Open;
        Ok(())
    }

    async fn set_needs(
        &self,
        id: &BeadId,
        needs: &[domain::CrossRigNeed],
    ) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        bead.cross_needs = Some(needs.to_vec());
        Ok(())
    }

    async fn set_description(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        text.clone_into(&mut bead.description);
        Ok(())
    }

    async fn list_closed(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        self.readable()?;
        Ok(self
            .beads
            .lock()
            .await
            .values()
            .filter(|b| b.kind == Some(kind) && b.status == BeadStatus::Closed)
            .cloned()
            .collect())
    }

    async fn set_meta(&self, id: &BeadId, meta: &FactoryMeta) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        bead.meta = Some(meta.clone());
        Ok(())
    }

    async fn set_verify(&self, id: &BeadId, meta: &VerifyMeta) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        bead.verify = Some(meta.clone());
        Ok(())
    }

    async fn add_needs(&self, dependent: &BeadId, blocker: &BeadId) -> Result<(), StoreError> {
        if !self.beads.lock().await.contains_key(dependent) {
            return Err(StoreError::NotFound {
                id: dependent.clone(),
            });
        }
        self.needs
            .lock()
            .await
            .entry(dependent.clone())
            .or_default()
            .push(blocker.clone());
        Ok(())
    }

    async fn note(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        bead.notes = Some(match bead.notes.take() {
            Some(existing) => format!("{existing}\n{text}"),
            None => text.to_owned(),
        });
        Ok(())
    }

    async fn label(&self, id: &BeadId, label: &str) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        if !bead.labels.iter().any(|l| l == label) {
            bead.labels.push(label.to_owned());
        }
        Ok(())
    }

    async fn create(&self, new: NewBead) -> Result<BeadId, StoreError> {
        let mut next = self.next.lock().await;
        *next += 1;
        let id = BeadId::try_new(format!("fake-{}", *next)).map_err(|e| StoreError::Rejected {
            op: crate::ports::StoreOp::Create,
            detail: e.to_string(),
        })?;
        self.needs
            .lock()
            .await
            .insert(id.clone(), new.needs.clone());
        self.beads.lock().await.insert(
            id.clone(),
            Bead {
                id: id.clone(),
                title: new.title.to_string(),
                description: new.description,
                acceptance: new.acceptance,
                notes: None,
                status: if new.deferred {
                    BeadStatus::Deferred
                } else {
                    BeadStatus::Open
                },
                labels: vec![new.kind.label()],
                parent: new.parent,
                kind: Some(new.kind),
                meta: match &new.meta {
                    Some(BeadMeta::Task(m)) => Some(m.clone()),
                    Some(BeadMeta::Verify(_) | BeadMeta::Merge(_) | BeadMeta::Needs(_)) | None => {
                        None
                    }
                },
                verify: match &new.meta {
                    Some(BeadMeta::Verify(m)) => Some(m.clone()),
                    Some(BeadMeta::Task(_) | BeadMeta::Merge(_) | BeadMeta::Needs(_)) | None => {
                        None
                    }
                },
                merge: match &new.meta {
                    Some(BeadMeta::Merge(m)) => Some(m.clone()),
                    Some(BeadMeta::Task(_) | BeadMeta::Verify(_) | BeadMeta::Needs(_)) | None => {
                        None
                    }
                },
                cross_needs: match &new.meta {
                    Some(BeadMeta::Needs(n)) => Some(n.clone()),
                    Some(BeadMeta::Task(_) | BeadMeta::Verify(_) | BeadMeta::Merge(_)) | None => {
                        None
                    }
                },
            },
        );
        Ok(id)
    }

    async fn close(&self, id: &BeadId, _reason: &str) -> Result<(), StoreError> {
        let mut beads = self.beads.lock().await;
        let bead = beads
            .get_mut(id)
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?;
        bead.status = BeadStatus::Closed;
        Ok(())
    }

    async fn children(&self, id: &BeadId) -> Result<Vec<Bead>, StoreError> {
        self.readable()?;
        Ok(self
            .beads
            .lock()
            .await
            .values()
            .filter(|b| b.parent.as_ref() == Some(id))
            .cloned()
            .collect())
    }
}

/// A bare bead of `kind` for tests that need one outside the seeders.
#[must_use]
pub fn plain_bead(id: BeadId, kind: Option<BeadKind>) -> Bead {
    plain(id, "bead", kind, None, BeadStatus::Open)
}

fn plain(
    id: BeadId,
    title: &str,
    kind: Option<BeadKind>,
    parent: Option<BeadId>,
    status: BeadStatus,
) -> Bead {
    Bead {
        id,
        title: title.into(),
        description: String::new(),
        acceptance: None,
        notes: None,
        status,
        labels: kind.map(BeadKind::label).into_iter().collect(),
        parent,
        kind,
        meta: None,
        verify: None,
        merge: None,
        cross_needs: None,
    }
}

/// Collects events in memory.
#[derive(Debug, Default)]
pub struct MemorySink(std::sync::Mutex<Vec<FactoryEvent>>);

impl MemorySink {
    pub async fn events(&self) -> Vec<FactoryEvent> {
        self.0.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl EventSink for MemorySink {
    fn record(&self, event: &FactoryEvent) {
        if let Ok(mut g) = self.0.lock() {
            g.push(event.clone());
        }
    }
}

/// A clock that returns whatever it was set to; `sleep` just yields.
#[derive(Debug)]
pub struct FixedClock(pub Timestamp);

#[async_trait]
impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }

    async fn sleep(&self, _d: Duration) {
        tokio::task::yield_now().await;
    }
}

#[path = "testing_repo.rs"]
mod repo;
pub use repo::{FakeRepo, FakeRunner};

#[path = "testing_harness.rs"]
mod harness;
pub use harness::FakeHarness;

#[path = "testing_flaky.rs"]
mod flaky;
pub use flaky::FlakyStore;

/// Remote-control fakes live in a sibling file to keep this one under the size cap.
#[path = "testing_remote.rs"]
pub mod remote;

/// Host docker fake lives in a sibling file (size cap).
#[path = "testing_host.rs"]
pub mod host;
pub use host::FakeHostDocker;
