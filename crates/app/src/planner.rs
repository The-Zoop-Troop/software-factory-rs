//! The Planner v0 (ARCHITECTURE.md §4.1): one structured-output harness call turns a plan
//! in prose into an epic DAG in beads. No survey turn yet (Phase 2).

use std::collections::BTreeMap;
use std::path::Path;

use domain::plan::TaskKey;
use domain::{
    Attempts, BeadId, BeadKind, BeadMeta, BranchName, FactoryMeta, Plan, PlanDefaults, PlanError,
    Priority, RawPlan, Sha, TaskState, Title, Tokens, Usage, VerifyMeta,
};

use crate::bead::NewBead;
use crate::ports::{
    BeadStore, Harness, HarnessError, HarnessRequest, Repo, RepoError, StoreError, ToolPolicy,
};

/// What planning produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanReport {
    pub epic: BeadId,
    pub tasks: Vec<(TaskKey, BeadId)>,
    pub tokens: domain::Tokens,
}

/// Planning failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlannerError {
    #[error(transparent)]
    Harness(#[from] HarnessError),
    #[error("model reported an error: {message}")]
    ModelError { message: String },
    #[error("model returned no structured output")]
    NoStructuredOutput,
    #[error("model output did not match the plan schema: {detail}")]
    Shape { detail: String },
    #[error(transparent)]
    Invalid(#[from] PlanError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("repo: {0}")]
    Repo(#[from] RepoError),
}

/// Turn `plan_text` into an epic with task + verify beads. Returns the ids created.
///
/// # Errors
/// Any stage: harness, schema, validation, ledger.
#[tracing::instrument(skip_all, err)]
pub async fn plan(
    store: &dyn BeadStore,
    harness: &dyn Harness,
    repo: &dyn Repo,
    repo_path: &Path,
    main: &BranchName,
    plan_text: &str,
    defaults: PlanDefaults,
) -> Result<PlanReport, PlannerError> {
    let outcome = harness
        .run(HarnessRequest {
            cwd: repo_path.to_path_buf(),
            system_prompt: SYSTEM_PROMPT.to_owned(),
            prompt: plan_text.to_owned(),
            schema: Some(plan_schema()),
            tools: ToolPolicy::None,
            mcp: crate::mcp::McpConfig::default(),
            max_turns: domain::Turns::new(4),
            timeout: domain::Duration::from_minutes(10),
            effort: defaults.effort,
        })
        .await?;
    if outcome.is_error {
        return Err(PlannerError::ModelError {
            message: outcome.text,
        });
    }
    // Harnesses without native structured output (or that return it as text) fall back to
    // the first JSON object in the reply.
    let value = match outcome.structured {
        Some(v) => v,
        None => extract_json_object(&outcome.text).ok_or(PlannerError::NoStructuredOutput)?,
    };
    let raw: RawPlan = serde_json::from_value(value).map_err(|e| PlannerError::Shape {
        detail: e.to_string(),
    })?;
    let plan = raw.validate(defaults)?;
    let base = repo.head_of(main).await?;
    let mut report = materialize(store, &plan, &base).await?;
    report.tokens = outcome.tokens;
    Ok(report)
}

/// Write a validated plan to the ledger: epic, optional reference bead, then tasks in
/// topological order so every `needs` edge points at an already-created bead.
async fn materialize(
    store: &dyn BeadStore,
    plan: &Plan,
    base: &Sha,
) -> Result<PlanReport, PlannerError> {
    let epic = store
        .create(NewBead {
            title: plan.summary.clone(),
            description: plan
                .tasks
                .iter()
                .map(|t| format!("- {}: {}", t.key, t.title))
                .collect::<Vec<_>>()
                .join("\n"),
            kind: BeadKind::Epic,
            priority: Priority::HIGH,
            parent: None,
            needs: vec![],
            acceptance: None,
            meta: None,
        })
        .await?;
    if let Some(reference) = &plan.reference {
        // Reference beads are context, not work: created closed so they never show in `bd ready`
        // and never hold the epic open. Workers read them via `children` regardless of status.
        let reference_id = store
            .create(NewBead {
                title: Title::derived(&format!("reference: {}", plan.summary)),
                description: reference.clone(),
                kind: BeadKind::Reference,
                priority: Priority::LOW,
                parent: Some(epic.clone()),
                needs: vec![],
                acceptance: None,
                meta: None,
            })
            .await?;
        store
            .close(&reference_id, "reference material, not work")
            .await?;
    }

    let mut ids: BTreeMap<TaskKey, BeadId> = BTreeMap::new();
    for t in &plan.tasks {
        let needs = t
            .needs
            .iter()
            .filter_map(|k| ids.get(k).cloned())
            .collect::<Vec<_>>();
        // Verify bead first so the task can point at it; it NEEDS the task so `bd ready` hides it.
        let verify_placeholder = store
            .create(NewBead {
                title: Title::derived(&format!("verify: {}", t.title)),
                description: t
                    .acceptance
                    .iter()
                    .map(|a| format!("- {a}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                kind: BeadKind::Verify,
                priority: Priority::MEDIUM,
                parent: Some(epic.clone()),
                needs: vec![],
                acceptance: None,
                meta: None,
            })
            .await?;
        let task = store
            .create(NewBead {
                title: t.title.clone(),
                description: t.description.clone(),
                kind: BeadKind::Task,
                priority: Priority::HIGH,
                parent: Some(epic.clone()),
                needs,
                acceptance: Some(t.acceptance.join("\n")),
                meta: Some(BeadMeta::Task(FactoryMeta {
                    verify_bead: verify_placeholder.clone(),
                    base: base.clone(),
                    budget: t.budget,
                    usage: Usage::default(),
                    lease_expiries: Attempts::new(0),
                    state: TaskState::Open,
                })),
            })
            .await?;
        store
            .set_verify(
                &verify_placeholder,
                &VerifyMeta {
                    task: task.clone(),
                    commands: t.verify.clone(),
                    timeout: t.verify_timeout,
                },
            )
            .await?;
        store.add_needs(&verify_placeholder, &task).await?;
        ids.insert(t.key.clone(), task);
    }
    let tasks = ids.into_iter().collect();
    Ok(PlanReport {
        epic,
        tasks,
        tokens: Tokens::new(0),
    })
}

/// The first balanced `{ … }` in `text` that parses as JSON, ignoring code fences.
fn extract_json_object(text: &str) -> Option<serde_json::Value> {
    let mut start = None;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escape = false;
    for (i, b) in text.bytes().enumerate() {
        if in_str {
            match (escape, b) {
                (true, _) => escape = false,
                (false, b'\\') => escape = true,
                (false, b'"') => in_str = false,
                (false, _) => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(s) = start
                    && let Some(slice) = text.get(s..=i)
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(slice)
                {
                    return Some(v);
                }
            }
            _ => {} // fp-allow: matching raw bytes of untrusted text, not a domain enum
        }
    }
    None
}

const SYSTEM_PROMPT: &str = "\
You are the Planner of an autonomous software factory. You receive a high-level plan for a \
software project and decompose it into implementation tasks that stateless coding agents will \
execute one at a time, each in a fresh git worktree with no memory of other tasks.

Rules:
- Each task must be completable by one agent in one session (roughly under an hour of focused work).
- Each task MUST include executable `verify` shell commands that prove the task is done when run \
from the repository root of the task's branch (e.g. `cargo test -p auth`, `npm test -- login`, \
`test -f docs/api.md`). A task with no verifiable outcome is not a task.
- Verify commands are executed one per line by POSIX `/bin/sh` (dash), not bash, from the repo \
root: source files with an explicit path (`. ./lib.sh` — never `source`, never `. lib.sh`, which \
searches PATH), no `[[ ]]`, no arrays, no `set -o pipefail`. Each must exit 0 on success.
- `acceptance` lists the human-readable criteria the verify commands check.
- Use `needs` to express hard ordering only (B cannot start until A is merged). Independent tasks \
must not be chained; they will run in parallel.
- Keys are short slugs (e.g. `db-schema`, `login-endpoint`). Reference only keys you define.
- `reference` holds architecture notes and decisions every task's agent should read: module \
layout, conventions, technology choices. Be concrete.
- Do not invent tasks outside the plan's scope. Do not include setup tasks the repo already covers.";

fn plan_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string", "description": "One-line name for the epic" },
            "reference": { "type": "string", "description": "Architecture notes for all tasks" },
            "tasks": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string" },
                        "title": { "type": "string" },
                        "description": { "type": "string" },
                        "acceptance": { "type": "array", "items": { "type": "string" } },
                        "verify": { "type": "array", "minItems": 1, "items": { "type": "string" } },
                        "needs": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["key", "title", "description", "acceptance", "verify", "needs"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["summary", "reference", "tasks"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeHarness, FakeRepo, FakeStore};

    fn main() -> BranchName {
        BranchName::try_new("main").unwrap()
    }

    #[tokio::test]
    async fn materializes_epic_tasks_verify_beads_and_edges() {
        let harness = FakeHarness::structured(serde_json::json!({
            "summary": "Add login",
            "reference": "Use argon2. Handlers live in src/http/.",
            "tasks": [
                {"key": "endpoint", "title": "Login endpoint", "description": "POST /login", "acceptance": ["401 on bad password"], "verify": ["cargo test -p http login"], "needs": ["schema"]},
                {"key": "schema", "title": "User table", "description": "migration", "acceptance": ["migration applies"], "verify": ["cargo test -p db"], "needs": []}
            ]
        }));
        let store = FakeStore::default();
        let report = plan(
            &store,
            &harness,
            &FakeRepo::default(),
            Path::new("/repo"),
            &main(),
            "add login",
            PlanDefaults::default(),
        )
        .await
        .unwrap();
        assert_eq!(report.tasks.len(), 2);
        let tasks = store.list_active(BeadKind::Task).await.unwrap();
        assert_eq!(tasks.len(), 2);
        let endpoint = tasks.iter().find(|t| t.title == "Login endpoint").unwrap();
        let schema = tasks.iter().find(|t| t.title == "User table").unwrap();
        assert_eq!(
            store.needs.lock().await.get(&endpoint.id).unwrap(),
            &vec![schema.id.clone()]
        );
        assert_eq!(endpoint.parent.as_ref(), Some(&report.epic));
        let verifies = store.list_active(BeadKind::Verify).await.unwrap();
        assert_eq!(verifies.len(), 2);
        let v = verifies
            .iter()
            .find(|v| v.verify.as_ref().unwrap().task == endpoint.id)
            .unwrap();
        assert_eq!(
            Vec::from(v.verify.as_ref().unwrap().commands.clone()),
            vec![domain::VerifyCommand::try_new("cargo test -p http login").unwrap()]
        );
        assert_eq!(endpoint.meta.as_ref().unwrap().verify_bead, v.id);
        assert_eq!(
            store.needs.lock().await.get(&v.id).unwrap(),
            &vec![endpoint.id.clone()],
            "verify bead needs its task"
        );
        assert!(
            store
                .list_active(BeadKind::Reference)
                .await
                .unwrap()
                .is_empty(),
            "reference is created closed"
        );
        let refs = store.children(&report.epic).await.unwrap();
        assert_eq!(
            refs.iter()
                .filter(|c| c.kind == Some(BeadKind::Reference))
                .count(),
            1
        );
        let req = &harness.requests.lock().unwrap()[0];
        assert_eq!(req.tools, ToolPolicy::None);
        assert!(req.schema.is_some());
    }

    #[test]
    fn extracts_json_from_prose_and_fences() {
        let t = "Here is the plan:\n```json\n{\"summary\":\"s\",\"tasks\":[{\"key\":\"a\"}]}\n```\nDone.";
        let v = extract_json_object(t).unwrap();
        assert_eq!(v["summary"], "s");
        assert_eq!(extract_json_object("no json here"), None);
        assert_eq!(extract_json_object("{\"a\":\"}\"}").unwrap()["a"], "}");
    }

    #[tokio::test]
    async fn text_only_plan_is_accepted() {
        let mut harness = FakeHarness::structured(serde_json::json!({}));
        if let Some(o) = harness.outcome.as_mut() {
            o.structured = None;
            o.text = "{\"summary\":\"x\",\"reference\":\"\",\"tasks\":[{\"key\":\"a\",\"title\":\"A\",\"description\":\"\",\"acceptance\":[],\"verify\":[\"true\"],\"needs\":[]}]}".into();
        }
        let store = FakeStore::default();
        let report = plan(
            &store,
            &harness,
            &FakeRepo::default(),
            Path::new("/repo"),
            &main(),
            "x",
            PlanDefaults::default(),
        )
        .await
        .unwrap();
        assert_eq!(report.tasks.len(), 1);
    }

    #[tokio::test]
    async fn invalid_plan_writes_nothing() {
        let harness = FakeHarness::structured(serde_json::json!({
            "summary": "x", "reference": "",
            "tasks": [{"key": "a", "title": "A", "description": "", "acceptance": [], "verify": ["true"], "needs": ["a"]}]
        }));
        let store = FakeStore::default();
        let err = plan(
            &store,
            &harness,
            &FakeRepo::default(),
            Path::new("/repo"),
            &main(),
            "x",
            PlanDefaults::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(
            err,
            PlannerError::Invalid(PlanError::SelfNeed { .. })
        ));
        assert!(store.list_active(BeadKind::Epic).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn model_error_is_surfaced() {
        let mut harness = FakeHarness::structured(serde_json::json!({}));
        if let Some(o) = harness.outcome.as_mut() {
            o.is_error = true;
            o.text = "rate limited".into();
        }
        let store = FakeStore::default();
        let err = plan(
            &store,
            &harness,
            &FakeRepo::default(),
            Path::new("/repo"),
            &main(),
            "x",
            PlanDefaults::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, PlannerError::ModelError { message } if message == "rate limited"));
    }
}
