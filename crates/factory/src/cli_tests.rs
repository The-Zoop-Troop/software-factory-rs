//! Tests for the `factory` CLI (sibling file to respect the size cap).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use infra::app::domain::Attempts;

use infra::app::domain::{
    AgentId, Budget, Duration, FactoryMeta, Lease, Sha, TaskState, Timestamp, Usage,
};
use infra::app::{Bead, BeadStatus};

#[test]
fn parses_every_command() {
    let c = Cli::parse_from(["factory", "version"]);
    assert!(matches!(c.command, Command::Version));
    let c = Cli::parse_from(["factory", "--workdir", "/w", "bead", "show", "fac-1"]);
    assert_eq!(c.workdir, PathBuf::from("/w"));
    assert!(
        matches!(c.command, Command::Bead { command: BeadCommand::Show { ref id } } if id == "fac-1")
    );
    let c = Cli::parse_from([
        "factory",
        "plan",
        "--harness",
        "opencode",
        "--model",
        "p/m",
        "--text",
        "hi",
    ]);
    assert!(matches!(
        c.command,
        Command::Plan(crate::plan_cmd::PlanArgs {
            harness: HarnessKind::Opencode,
            ..
        })
    ));
    let c = Cli::parse_from([
        "factory",
        "work",
        "--harness",
        "codex",
        "--agent",
        "w9",
        "--lease-ttl",
        "7",
        "--interval",
        "3",
    ]);
    assert!(
        matches!(c.command, Command::Work { harness: HarnessKind::Codex, ref agent, lease_ttl: 7, interval: Some(3), .. } if agent == "w9")
    );
    let c = Cli::parse_from(["factory", "verify", "--repo", "r"]);
    assert!(matches!(c.command, Command::Verify { interval: None, .. }));
    let c = Cli::parse_from([
        "factory",
        "integrate",
        "--check",
        "a",
        "--check",
        "b",
        "--remote",
        "origin",
    ]);
    assert!(
        matches!(c.command, Command::Integrate { ref checks, remote: Some(ref r), .. } if checks.len() == 2 && r == "origin")
    );
    assert!(Cli::try_parse_from(["factory", "bogus"]).is_err());
}

#[test]
fn build_harness_variants() {
    assert!(build_harness(HarnessKind::Claude, Some("m".into()), 1.0).is_ok());
    assert!(build_harness(HarnessKind::Codex, None, 1.0).is_ok());
    assert!(build_harness(HarnessKind::Opencode, Some("p/m".into()), 1.0).is_ok());
    assert!(
        build_harness(HarnessKind::Opencode, None, 1.0).is_err(),
        "opencode needs a model"
    );
    assert!(build_harness(HarnessKind::Opencode, Some("nope".into()), 1.0).is_err());
}

fn bead(meta: Option<FactoryMeta>) -> Bead {
    Bead {
        id: infra::app::domain::BeadId::try_new("fac-1").unwrap(),
        title: "t".into(),
        description: String::new(),
        acceptance: Some("acc".into()),
        notes: Some("n1\nn2".into()),
        status: BeadStatus::Open,
        labels: vec![],
        parent: None,
        kind: meta.is_some().then_some(infra::app::domain::BeadKind::Task),
        meta,
        verify: None,
        merge: None,
        cross_needs: None,
    }
}

fn meta(state: TaskState) -> FactoryMeta {
    FactoryMeta {
        verify_bead: infra::app::domain::BeadId::try_new("fac-2").unwrap(),
        base: Sha::try_new("a".repeat(40)).unwrap(),
        budget: Budget::default(),
        usage: Usage::default(),
        lease_expiries: Attempts::new(0),
        blocked_releases: Attempts::new(0),
        state,
    }
}

#[test]
fn render_covers_every_state() {
    let plain = render(&bead(None));
    assert!(
        plain.contains("(not a factory bead)")
            && plain.contains("accept")
            && plain.contains("    n2")
    );
    let sha = Sha::try_new("b".repeat(40)).unwrap();
    let branch = infra::app::domain::BranchName::try_new("task/fac-1").unwrap();
    let lease = Lease::grant(
        AgentId::try_new("w").unwrap(),
        Timestamp::from_unix_seconds(1),
        Duration::from_seconds(9),
    );
    for (state, needle) in [
        (TaskState::Open, "state     : open"),
        (TaskState::Leased { lease }, "lease     : w until 10"),
        (
            TaskState::InVerify {
                branch: branch.clone(),
                head: sha.clone(),
            },
            "branch    : task/fac-1 @",
        ),
        (
            TaskState::Mergeable {
                branch,
                head: sha.clone(),
            },
            "branch    : task/fac-1 @",
        ),
        (TaskState::Closed { merged: sha }, "merged    :"),
        (
            TaskState::Incident {
                reason: infra::app::domain::task::IncidentReason::Manual { detail: "x".into() },
            },
            "incident  :",
        ),
    ] {
        let out = render(&bead(Some(meta(state))));
        assert!(out.contains(needle), "{out}");
        assert!(out.contains("budget    : tokens 0/400000"));
    }
}

#[test]
fn render_summary_shows_progress() {
    let mut s = infra::app::LedgerSummary::default();
    let mut e = infra::app::EpicSummary {
        title: "Epic".into(),
        ..Default::default()
    };
    e.by_state.insert("closed", 4);
    e.by_state.insert("open", 1);
    e.total = 5;
    s.epics
        .insert(infra::app::domain::BeadId::try_new("fac-e").unwrap(), e);
    s.open_incidents = 1;
    let out = render_summary(&s);
    assert!(out.contains("fac-e  Epic  [4/5] closed=4 open=1"));
    assert!(out.contains("incidents: 1"));
    assert!(Cli::try_parse_from(["factory", "doctor"]).is_ok());
    assert!(Cli::try_parse_from(["factory", "inbox", "--resolve", "fac-1", "--note", "n"]).is_ok());
    assert!(Cli::try_parse_from(["factory", "watch", "--interval", "5"]).is_ok());
}

#[tokio::test]
async fn run_version_and_missing_plan_text() {
    assert!(run(Cli::parse_from(["factory", "version"])).await.is_ok());
    let err = run(Cli::parse_from(["factory", "plan"])).await.unwrap_err();
    assert!(err.to_string().contains("--text or --file"));
}

#[test]
fn tilde_expands_to_home() {
    use std::path::PathBuf;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("HOME set in tests");
    assert_eq!(
        super::expand_home(PathBuf::from("~/.factory")),
        home.join(".factory")
    );
    assert_eq!(
        super::expand_home(PathBuf::from("/abs/x")),
        PathBuf::from("/abs/x")
    );
    assert_eq!(
        super::expand_home(PathBuf::from("rel")),
        PathBuf::from("rel")
    );
}

#[test]
fn plan_queued_conflicts_with_queue() {
    assert!(Cli::try_parse_from(["factory", "plan", "--queued", "--queue"]).is_err());
    let c = Cli::parse_from(["factory", "plan", "--queued", "--text", "hi"]);
    assert!(matches!(
        c.command,
        Command::Plan(crate::plan_cmd::PlanArgs { queued: true, .. })
    ));
}
