//! `BeadStore` over the `bd` CLI. Every call is `bd … --json`; output is decoded once
//! into `RawBead` and parsed into `app::Bead`.

use std::path::PathBuf;
use std::process::Stdio;

use app::domain::meta::{MERGE_META_KEY, META_KEY, VERIFY_META_KEY};
use app::domain::{BeadId, BeadKind, BeadMeta, FactoryMeta, MergeMeta, VerifyMeta};
use app::{Bead, BeadStatus, BeadStore, NewBead, StoreError, StoreOp, Unavailable};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::process::Command;

/// Adapter configuration.
#[derive(Debug, Clone)]
pub struct BdCli {
    bin: PathBuf,
    workdir: PathBuf,
    actor: Option<String>,
}

impl BdCli {
    #[must_use]
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            bin: PathBuf::from("bd"),
            workdir: workdir.into(),
            actor: None,
        }
    }

    #[must_use]
    pub fn with_bin(self, bin: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            ..self
        }
    }

    /// Name recorded in the beads audit trail (`--actor`).
    #[must_use]
    pub fn with_actor(self, actor: impl Into<String>) -> Self {
        Self {
            actor: Some(actor.into()),
            ..self
        }
    }

    async fn run(&self, op: StoreOp, args: &[&str]) -> Result<Vec<u8>, StoreError> {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(&self.workdir)
            .args(args)
            .stdin(Stdio::null());
        if let Some(actor) = &self.actor {
            cmd.arg("--actor").arg(actor);
        }
        tracing::debug!(args = ?args, "bd");
        let out = cmd.output().await.map_err(|e| StoreError::Unavailable {
            op,
            cause: crate::classify_io(e.kind()),
            detail: e.to_string(),
        })?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(parse_bd_stderr(op, &stderr))
        }
    }

    async fn run_json<T: for<'de> Deserialize<'de>>(
        &self,
        op: StoreOp,
        args: &[&str],
    ) -> Result<T, StoreError> {
        let bytes = self.run(op, args).await?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::Decode {
            op,
            field: "json",
            detail: e.to_string(),
        })
    }
}

/// Parse `bd`'s stderr once into a typed error. This is the only place its wording is read.
fn parse_bd_stderr(op: StoreOp, stderr: &str) -> StoreError {
    let lower = stderr.to_ascii_lowercase(); // fp-allow: boundary parse of CLI stderr, done once here
    if let Some(blocked) = parse_blocked(stderr) {
        return blocked;
    }
    // fp-allow: boundary parse of CLI stderr, done once here
    if lower.contains("no issue found") || lower.contains("not found") {
        // fp-allow: boundary parse of CLI stderr
        // fp-allow: boundary parse of CLI stderr
        return StoreError::Rejected {
            op,
            detail: stderr.to_owned(),
        };
    }
    // fp-allow: boundary parse of CLI stderr, done once here
    if lower.contains("database") || lower.contains("dolt") || lower.contains("lock") {
        // fp-allow: boundary parse of CLI stderr
        // fp-allow: boundary parse of CLI stderr
        let cause = if lower.contains("lock") {
            Unavailable::Locked
        } else {
            Unavailable::Database
        };
        return StoreError::Unavailable {
            op,
            cause,
            detail: stderr.to_owned(),
        };
    }
    StoreError::Rejected {
        op,
        detail: stderr.to_owned(),
    }
}

/// `cannot close X: blocked by open issues [a b] (use --force to override)`
fn parse_blocked(stderr: &str) -> Option<StoreError> {
    let rest = stderr.split("cannot close ").nth(1)?;
    let (id, rest) = rest.split_once(':')?;
    let list = rest
        .split("blocked by open issues [")
        .nth(1)?
        .split(']')
        .next()?;
    let by = list
        .split_whitespace()
        .filter_map(|s| BeadId::try_new(s).ok())
        .collect::<Vec<_>>();
    let id = BeadId::try_new(id.trim()).ok()?;
    Some(StoreError::Blocked { id, by })
}

/// Wire shape of `bd show/ready/list --json`. Fields absent when empty.
#[derive(Debug, Deserialize)]
struct RawBead {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    status: String,
    #[serde(default)]
    labels: Option<Vec<String>>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

impl TryFrom<RawBead> for Bead {
    type Error = StoreError;

    fn try_from(raw: RawBead) -> Result<Self, Self::Error> {
        let decode = |what: &'static str, e: &dyn std::fmt::Display| StoreError::Decode {
            op: StoreOp::Show,
            field: what,
            detail: e.to_string(),
        };
        let id = BeadId::try_new(raw.id).map_err(|e| decode("id", &e))?;
        let status: BeadStatus = raw.status.parse().map_err(|e| decode("status", &e))?;
        let labels = raw.labels.unwrap_or_default();
        let kind = BeadKind::from_labels(labels.iter().map(String::as_str));
        let parent = raw
            .parent
            .filter(|p| !p.is_empty())
            .map(|p| BeadId::try_new(p).map_err(|e| decode("parent", &e)))
            .transpose()?;
        let mut metadata = raw.metadata.unwrap_or_default();
        let meta = metadata
            .remove(META_KEY)
            .map(|v| serde_json::from_value::<FactoryMeta>(v).map_err(|e| decode(META_KEY, &e)))
            .transpose()?;
        let verify = metadata
            .remove(VERIFY_META_KEY)
            .map(|v| {
                serde_json::from_value::<VerifyMeta>(v).map_err(|e| decode(VERIFY_META_KEY, &e))
            })
            .transpose()?;
        let merge = metadata
            .remove(MERGE_META_KEY)
            .map(|v| serde_json::from_value::<MergeMeta>(v).map_err(|e| decode(MERGE_META_KEY, &e)))
            .transpose()?;
        let cross_needs = metadata
            .remove(domain::NEEDS_META_KEY)
            .map(|v| {
                serde_json::from_value::<Vec<domain::CrossRigNeed>>(v)
                    .map_err(|e| decode(domain::NEEDS_META_KEY, &e))
            })
            .transpose()?;
        Ok(Self {
            id,
            title: raw.title,
            description: raw.description,
            acceptance: raw.acceptance_criteria.filter(|s| !s.is_empty()),
            notes: raw.notes.filter(|s| !s.is_empty()),
            status,
            labels,
            parent,
            kind,
            meta,
            verify,
            merge,
            cross_needs,
        })
    }
}

#[derive(Debug, Deserialize)]
struct Created {
    id: String,
}

fn meta_json(meta: &FactoryMeta) -> Result<String, StoreError> {
    wrap_json(META_KEY, meta)
}

fn bead_meta_json(meta: &BeadMeta) -> Result<String, StoreError> {
    match meta {
        BeadMeta::Task(m) => wrap_json(meta.key(), m),
        BeadMeta::Verify(m) => wrap_json(meta.key(), m),
        BeadMeta::Merge(m) => wrap_json(meta.key(), m),
        BeadMeta::Needs(n) => wrap_json(meta.key(), n),
    }
}

fn wrap_json<T: serde::Serialize>(key: &str, value: &T) -> Result<String, StoreError> {
    let value = serde_json::to_value(value).map_err(|e| StoreError::Decode {
        op: StoreOp::Update,
        field: "metadata",
        detail: e.to_string(),
    })?;
    let wrapped = serde_json::Value::Object(std::iter::once((key.to_owned(), value)).collect());
    serde_json::to_string(&wrapped).map_err(|e| StoreError::Decode {
        op: StoreOp::Update,
        field: "metadata",
        detail: e.to_string(),
    })
}

#[async_trait]
impl BeadStore for BdCli {
    async fn show(&self, id: &BeadId) -> Result<Bead, StoreError> {
        let raws: Vec<RawBead> = match self
            .run_json(StoreOp::Show, &["show", id.as_ref(), "--json"])
            .await
        {
            // fp-allow: boundary parse of CLI stderr, done once
            Err(StoreError::Rejected { detail, .. })
                if detail.to_ascii_lowercase().contains("no issue found") // fp-allow: boundary parse of CLI stderr
                    || detail.to_ascii_lowercase().contains("not found") =>
            // fp-allow: boundary parse of CLI stderr
            {
                return Err(StoreError::NotFound { id: id.clone() });
            }
            other => other?,
        };
        raws.into_iter()
            .next()
            .ok_or_else(|| StoreError::NotFound { id: id.clone() })?
            .try_into()
    }

    async fn ready(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        let label = kind.label();
        let raws: Vec<RawBead> = self
            .run_json(
                StoreOp::Ready,
                &["ready", "--label", &label, "--limit", "0", "--json"],
            )
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }

    async fn list_active(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        let label = kind.label();
        let raws: Vec<RawBead> = self
            .run_json(
                StoreOp::List,
                &[
                    "list",
                    "--label",
                    &label,
                    "--status",
                    "open,in_progress,blocked,hooked",
                    "--limit",
                    "0",
                    "--json",
                ],
            )
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }

    async fn list_closed(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        let label = kind.label();
        let raws: Vec<RawBead> = self
            .run_json(
                StoreOp::List,
                &[
                    "list", "--label", &label, "--status", "closed", "--limit", "0", "--json",
                ],
            )
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }

    async fn set_meta(&self, id: &BeadId, meta: &FactoryMeta) -> Result<(), StoreError> {
        let json = meta_json(meta)?;
        self.run(
            StoreOp::Update,
            &["update", id.as_ref(), "--metadata", &json, "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn set_verify(&self, id: &BeadId, meta: &VerifyMeta) -> Result<(), StoreError> {
        let json = wrap_json(VERIFY_META_KEY, meta)?;
        self.run(
            StoreOp::Update,
            &["update", id.as_ref(), "--metadata", &json, "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn add_needs(&self, dependent: &BeadId, blocker: &BeadId) -> Result<(), StoreError> {
        self.run(
            StoreOp::Dep,
            &["dep", "add", dependent.as_ref(), blocker.as_ref()],
        )
        .await
        .map(|_| ())
    }

    async fn note(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        self.run(
            StoreOp::Note,
            &["update", id.as_ref(), "--append-notes", text, "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn label(&self, id: &BeadId, label: &str) -> Result<(), StoreError> {
        self.run(
            StoreOp::Note,
            &["label", "add", id.as_ref(), label, "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn create(&self, new: NewBead) -> Result<BeadId, StoreError> {
        let priority = new.priority.get().to_string();
        let label = new.kind.label();
        let mut args: Vec<String> = vec![
            "create".into(),
            new.title.to_string(),
            "--type".into(),
            bd_type(new.kind).into(),
            "--priority".into(),
            priority,
            "--labels".into(),
            label,
            "--description".into(),
            new.description,
            "--json".into(),
        ];
        if let Some(parent) = &new.parent {
            // Never inherit the epic's `fac:kind=epic` label; kind must be unambiguous.
            args.extend([
                "--parent".into(),
                parent.to_string(),
                "--no-inherit-labels".into(),
            ]);
        }
        if let Some(acceptance) = new.acceptance {
            args.extend(["--acceptance".into(), acceptance]);
        }
        if let Some(meta) = &new.meta {
            args.extend(["--metadata".into(), bead_meta_json(meta)?]);
        }
        // A bead is claimable the instant it exists, but its `needs` edges can only be added
        // afterwards. Hide it from `bd ready` until the edges are in place, or a polling worker
        // can grab a task whose blockers aren't closed.
        let hide_until_edges = !new.needs.is_empty();
        if hide_until_edges || new.deferred {
            args.extend(["--defer".into(), "+1d".into()]);
        }
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let created: Created = self.run_json(StoreOp::Create, &borrowed).await?;
        let id = BeadId::try_new(created.id).map_err(|e| StoreError::Decode {
            op: StoreOp::Create,
            field: "id",
            detail: e.to_string(),
        })?;
        // `--deps blocks:X` means "this bead blocks X" — the opposite of `needs` — so edges are
        // added explicitly with `dep add <dependent> <blocker>`.
        for blocker in &new.needs {
            self.add_needs(&id, blocker).await?;
        }
        if hide_until_edges && !new.deferred {
            self.undefer(&id).await?;
        }
        Ok(id)
    }

    async fn try_claim(&self, id: &BeadId) -> Result<bool, StoreError> {
        match self
            .run(
                StoreOp::Update,
                &["update", id.as_ref(), "--claim", "--json"],
            )
            .await
        {
            Ok(_) => Ok(true),
            // The ledger refuses a claim on a bead someone already took; that is the race
            // working, not a failure.
            Err(StoreError::Rejected { detail, .. })
                if detail.contains("claim")
                    || detail.contains("assign")
                    || detail.contains("in_progress") =>
            {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    async fn unclaim(&self, id: &BeadId) -> Result<(), StoreError> {
        self.run(
            StoreOp::Update,
            &[
                "update",
                id.as_ref(),
                "--status",
                "open",
                "--assignee",
                "",
                "--json",
            ],
        )
        .await
        .map(|_| ())
    }

    async fn list_deferred(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        let label = kind.label();
        let raws: Vec<RawBead> = self
            .run_json(
                StoreOp::List,
                &[
                    "list", "--label", &label, "--status", "deferred", "--limit", "0", "--json",
                ],
            )
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }

    async fn undefer(&self, id: &BeadId) -> Result<(), StoreError> {
        self.run(
            StoreOp::Update,
            &["update", id.as_ref(), "--defer", "", "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn set_needs(
        &self,
        id: &BeadId,
        needs: &[domain::CrossRigNeed],
    ) -> Result<(), StoreError> {
        let json = wrap_json(domain::NEEDS_META_KEY, &needs)?;
        self.run(
            StoreOp::Update,
            &["update", id.as_ref(), "--metadata", &json, "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn set_description(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        self.run(
            StoreOp::Update,
            &["update", id.as_ref(), "--description", text, "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn close(&self, id: &BeadId, reason: &str) -> Result<(), StoreError> {
        self.run(
            StoreOp::Close,
            &["close", id.as_ref(), "--reason", reason, "--json"],
        )
        .await
        .map(|_| ())
    }

    async fn children(&self, id: &BeadId) -> Result<Vec<Bead>, StoreError> {
        let raws: Vec<RawBead> = self
            .run_json(
                StoreOp::List,
                &[
                    "list",
                    "--parent",
                    id.as_ref(),
                    "--all",
                    "--limit",
                    "0",
                    "--json",
                ],
            )
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }
}

/// Native beads issue type for each factory kind.
const fn bd_type(kind: BeadKind) -> &'static str {
    match kind {
        BeadKind::Epic => "epic",
        BeadKind::Task
        | BeadKind::Verify
        | BeadKind::Merge
        | BeadKind::Reference
        | BeadKind::Contract
        | BeadKind::PlanRequest => "task",
        BeadKind::Question => "decision",
        BeadKind::Incident => "bug",
    }
}

#[cfg(test)]
#[path = "bd_tests.rs"]
mod tests;
