//! Remote-control fakes: a fixed principal per token, a registry over one or more rigs, an
//! in-memory event log, and a planner that returns a preset epic id.
#![allow(
    clippy::disallowed_types,
    reason = "test support: leaf std Mutex, never held across an await"
)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use domain::{BeadId, Principal, RigName};

use crate::remote::a2a::Task;
use crate::remote::chat::{A2aApi, ClientError};
use crate::remote::{
    Authenticator, EventRecord, EventTail, PlanSubmitter, Rig, RigRegistry, SubmitError, TailError,
};
use crate::testing::{FakeStore, MemorySink};

#[derive(Debug, Default)]
pub struct FakeAuth(pub BTreeMap<String, Principal>);

impl Authenticator for FakeAuth {
    fn authenticate(&self, bearer: &str) -> Option<Principal> {
        self.0.get(bearer).cloned()
    }
}

#[derive(Debug, Default)]
pub struct FakeRegistry(pub BTreeMap<RigName, Rig>);

impl RigRegistry for FakeRegistry {
    fn names(&self) -> Vec<RigName> {
        self.0.keys().cloned().collect()
    }
    fn rig(&self, name: &RigName) -> Option<Rig> {
        self.0.get(name).cloned()
    }
}

/// Records with a cursor = index. `fail` makes every read an `Io` error.
#[derive(Debug, Default)]
pub struct FakeTail {
    pub records: Mutex<Vec<EventRecord>>,
    pub fail: bool,
}

impl FakeTail {
    pub fn push(&self, actor: &str, bead: Option<BeadId>, kind: &str) {
        if let Ok(mut r) = self.records.lock() {
            r.push(EventRecord {
                at: "1970-01-01T00:00:00Z".to_owned(),
                actor: actor.to_owned(),
                bead,
                kind: kind.to_owned(),
                detail: serde_json::Map::new(),
            });
        }
    }
}

#[async_trait]
impl EventTail for FakeTail {
    async fn read_from(&self, cursor: u64) -> Result<(Vec<EventRecord>, u64), TailError> {
        if self.fail {
            return Err(TailError::Io {
                detail: "fake".to_owned(),
            });
        }
        let all = self.records.lock().map(|r| r.clone()).unwrap_or_default();
        let start = usize::try_from(cursor).unwrap_or(usize::MAX).min(all.len());
        let next = u64::try_from(all.len()).unwrap_or(u64::MAX);
        Ok((
            all.get(start..)
                .map(<[EventRecord]>::to_vec)
                .unwrap_or_default(),
            next,
        ))
    }
}

/// Returns `epic` for every submission (or the error), remembering the texts.
#[derive(Debug)]
pub struct FakePlanner {
    pub epic: Result<BeadId, SubmitError>,
    pub submitted: Mutex<Vec<String>>,
}

impl FakePlanner {
    #[must_use]
    pub fn returning(epic: &str) -> Self {
        Self {
            epic: Ok(BeadId::try_new(epic).expect("test id")),
            submitted: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl PlanSubmitter for FakePlanner {
    async fn submit(&self, plan_text: &str) -> Result<BeadId, SubmitError> {
        if let Ok(mut s) = self.submitted.lock() {
            s.push(plan_text.to_owned());
        }
        self.epic.clone()
    }
}

/// A rig over fresh fakes; the store and tail are shared so tests can seed and inspect.
#[allow(clippy::type_complexity)]
pub fn rig(
    name: &str,
    planner: FakePlanner,
) -> (Rig, Arc<FakeStore>, Arc<MemorySink>, Arc<FakeTail>) {
    let store = Arc::new(FakeStore::default());
    let sink = Arc::new(MemorySink::default());
    let tail = Arc::new(FakeTail::default());
    let rig = Rig {
        name: RigName::try_new(name).expect("test rig name"),
        store: store.clone(),
        sink: sink.clone(),
        events: tail.clone(),
        planner: Arc::new(planner),
        budget: domain::RigBudget::default(),
    };
    (rig, store, sink, tail)
}

/// A console as a client sees it: canned tasks, a log of sends/cancels, optional failure.
#[derive(Debug, Default)]
pub struct FakeApi {
    pub tasks: Mutex<Vec<Task>>,
    pub sent: Mutex<Vec<(String, Option<String>)>>,
    pub canceled: Mutex<Vec<String>>,
    pub fail: Option<ClientError>,
}

impl FakeApi {
    #[must_use]
    pub fn with_tasks(tasks: Vec<Task>) -> Self {
        Self {
            tasks: Mutex::new(tasks),
            ..Self::default()
        }
    }
    fn check(&self) -> Result<(), ClientError> {
        self.fail.clone().map_or(Ok(()), Err)
    }
    fn find(&self, id: &str) -> Result<Task, ClientError> {
        self.tasks
            .lock()
            .ok()
            .and_then(|t| t.iter().find(|t| t.id == id).cloned())
            .ok_or_else(|| ClientError::Refused {
                status: 404,
                code: Some(-32001),
                message: format!("task `{id}` not found"),
            })
    }
}

#[async_trait]
impl A2aApi for FakeApi {
    async fn card(&self) -> Result<serde_json::Value, ClientError> {
        self.check()?;
        Ok(serde_json::Value::String("factory rig fake".to_owned()))
    }

    async fn list_tasks(&self) -> Result<Vec<Task>, ClientError> {
        self.check()?;
        Ok(self.tasks.lock().map(|t| t.clone()).unwrap_or_default())
    }
    async fn get_task(&self, id: &str) -> Result<Task, ClientError> {
        self.check()?;
        self.find(id)
    }
    async fn send(&self, text: &str, task_id: Option<&str>) -> Result<Task, ClientError> {
        self.check()?;
        if let Ok(mut s) = self.sent.lock() {
            s.push((text.to_owned(), task_id.map(str::to_owned)));
        }
        match task_id {
            Some(id) => self.find(id),
            None => self
                .tasks
                .lock()
                .ok()
                .and_then(|t| t.first().cloned())
                .ok_or(ClientError::Decode {
                    detail: "no tasks seeded".to_owned(),
                }),
        }
    }
    async fn cancel(&self, id: &str) -> Result<Task, ClientError> {
        self.check()?;
        if let Ok(mut c) = self.canceled.lock() {
            c.push(id.to_owned());
        }
        self.find(id)
    }
}
