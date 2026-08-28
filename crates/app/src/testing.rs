//! Hand-written fakes for the ports. No mocking framework.
#![allow(
    clippy::disallowed_types,
    clippy::missing_panics_doc,
    clippy::unused_async,
    reason = "test support: a leaf Mutex over a Vec is the simplest correct sink"
)]

use std::collections::BTreeMap;

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use domain::{
    BeadId, BeadKind, BeadMeta, BranchName, Duration, FactoryMeta, Sha, Timestamp, VerifyMeta,
};
use tokio::sync::Mutex;

use crate::bead::{Bead, BeadStatus, NewBead};
use crate::events::FactoryEvent;
use crate::ports::{
    BeadStore, Clock, EventSink, Repo, RepoError, RunError, RunOutput, Runner, StoreError, Worktree,
};

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
                verify: None,
                merge: None,
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
            commands: commands.iter().map(|c| (*c).to_owned()).collect(),
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
                meta: match &new.meta {
                    Some(BeadMeta::Task(m)) => Some(m.clone()),
                    Some(BeadMeta::Verify(_) | BeadMeta::Merge(_)) | None => None,
                },
                verify: match &new.meta {
                    Some(BeadMeta::Verify(m)) => Some(m.clone()),
                    Some(BeadMeta::Task(_) | BeadMeta::Merge(_)) | None => None,
                },
                merge: match &new.meta {
                    Some(BeadMeta::Merge(m)) => Some(m.clone()),
                    Some(BeadMeta::Task(_) | BeadMeta::Verify(_)) | None => None,
                },
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

    async fn children(&self, id: &BeadId) -> Result<Vec<Bead>, StoreError> {
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

/// A clock that returns whatever it was set to.
#[derive(Debug)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// Records worktree adds/removes; never touches disk.
#[derive(Debug, Default)]
pub struct FakeRepo {
    pub added: std::sync::Mutex<Vec<Worktree>>,
    pub removed: std::sync::Mutex<Vec<Worktree>>,
    /// Heads that `worktree_add` should reject as unknown.
    pub missing: Vec<Sha>,
    /// Heads whose rebase should conflict.
    pub conflicting: Vec<Sha>,
    /// What `rebase` returns as the new head for a given old head (identity if absent).
    pub rebased_to: BTreeMap<Sha, Sha>,
    pub fast_forwards: std::sync::Mutex<Vec<(BranchName, Sha)>>,
    pub pushes: std::sync::Mutex<Vec<(String, BranchName)>>,
    /// Make every push fail with `Unavailable`.
    pub push_fails: bool,
}

#[async_trait]
impl Repo for FakeRepo {
    async fn worktree_add(&self, branch: &BranchName, head: &Sha) -> Result<Worktree, RepoError> {
        if self.missing.contains(head) {
            return Err(RepoError::RefNotFound(head.to_string()));
        }
        let wt = Worktree {
            path: PathBuf::from(format!("/fake/wt/{branch}")),
            branch: branch.clone(),
            head: head.clone(),
        };
        self.added.lock().expect("test mutex").push(wt.clone());
        Ok(wt)
    }

    async fn worktree_remove(&self, worktree: Worktree) -> Result<(), RepoError> {
        self.removed.lock().expect("test mutex").push(worktree);
        Ok(())
    }

    async fn rebase(&self, worktree: &Worktree, _onto: &BranchName) -> Result<Sha, RepoError> {
        if self.conflicting.contains(&worktree.head) {
            return Err(RepoError::Conflict(format!(
                "CONFLICT in {}",
                worktree.branch
            )));
        }
        Ok(self
            .rebased_to
            .get(&worktree.head)
            .cloned()
            .unwrap_or_else(|| worktree.head.clone()))
    }

    async fn fast_forward(&self, branch: &BranchName, to: &Sha) -> Result<(), RepoError> {
        self.fast_forwards
            .lock()
            .expect("test mutex")
            .push((branch.clone(), to.clone()));
        Ok(())
    }

    async fn push(&self, remote: &str, branch: &BranchName) -> Result<(), RepoError> {
        if self.push_fails {
            return Err(RepoError::Unavailable("remote down".into()));
        }
        self.pushes
            .lock()
            .expect("test mutex")
            .push((remote.to_owned(), branch.clone()));
        Ok(())
    }
}

/// Scripted command outcomes: exact command string → output. Unknown commands fail to spawn.
#[derive(Debug, Default)]
pub struct FakeRunner {
    pub script: BTreeMap<String, RunOutput>,
    pub calls: std::sync::Mutex<Vec<(PathBuf, String)>>,
}

impl FakeRunner {
    #[must_use]
    pub fn ok(stdout: &str) -> RunOutput {
        RunOutput {
            exit_code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
            timed_out: false,
        }
    }

    #[must_use]
    pub fn fail(code: i32, stderr: &str) -> RunOutput {
        RunOutput {
            exit_code: Some(code),
            stdout: String::new(),
            stderr: stderr.into(),
            timed_out: false,
        }
    }
}

#[async_trait]
impl Runner for FakeRunner {
    async fn run(
        &self,
        cwd: &Path,
        command: &str,
        _timeout: Duration,
    ) -> Result<RunOutput, RunError> {
        self.calls
            .lock()
            .expect("test mutex")
            .push((cwd.to_path_buf(), command.to_owned()));
        self.script.get(command).cloned().ok_or_else(|| RunError {
            command: command.to_owned(),
            reason: "unscripted".into(),
        })
    }
}
