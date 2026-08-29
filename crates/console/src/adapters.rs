//! Infrastructure behind the remote-control ports: a file-tailing event reader, a planner
//! that shells out to the rig, and a registry assembled from the config.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use app::{EventRecord, EventTail, PlanSubmitter, Rig, RigRegistry, SubmitError, TailError};
use async_trait::async_trait;
use domain::{BeadId, RigName};
use infra::{BdCli, JsonlSink};

use crate::config::RigSpec;

/// Reads `events.jsonl` from a byte offset; the cursor is the offset after the last
/// complete line, so a partially written line is picked up on the next read.
#[derive(Debug)]
pub(crate) struct FileTail {
    path: PathBuf,
}

impl FileTail {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// Split complete lines after `cursor`, decoding each; undecodable lines are skipped.
pub(crate) fn parse_from(bytes: &[u8], cursor: u64) -> (Vec<EventRecord>, u64) {
    let start = usize::try_from(cursor)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let rest = bytes.get(start..).unwrap_or_default();
    let complete = rest.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let records = rest
        .get(..complete)
        .unwrap_or_default()
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_slice::<EventRecord>(l).ok())
        .collect();
    let next = u64::try_from(start + complete).unwrap_or(u64::MAX);
    (records, next)
}

#[async_trait]
impl EventTail for FileTail {
    async fn read_from(&self, cursor: u64) -> Result<(Vec<EventRecord>, u64), TailError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => Ok(parse_from(&bytes, cursor)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Vec::new(), cursor)),
            Err(e) => Err(TailError::Io {
                detail: format!("{}: {e}", self.path.display()),
            }),
        }
    }
}

/// Runs the rig's plan command with `--text <plan>` and reads `epic <id>` from stdout.
#[derive(Debug)]
pub(crate) struct CommandPlanner {
    argv: Vec<String>,
}

impl CommandPlanner {
    pub(crate) fn new(argv: Vec<String>) -> Self {
        Self { argv }
    }
}

/// The epic id from the planner's report line (`epic <id>  (...)`).
pub(crate) fn parse_epic(stdout: &str) -> Option<BeadId> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("epic "))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|id| BeadId::try_new(id).ok())
}

#[async_trait]
impl PlanSubmitter for CommandPlanner {
    async fn submit(&self, plan_text: &str) -> Result<BeadId, SubmitError> {
        let Some((bin, args)) = self.argv.split_first() else {
            return Err(SubmitError::Unreachable {
                detail: "empty plan command".to_owned(),
            });
        };
        let out = tokio::process::Command::new(bin)
            .args(args)
            .arg("--text")
            .arg(plan_text)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| SubmitError::Unreachable {
                detail: format!("{bin}: {e}"),
            })?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            return Err(SubmitError::Rejected {
                detail: String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .last()
                    .unwrap_or("planner failed")
                    .to_owned(),
            });
        }
        parse_epic(&stdout).ok_or_else(|| SubmitError::Rejected {
            detail: "planner printed no epic id".to_owned(),
        })
    }
}

/// Rigs built once from the config: `bd` in the ledger dir, the event log for audit.
#[derive(Debug, Default)]
pub(crate) struct FileRegistry {
    rigs: BTreeMap<RigName, Rig>,
}

impl FileRegistry {
    /// # Errors
    /// When a rig's event log cannot be opened for append.
    pub(crate) fn build(specs: &[RigSpec]) -> Result<Self, std::io::Error> {
        let rigs = specs
            .iter()
            .map(|s| Ok((s.name.clone(), rig_from(s)?)))
            .collect::<Result<BTreeMap<_, _>, std::io::Error>>()?;
        Ok(Self { rigs })
    }
}

fn rig_from(spec: &RigSpec) -> Result<Rig, std::io::Error> {
    if let Some(dir) = spec.events.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(Rig {
        name: spec.name.clone(),
        store: Arc::new(BdCli::new(&spec.ledger).with_actor("console")),
        sink: Arc::new(JsonlSink::open(&spec.events)?),
        events: Arc::new(FileTail::new(&spec.events)),
        planner: Arc::new(CommandPlanner::new(spec.plan_cmd.clone())),
        budget: spec.budget,
    })
}

impl RigRegistry for FileRegistry {
    fn names(&self) -> Vec<RigName> {
        self.rigs.keys().cloned().collect()
    }
    fn rig(&self, name: &RigName) -> Option<Rig> {
        self.rigs.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_from_handles_partial_lines_and_junk() {
        let a = r#"{"at":"t","actor":"w","bead":"x-1","kind":"claimed","holder":"w"}"#;
        let bytes = format!("{a}\nnot json\n{a}\n{{\"partial\":").into_bytes();
        let (recs, next) = parse_from(&bytes, 0);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].kind, "claimed");
        assert_eq!(recs[0].detail["holder"], "w");
        assert_eq!(
            usize::try_from(next).expect("fits"),
            bytes.len() - "{\"partial\":".len()
        );
        let (again, same) = parse_from(&bytes, next);
        assert!(again.is_empty());
        assert_eq!(same, next);
        assert_eq!(parse_from(&bytes, u64::MAX).0.len(), 0);
    }

    #[tokio::test]
    async fn file_tail_reads_missing_and_present_files() {
        let dir = std::env::temp_dir().join(format!("console-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp");
        let path = dir.join("events.jsonl");
        let tail = FileTail::new(&path);
        assert_eq!(tail.read_from(0).await.expect("ok"), (Vec::new(), 0));
        std::fs::write(
            &path,
            "{\"at\":\"t\",\"actor\":\"a\",\"kind\":\"sweep_done\"}\n",
        )
        .expect("w");
        let (recs, next) = tail.read_from(0).await.expect("ok");
        assert_eq!((recs.len(), next > 0), (1, true));
        assert!(recs[0].bead.is_none());
        let dir_tail = FileTail::new(&dir);
        assert!(matches!(
            dir_tail.read_from(0).await,
            Err(TailError::Io { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn epic_id_is_parsed_from_the_report() {
        assert_eq!(
            parse_epic("epic fac-abc  (3 tasks, 12 tokens)\n  fac-abc.1  a\n")
                .map(|i| i.to_string()),
            Some("fac-abc".to_owned())
        );
        assert!(parse_epic("nothing here").is_none());
        assert!(parse_epic("epic !!bad").is_none());
    }

    #[tokio::test]
    async fn command_planner_reports_each_failure_mode() {
        let ok = CommandPlanner::new(vec!["sh".into(), "-c".into(), "echo epic fac-zz9".into()]);
        assert_eq!(
            ok.submit("p").await.map(|i| i.to_string()),
            Ok("fac-zz9".to_owned())
        );
        let noid = CommandPlanner::new(vec!["sh".into(), "-c".into(), "echo hi".into()]);
        assert!(matches!(
            noid.submit("p").await,
            Err(SubmitError::Rejected { .. })
        ));
        let fails = CommandPlanner::new(vec![
            "sh".into(),
            "-c".into(),
            "echo boom >&2; exit 3".into(),
        ]);
        assert_eq!(
            fails.submit("p").await,
            Err(SubmitError::Rejected {
                detail: "boom".into()
            })
        );
        let missing = CommandPlanner::new(vec!["/nonexistent/planner".into()]);
        assert!(matches!(
            missing.submit("p").await,
            Err(SubmitError::Unreachable { .. })
        ));
        assert!(matches!(
            CommandPlanner::new(vec![]).submit("p").await,
            Err(SubmitError::Unreachable { .. })
        ));
        let passes_text = CommandPlanner::new(vec![
            "sh".into(),
            "-c".into(),
            "test \"$2\" = hello && echo epic fac-ok1".into(),
            "sh".into(),
        ]);
        assert!(passes_text.submit("hello").await.is_ok());
    }

    #[test]
    fn registry_builds_from_specs() {
        let dir = std::env::temp_dir().join(format!("console-reg-{}", std::process::id()));
        let spec = RigSpec {
            name: RigName::try_new("toy").expect("r"),
            ledger: dir.join("ledger"),
            events: dir.join("logs").join("events.jsonl"),
            plan_cmd: vec!["true".into()],
            budget: domain::RigBudget::default(),
        };
        let reg = FileRegistry::build(&[spec]).expect("built");
        assert_eq!(reg.names().len(), 1);
        assert!(reg.rig(&RigName::try_new("toy").expect("r")).is_some());
        assert!(reg.rig(&RigName::try_new("nope").expect("r")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
