//! `BeadStore` over the `bd` CLI. Every call is `bd … --json`; output is decoded once
//! into `RawBead` and parsed into `app::Bead`.

use std::path::PathBuf;
use std::process::Stdio;

use app::domain::meta::META_KEY;
use app::domain::{BeadId, BeadKind, FactoryMeta};
use app::{Bead, BeadStatus, BeadStore, NewBead, StoreError};
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

    async fn run(&self, args: &[&str]) -> Result<Vec<u8>, StoreError> {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(&self.workdir)
            .args(args)
            .stdin(Stdio::null());
        if let Some(actor) = &self.actor {
            cmd.arg("--actor").arg(actor);
        }
        tracing::debug!(args = ?args, "bd");
        let out = cmd
            .output()
            .await
            .map_err(|e| StoreError::Unavailable(e.to_string()))?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(classify(&stderr))
        }
    }

    async fn run_json<T: for<'de> Deserialize<'de>>(&self, args: &[&str]) -> Result<T, StoreError> {
        let bytes = self.run(args).await?;
        serde_json::from_slice(&bytes).map_err(|e| StoreError::Decode(e.to_string()))
    }
}

fn classify(stderr: &str) -> StoreError {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("not found") || lower.contains("no issue") {
        // We don't have the id here; callers with an id map this themselves.
        StoreError::Rejected(stderr.to_owned())
    } else if lower.contains("database") || lower.contains("dolt") || lower.contains("lock") {
        StoreError::Unavailable(stderr.to_owned())
    } else {
        StoreError::Rejected(stderr.to_owned())
    }
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
        let decode =
            |what: &str, e: &dyn std::fmt::Display| StoreError::Decode(format!("{what}: {e}"));
        let id = BeadId::try_new(raw.id).map_err(|e| decode("id", &e))?;
        let status: BeadStatus = raw.status.parse().map_err(|e| decode("status", &e))?;
        let labels = raw.labels.unwrap_or_default();
        let kind = BeadKind::from_labels(labels.iter().map(String::as_str));
        let parent = raw
            .parent
            .filter(|p| !p.is_empty())
            .map(|p| BeadId::try_new(p).map_err(|e| decode("parent", &e)))
            .transpose()?;
        let meta = raw
            .metadata
            .and_then(|mut m| m.remove(META_KEY))
            .map(|v| {
                serde_json::from_value::<FactoryMeta>(v).map_err(|e| decode("metadata.fac", &e))
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
        })
    }
}

#[derive(Debug, Deserialize)]
struct Created {
    id: String,
}

fn meta_json(meta: &FactoryMeta) -> Result<String, StoreError> {
    let value = serde_json::to_value(meta).map_err(|e| StoreError::Decode(e.to_string()))?;
    let wrapped =
        serde_json::Value::Object(std::iter::once((META_KEY.to_owned(), value)).collect());
    serde_json::to_string(&wrapped).map_err(|e| StoreError::Decode(e.to_string()))
}

#[async_trait]
impl BeadStore for BdCli {
    async fn show(&self, id: &BeadId) -> Result<Bead, StoreError> {
        let raws: Vec<RawBead> = match self.run_json(&["show", id.as_ref(), "--json"]).await {
            Err(StoreError::Rejected(msg))
                if msg.to_ascii_lowercase().contains("not found")
                    || msg.to_ascii_lowercase().contains("no issue found") =>
            {
                return Err(StoreError::NotFound(id.clone()));
            }
            other => other?,
        };
        raws.into_iter()
            .next()
            .ok_or_else(|| StoreError::NotFound(id.clone()))?
            .try_into()
    }

    async fn ready(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        let label = kind.label();
        let raws: Vec<RawBead> = self
            .run_json(&["ready", "--label", &label, "--limit", "0", "--json"])
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }

    async fn list_active(&self, kind: BeadKind) -> Result<Vec<Bead>, StoreError> {
        let label = kind.label();
        let raws: Vec<RawBead> = self
            .run_json(&[
                "list",
                "--label",
                &label,
                "--status",
                "open,in_progress,blocked,hooked",
                "--limit",
                "0",
                "--json",
            ])
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }

    async fn set_meta(&self, id: &BeadId, meta: &FactoryMeta) -> Result<(), StoreError> {
        let json = meta_json(meta)?;
        self.run(&["update", id.as_ref(), "--metadata", &json, "--json"])
            .await
            .map(|_| ())
    }

    async fn note(&self, id: &BeadId, text: &str) -> Result<(), StoreError> {
        self.run(&["update", id.as_ref(), "--append-notes", text, "--json"])
            .await
            .map(|_| ())
    }

    async fn create(&self, new: NewBead) -> Result<BeadId, StoreError> {
        let priority = new.priority.to_string();
        let label = new.kind.label();
        let mut args: Vec<String> = vec![
            "create".into(),
            new.title,
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
            args.extend(["--parent".into(), parent.to_string()]);
        }
        if !new.needs.is_empty() {
            let deps = new
                .needs
                .iter()
                .map(|d| format!("blocks:{d}"))
                .collect::<Vec<_>>()
                .join(",");
            args.extend(["--deps".into(), deps]);
        }
        if let Some(acceptance) = new.acceptance {
            args.extend(["--acceptance".into(), acceptance]);
        }
        if let Some(meta) = &new.meta {
            args.extend(["--metadata".into(), meta_json(meta)?]);
        }
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let created: Created = self.run_json(&borrowed).await?;
        BeadId::try_new(created.id).map_err(|e| StoreError::Decode(e.to_string()))
    }

    async fn close(&self, id: &BeadId, reason: &str) -> Result<(), StoreError> {
        self.run(&["close", id.as_ref(), "--reason", reason, "--json"])
            .await
            .map(|_| ())
    }

    async fn children(&self, id: &BeadId) -> Result<Vec<Bead>, StoreError> {
        let raws: Vec<RawBead> = self
            .run_json(&[
                "list",
                "--parent",
                id.as_ref(),
                "--all",
                "--limit",
                "0",
                "--json",
            ])
            .await?;
        raws.into_iter().map(Bead::try_from).collect()
    }
}

/// Native beads issue type for each factory kind.
const fn bd_type(kind: BeadKind) -> &'static str {
    match kind {
        BeadKind::Epic => "epic",
        BeadKind::Task | BeadKind::Verify | BeadKind::Merge | BeadKind::Reference => "task",
        BeadKind::Question => "decision",
        BeadKind::Incident => "bug",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_bead_with_factory_meta_decodes() {
        let json = r#"{
          "id":"fac-1","title":"t","status":"open",
          "labels":["fac:kind=task"],
          "metadata":{"fac":{"version":1,"verify_bead":"fac-2",
            "base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","state":{"state":"open"}}}
        }"#;
        let raw: RawBead = serde_json::from_str(json).unwrap();
        let bead = Bead::try_from(raw).unwrap();
        assert_eq!(bead.kind, Some(BeadKind::Task));
        assert!(bead.meta.is_some());
    }

    #[test]
    fn raw_bead_without_meta_is_plain() {
        let raw: RawBead =
            serde_json::from_str(r#"{"id":"fac-1","title":"t","status":"closed"}"#).unwrap();
        let bead = Bead::try_from(raw).unwrap();
        assert_eq!(bead.kind, None);
        assert_eq!(bead.status, BeadStatus::Closed);
    }
}
