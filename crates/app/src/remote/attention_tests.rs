#![allow(clippy::too_many_lines, reason = "one scenario per option")]
use domain::task::IncidentReason;
use domain::{Attempts, BeadId, BeadKind, BudgetExceeded, Scope, TaskState};

use super::attention::{
    AttentionOption, attention_for, guidance_notes, incident_task_id, last_verify_block,
};
use super::remote_fixtures_tests::{clock, id, meta, seeded, who};
use super::service::{RemoteError, Sent, apply_option, get_task, list_tasks, overview};
use crate::testing::{FakeStore, plain_bead};
use crate::{BeadStore as _, NewBead};

#[test]
fn notes_are_mined_for_evidence() {
    let notes = "attempt 1\nverify FAILED\n$ sh tests/run.sh\n[exit 1]\nboom\nguidance: use POSIX sh\nverify FAILED\n$ make test\n[exit 2]\nnope";
    assert_eq!(
        last_verify_block(notes).as_deref(),
        Some("verify FAILED\n$ make test\n[exit 2]\nnope")
    );
    assert_eq!(guidance_notes(notes), vec!["use POSIX sh".to_owned()]);
    assert_eq!(last_verify_block("nothing"), None);
    let mut inc = plain_bead(id("inc-1"), Some(BeadKind::Incident));
    inc.title = "incident on ep-1.3".into();
    assert_eq!(incident_task_id(&inc), Some(id("ep-1.3")));
    assert_eq!(
        incident_task_id(&plain_bead(id("q-1"), Some(BeadKind::Question))),
        None
    );
    for o in [
        AttentionOption::RetryFresh,
        AttentionOption::RetryWithGuidance,
        AttentionOption::StopEpic,
        AttentionOption::Replan,
        AttentionOption::Answer,
    ] {
        assert_eq!(AttentionOption::parse(o.as_str()), Ok(o));
    }
    assert!(AttentionOption::parse("nope").is_err());
}

#[test]
fn attention_reflects_the_task_state() {
    let mut inc = plain_bead(id("inc-1"), Some(BeadKind::Incident));
    inc.title = "incident on ep-1.3".into();
    inc.description = "budget exhausted: attempts".into();
    let mut task = plain_bead(id("ep-1.3"), Some(BeadKind::Task));
    task.parent = Some(id("ep-1"));
    task.notes = Some("verify FAILED\n$ x\n[exit 1]\nguidance: try harder".into());
    task.meta = Some(meta(
        TaskState::Incident {
            reason: IncidentReason::Budget {
                exceeded: BudgetExceeded::Attempts {
                    used: Attempts::new(3),
                    limit: Attempts::new(3),
                },
            },
        },
        500,
    ));
    let a = attention_for(&inc, Some(&task));
    assert_eq!(a.reason.kind, "budget");
    assert_eq!(a.epic_id.as_deref(), Some("ep-1"));
    assert_eq!(a.tokens.map(|c| c.used), Some(500));
    assert_eq!(a.attempts.map(|c| c.limit), Some(3));
    assert_eq!(
        a.last_verify.as_deref(),
        Some("verify FAILED\n$ x\n[exit 1]")
    );
    assert_eq!(a.guidance, vec!["try harder".to_owned()]);
    assert_eq!(a.options.len(), 4);
    assert!(
        a.options
            .iter()
            .any(|o| o.id == AttentionOption::StopEpic && o.destructive)
    );
    let json = serde_json::to_value(&a).expect("json");
    assert_eq!(json["reason"]["kind"], "budget");
    assert_eq!(json["options"][1]["id"], "retry_with_guidance");

    let q = plain_bead(id("q-1"), Some(BeadKind::Question));
    let a = attention_for(&q, None);
    assert_eq!(a.reason.kind, "question");
    assert_eq!(a.options.len(), 1);
    assert!(a.tokens.is_none());
    let moved_on = plain_bead(id("ep-1.3"), Some(BeadKind::Task));
    let mut t2 = moved_on.clone();
    t2.meta = Some(meta(TaskState::Open, 0));
    assert_eq!(attention_for(&inc, Some(&t2)).reason.kind, "resolved");
    let mut t3 = moved_on;
    t3.meta = Some(meta(
        TaskState::InVerify {
            branch: domain::BranchName::try_new("task/x").expect("b"),
            head: domain::Sha::try_new("2".repeat(40)).expect("s"),
        },
        0,
    ));
    assert_eq!(
        attention_for(&inc, Some(&t3)).branch.as_deref(),
        Some("task/x")
    );
}

async fn seeded_incident() -> (super::Rig, std::sync::Arc<FakeStore>, BeadId) {
    let (rig, store, _, _) = seeded().await;
    store
        .seed_task(
            id("ep-1.3"),
            meta(
                TaskState::Incident {
                    reason: IncidentReason::MergeConflict {
                        detail: "lib.sh".into(),
                    },
                },
                10,
            ),
        )
        .await;
    store.set_parent(&id("ep-1.3"), &id("ep-1")).await;
    store
        .note(&id("ep-1.3"), "verify FAILED\n$ sh t.sh\n[exit 1]")
        .await
        .expect("note");
    let inc = store
        .create(NewBead {
            title: domain::Title::try_new("incident on ep-1.3").expect("t"),
            description: "the branch no longer merges".into(),
            kind: BeadKind::Incident,
            priority: domain::Priority::CRITICAL,
            parent: Some(id("ep-1")),
            needs: vec![],
            acceptance: None,
            meta: None,
        })
        .await
        .expect("created");
    (rig, store, inc)
}

#[tokio::test]
async fn inbox_tasks_carry_attention_and_epics_carry_children() {
    let (rig, _, inc) = seeded_incident().await;
    let tasks = list_tasks(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .expect("ok");
    let item = tasks
        .iter()
        .find(|t| t.id == inc.to_string())
        .expect("inbox item");
    let data = item
        .status
        .message
        .as_ref()
        .and_then(|m| {
            m.parts.iter().find_map(|p| match p {
                super::a2a::Part::Data(v) => Some(v.clone()),
                super::a2a::Part::Text(_) => None,
            })
        })
        .expect("data part");
    assert_eq!(data["reason"]["kind"], "merge_conflict");
    assert_eq!(data["taskId"], "ep-1.3");
    assert!(
        data["lastVerify"]
            .as_str()
            .is_some_and(|s| s.contains("exit 1"))
    );
    let epic = get_task(&rig, &clock(), &who(&[Scope::Watch]), "ep-1")
        .await
        .expect("epic");
    let children = epic.metadata["factory"]["children"]
        .as_array()
        .expect("children");
    assert_eq!(children.len(), 3);
    assert!(
        children
            .iter()
            .any(|c| c["id"] == "ep-1.3" && c["state"] == "incident" && c["tokens"] == 10)
    );
    let o = overview(&rig, &clock(), &who(&[Scope::Watch]))
        .await
        .expect("ok");
    assert_eq!((o.epics, o.attention, o.done), (1, 2, 0));
}

#[tokio::test]
async fn options_do_what_they_say() {
    let (rig, store, inc) = seeded_incident().await;
    let admin = who(&[Scope::Admin]);
    assert_eq!(
        apply_option(
            &rig,
            &clock(),
            &admin,
            inc.as_ref(),
            AttentionOption::RetryWithGuidance,
            "  "
        )
        .await,
        Err(RemoteError::EmptyMessage)
    );
    let sent = apply_option(
        &rig,
        &clock(),
        &admin,
        inc.as_ref(),
        AttentionOption::RetryWithGuidance,
        "use POSIX sh",
    )
    .await
    .expect("ok");
    assert!(matches!(
        sent,
        Sent::Resolved {
            reopened: Some(_),
            ..
        }
    ));
    let task = store.show(&id("ep-1.3")).await.expect("task");
    assert!(
        task.notes
            .as_deref()
            .is_some_and(|n| n.contains("guidance: use POSIX sh"))
    );
    assert!(matches!(task.meta.map(|m| m.state), Some(TaskState::Open)));

    let (rig2, _, inc2) = seeded_incident().await;
    assert!(matches!(
        apply_option(
            &rig2,
            &clock(),
            &admin,
            inc2.as_ref(),
            AttentionOption::RetryFresh,
            ""
        )
        .await,
        Ok(Sent::Resolved { .. })
    ));

    let (rig3, store3, inc3) = seeded_incident().await;
    let sent = apply_option(
        &rig3,
        &clock(),
        &admin,
        inc3.as_ref(),
        AttentionOption::StopEpic,
        "",
    )
    .await
    .expect("ok");
    assert!(matches!(sent, Sent::Resolved { reopened: None, .. }));
    assert_eq!(
        store3.show(&id("ep-1")).await.expect("epic").status,
        crate::BeadStatus::Closed
    );

    let (rig4, _, inc4) = seeded_incident().await;
    let sent = apply_option(
        &rig4,
        &clock(),
        &admin,
        inc4.as_ref(),
        AttentionOption::Replan,
        "smaller steps",
    )
    .await
    .expect("ok");
    assert!(matches!(sent, Sent::Planned(_)));
    let (rig5, _, inc5) = seeded_incident().await;
    assert!(matches!(
        apply_option(
            &rig5,
            &clock(),
            &who(&[Scope::Watch]),
            inc5.as_ref(),
            AttentionOption::StopEpic,
            ""
        )
        .await,
        Err(RemoteError::Forbidden(_))
    ));
    assert!(matches!(
        apply_option(
            &rig5,
            &clock(),
            &admin,
            "nope",
            AttentionOption::RetryFresh,
            ""
        )
        .await,
        Err(RemoteError::TaskNotFound { .. })
    ));
    let (rig6, _, _) = seeded_incident().await;
    let q = rig6
        .store
        .create(NewBead {
            title: domain::Title::try_new("why?").expect("t"),
            description: "which db?".into(),
            kind: BeadKind::Question,
            priority: domain::Priority::HIGH,
            parent: None,
            needs: vec![],
            acceptance: None,
            meta: None,
        })
        .await
        .expect("q");
    assert!(matches!(
        apply_option(
            &rig6,
            &clock(),
            &admin,
            q.as_ref(),
            AttentionOption::Answer,
            "postgres"
        )
        .await,
        Ok(Sent::Resolved { reopened: None, .. })
    ));
    assert!(matches!(
        apply_option(
            &rig6,
            &clock(),
            &admin,
            q.as_ref(),
            AttentionOption::StopEpic,
            ""
        )
        .await,
        Err(RemoteError::TaskNotFound { .. })
    ));
}

#[test]
fn environment_incidents_offer_resume_first() {
    let mut inc = plain_bead(id("inc-1"), Some(BeadKind::Incident));
    inc.title = "incident on ep-1.3".into();
    let mut task = plain_bead(id("ep-1.3"), Some(BeadKind::Task));
    task.meta = Some(meta(
        TaskState::Incident {
            reason: IncidentReason::Environment {
                detail: "exit 127".into(),
            },
        },
        10,
    ));
    let a = attention_for(&inc, Some(&task));
    assert_eq!(a.reason.kind, "environment");
    assert_eq!(a.options[0].id, AttentionOption::ResumeBranch);
    assert_eq!(
        AttentionOption::parse("resume_branch"),
        Ok(AttentionOption::ResumeBranch)
    );
    assert_eq!(a.options.len(), 3);
}

#[tokio::test]
async fn resume_branch_marks_the_task_and_reopens_it() {
    let (rig, store, _, _) = seeded().await;
    store
        .seed_task(
            id("ep-1.3"),
            meta(
                TaskState::Incident {
                    reason: IncidentReason::Environment {
                        detail: "noexec".into(),
                    },
                },
                10,
            ),
        )
        .await;
    store.set_parent(&id("ep-1.3"), &id("ep-1")).await;
    let inc = store
        .create(NewBead {
            title: domain::Title::try_new("incident on ep-1.3").expect("t"),
            description: "env".into(),
            kind: BeadKind::Incident,
            priority: domain::Priority::CRITICAL,
            parent: Some(id("ep-1")),
            needs: vec![],
            acceptance: None,
            meta: None,
        })
        .await
        .expect("created");
    let sent = apply_option(
        &rig,
        &clock(),
        &who(&[Scope::Admin]),
        inc.as_ref(),
        AttentionOption::ResumeBranch,
        "",
    )
    .await
    .expect("ok");
    assert!(matches!(
        sent,
        Sent::Resolved {
            reopened: Some(_),
            ..
        }
    ));
    let task = store.show(&id("ep-1.3")).await.expect("task");
    assert!(
        task.notes
            .as_deref()
            .is_some_and(|n| n.contains("resume-from: task/ep-1.3"))
    );
    assert_eq!(
        crate::worker::resume_branch(task.notes.as_deref()).map(|b| b.to_string()),
        Some("task/ep-1.3".to_owned())
    );
    assert!(crate::worker::resume_branch(Some("nothing here")).is_none());
    assert!(matches!(task.meta.map(|m| m.state), Some(TaskState::Open)));
}
