//! Infrastructure behind the remote-control ports: a file-tailing event reader, a planner
//! that shells out to the rig, and a registry assembled from the config.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use app::{
    BeadStore, EventRecord, EventTail, PlanSubmitter, Rig, RigRegistry, SubmitError, TailError,
};
use async_trait::async_trait;
use domain::{BeadId, RigName};
use infra::{BdCli, JsonlSink};

use crate::config::{PlannerSpec, RigSpec};

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

/// Leaves a `plan_request` bead and waits for the rig's Planner to close it.
pub(crate) struct QueuedPlanner {
    store: Arc<dyn BeadStore>,
    clock: Arc<dyn app::Clock>,
    timeout: domain::Duration,
    poll: domain::Duration,
}

impl core::fmt::Debug for QueuedPlanner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("QueuedPlanner")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl QueuedPlanner {
    pub(crate) fn new(
        store: Arc<dyn BeadStore>,
        clock: Arc<dyn app::Clock>,
        timeout: domain::Duration,
        poll: domain::Duration,
    ) -> Self {
        Self {
            store,
            clock,
            timeout,
            poll,
        }
    }
}

#[async_trait]
impl PlanSubmitter for QueuedPlanner {
    async fn submit(&self, plan_text: &str) -> Result<BeadId, SubmitError> {
        let unreachable = |e: app::StoreError| SubmitError::Unreachable {
            detail: e.to_string(),
        };
        let request = self
            .store
            .create(app::plan_request(plan_text, "console"))
            .await
            .map_err(unreachable)?;
        let deadline = self.clock.now() + self.timeout;
        loop {
            let bead = self.store.show(&request).await.map_err(unreachable)?;
            if let Some(outcome) = app::plan_outcome(&bead) {
                return outcome;
            }
            if self.clock.now() >= deadline {
                return Err(SubmitError::Unreachable {
                    detail: format!(
                        "planner did not answer request {request} within {}s; is the rig's planner service running?",
                        self.timeout.seconds()
                    ),
                });
            }
            self.clock.sleep(self.poll).await;
        }
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

/// Is the rig's ledger there, and (server mode) is its Dolt server answering? Reads two small
/// files and, in server mode, opens one TCP connection with a short timeout.
#[derive(Debug, Clone)]
pub(crate) struct LedgerProbe {
    ledger: PathBuf,
}

impl app::remote::Probe for LedgerProbe {
    fn available(&self) -> Result<(), app::remote::Unavailable> {
        let beads = self.ledger.join(".beads");
        let meta_path = beads.join("metadata.json");
        let meta = std::fs::read_to_string(&meta_path).map_err(|_| app::remote::Unavailable {
            reason: "no ledger yet: the rig has never run".to_owned(),
        })?;
        let meta: serde_json::Value =
            serde_json::from_str(&meta).unwrap_or(serde_json::Value::Null);
        if meta.get("dolt_mode").and_then(|v| v.as_str()) != Some("server") {
            return Ok(());
        }
        let host = meta
            .get("dolt_server_host")
            .and_then(|v| v.as_str())
            .unwrap_or("127.0.0.1")
            .to_owned();
        let port = std::fs::read_to_string(beads.join("dolt-server.port"))
            .ok()
            .and_then(|p| p.trim().parse::<u16>().ok())
            .unwrap_or(3307);
        let addr = (host.as_str(), port)
            .to_socket_addrs_first()
            .ok_or_else(|| app::remote::Unavailable {
                reason: format!("ledger server {host}:{port} does not resolve: the rig is stopped"),
            })?;
        std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(400))
            .map(drop)
            .map_err(|e| app::remote::Unavailable {
                reason: format!(
                    "ledger server {host}:{port} unreachable ({e}): the rig is stopped"
                ),
            })
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

trait FirstAddr {
    fn to_socket_addrs_first(&self) -> Option<std::net::SocketAddr>;
}
impl FirstAddr for (&str, u16) {
    fn to_socket_addrs_first(&self) -> Option<std::net::SocketAddr> {
        use std::net::ToSocketAddrs as _;
        self.to_socket_addrs().ok()?.next()
    }
}

fn rig_from(spec: &RigSpec) -> Result<Rig, std::io::Error> {
    if let Some(dir) = spec.events.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let store: Arc<dyn BeadStore> = Arc::new(BdCli::new(&spec.ledger).with_actor("console"));
    let planner: Arc<dyn PlanSubmitter> = match &spec.planner {
        PlannerSpec::Command { argv } => Arc::new(CommandPlanner::new(argv.clone())),
        PlannerSpec::Queue { timeout } => Arc::new(QueuedPlanner::new(
            store.clone(),
            Arc::new(infra::SystemClock),
            domain::Duration::from_seconds(timeout.as_secs()),
            domain::Duration::from_seconds(2),
        )),
    };
    Ok(Rig {
        name: spec.name.clone(),
        store,
        sink: Arc::new(JsonlSink::open(&spec.events)?),
        events: Arc::new(FileTail::new(&spec.events)),
        planner,
        budget: spec.budget,
        probe: Arc::new(LedgerProbe {
            ledger: spec.ledger.clone(),
        }),
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
        let numeric = "{\"at\":1788093449,\"actor\":\"planner\",\"bead\":\"be-1\",\"kind\":\"progress\",\"files\":3}\n";
        assert_eq!(
            parse_from(numeric.as_bytes(), 0).0.len(),
            1,
            "rigs write `at` as unix seconds"
        );
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

    #[tokio::test]
    async fn queued_planner_returns_the_epic_or_times_out() {
        use app::BeadStore as _;
        let store = Arc::new(app::testing::FakeStore::default());
        let clock = Arc::new(app::testing::FixedClock(
            domain::Timestamp::from_unix_seconds(0),
        ));
        let planner = QueuedPlanner::new(
            store.clone(),
            clock,
            domain::Duration::from_seconds(0),
            domain::Duration::from_seconds(0),
        );
        assert!(format!("{planner:?}").contains("timeout"));
        // Nobody answers: the fixed clock is already past the deadline after one poll.
        let err = planner.submit("plan me").await.expect_err("timeout");
        assert!(matches!(err, SubmitError::Unreachable { .. }));
        let pending = store
            .ready(domain::BeadKind::PlanRequest)
            .await
            .expect("ok");
        assert_eq!(pending.len(), 1);
        // The rig's planner answers before the console looks.
        store.note(&pending[0].id, "epic fac-q1").await.expect("ok");
        store
            .close(&pending[0].id, "epic fac-q1")
            .await
            .expect("ok");
        let answered = QueuedPlanner::new(
            store.clone(),
            Arc::new(app::testing::FixedClock(
                domain::Timestamp::from_unix_seconds(0),
            )),
            domain::Duration::from_seconds(0),
            domain::Duration::from_seconds(0),
        );
        let seeded = store
            .create(app::plan_request("second", "t"))
            .await
            .expect("ok");
        store.note(&seeded, "failed: nope").await.expect("ok");
        store.close(&seeded, "failed: nope").await.expect("ok");
        // `submit` creates its own request, which nobody closes → timeout again; outcome parsing is covered in app.
        assert!(answered.submit("third").await.is_err());
    }

    #[test]
    fn registry_builds_from_specs() {
        let dir = std::env::temp_dir().join(format!("console-reg-{}", std::process::id()));
        let spec = RigSpec {
            facts: crate::config::RigFacts::default(),
            name: RigName::try_new("toy").expect("r"),
            ledger: dir.join("ledger"),
            events: dir.join("logs").join("events.jsonl"),
            planner: PlannerSpec::Command {
                argv: vec!["true".into()],
            },
            budget: domain::RigBudget::default(),
        };
        let queued = RigSpec {
            facts: crate::config::RigFacts::default(),
            name: RigName::try_new("q").expect("r"),
            planner: PlannerSpec::Queue {
                timeout: std::time::Duration::from_secs(1),
            },
            ..spec.clone()
        };
        let reg = FileRegistry::build(&[spec, queued]).expect("built");
        assert_eq!(reg.names().len(), 2);
        assert!(reg.rig(&RigName::try_new("toy").expect("r")).is_some());
        assert!(reg.rig(&RigName::try_new("nope").expect("r")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
