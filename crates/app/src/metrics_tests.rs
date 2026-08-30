//! Metrics over the anonymised Phase 0 log of the guide project (one worker, five tasks).

use super::*;

const PHASE0: &str = include_str!("../fixtures/phase0-events.jsonl");

fn log() -> Vec<EventRecord> {
    PHASE0
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("fixture line"))
        .collect()
}

fn rec(at: i64, bead: &str, kind: &str, detail: &[(&str, serde_json::Value)]) -> EventRecord {
    EventRecord {
        at: at.to_string(),
        actor: "t".into(),
        bead: Some(domain::BeadId::try_new(bead).expect("id")),
        kind: kind.into(),
        detail: detail
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    }
}

#[test]
fn phase0_stage_table_matches_the_exec_plan() {
    let m = epic("ex-1", &log());
    assert_eq!(m.tasks.len(), 5);
    assert_eq!(m.landed, 5);
    // 61 min from the first claim to epic_closed (the plan quoted 69.8 counting planning); serial: ≈ 83 %.
    assert!(
        (3600..=3700).contains(&m.wall_clock),
        "wall {}",
        m.wall_clock
    );
    assert!((3000..=3100).contains(&m.work), "work {}", m.work);
    assert!(
        (80..=86).contains(&m.parallelism_pct),
        "par {}",
        m.parallelism_pct
    );
    // Retries and the rebase after a conflict cost about 25 minutes.
    assert!(
        (1300..=1600).contains(&m.retry_tax),
        "retry tax {}",
        m.retry_tax
    );
    assert_eq!(m.first_pass, 3, "three tasks verified first time");
    assert!(m.tokens > 500_000);
    let stage = |name: &str| m.stages.iter().find(|s| s.stage == name).expect("stage");
    assert_eq!(stage("session").samples, 9);
    assert!(stage("session").max >= 600 && stage("session").max <= 700);
    assert!(stage("verify").p50 <= 60, "verify is fast");
    assert!(stage("integrate").max <= 150);
    // No task_planned edges in this old log: the critical path is the longest landed session.
    assert!(
        m.critical_path >= 600 && m.critical_path <= 700,
        "cp {}",
        m.critical_path
    );
    // One worker: never more than one live session.
    assert!(m.concurrency.iter().all(|(_, n)| *n <= 1));
    assert!(m.concurrency.iter().any(|(_, n)| *n == 1));
    assert_eq!(
        stage("queue_wait").samples,
        0,
        "no ready edge without task_planned"
    );
}

#[test]
fn ready_derives_from_planned_and_landed_needs() {
    let log = vec![
        rec(
            0,
            "e-1.1",
            "task_planned",
            &[("needs", serde_json::json!([]))],
        ),
        rec(
            0,
            "e-1.2",
            "task_planned",
            &[("needs", serde_json::json!(["e-1.1"]))],
        ),
        rec(10, "e-1.1", "claimed", &[]),
        rec(70, "e-1.1", "submitted", &[("tokens", 5.into())]),
        rec(80, "e-1.1", "verify_started", &[]),
        rec(90, "e-1.1", "verified", &[("passed", true.into())]),
        rec(95, "e-1.1", "integrate_started", &[]),
        rec(100, "e-1.1", "integrated", &[("landed", "abc".into())]),
        rec(130, "e-1.2", "claimed", &[]),
        rec(170, "e-1.2", "submitted", &[("tokens", 7.into())]),
        rec(171, "e-1.2", "verified", &[("passed", false.into())]),
        rec(180, "e-1.2", "claimed", &[]),
        rec(200, "e-1.2", "submitted", &[("tokens", 1.into())]),
        rec(201, "e-1.2", "verified", &[("passed", true.into())]),
        rec(210, "e-1.2", "integrated", &[("landed", "def".into())]),
        rec(211, "e-1", "epic_closed", &[]),
    ];
    let m = epic("e-1", &log);
    let stage = |name: &str| m.stages.iter().find(|s| s.stage == name).expect("stage");
    // e-1.1 ready at 0, claimed at 10; e-1.2 ready when e-1.1 landed (100), claimed at 130.
    assert_eq!(stage("queue_wait").samples, 2);
    assert_eq!(stage("queue_wait").max, 30);
    assert_eq!(stage("verify_wait").p50, 10);
    assert_eq!(stage("integrate_wait").max, 5);
    assert_eq!(m.wall_clock, 211);
    assert_eq!(m.work, 60 + 40 + 20);
    assert_eq!(m.retry_tax, 40, "the failed attempt's session");
    assert_eq!(m.critical_path, 60 + 20, "landed sessions along the chain");
    assert_eq!(m.first_pass, 1);
    assert_eq!(m.tokens, 13);
    let t2 = m.tasks.iter().find(|t| t.task == "e-1.2").expect("t2");
    assert_eq!(t2.attempts[0].ended_by.as_deref(), Some("verify_failed"));
    assert_eq!(t2.needs, vec!["e-1.1".to_owned()]);
}

#[test]
fn attempts_end_on_release_reap_and_escalation_and_ignore_other_epics() {
    let log = vec![
        rec(0, "e-1.1", "claimed", &[]),
        rec(5, "e-1.1", "lease_reaped", &[]),
        rec(6, "e-1.1", "claimed", &[]),
        rec(9, "e-1.1", "released", &[]),
        rec(10, "e-1.1", "claimed", &[]),
        rec(20, "e-1.1", "escalated", &[]),
        rec(30, "e-2.1", "claimed", &[]),
        rec(0, "e-10.1", "claimed", &[]),
    ];
    let m = epic("e-1", &log);
    assert_eq!(m.tasks.len(), 1, "e-2 and e-10 are other epics");
    let ends: Vec<_> = m.tasks[0]
        .attempts
        .iter()
        .map(|a| a.ended_by.clone().unwrap_or_default())
        .collect();
    assert_eq!(ends, ["lease_reaped", "released", "escalated"]);
    assert_eq!(m.landed, 0);
    assert_eq!(m.work, 0, "no session ever submitted");
    assert!(epic("none", &[]).stages.iter().all(|s| s.samples == 0));
}
