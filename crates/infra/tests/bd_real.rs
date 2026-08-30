//! `BdCli` against a real `bd` ledger in a temp directory. Skipped when `bd` is not installed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

use std::path::PathBuf;
use std::process::Command;

use infra::BdCli;
use infra::app::domain::{
    Attempts, BeadId, BeadKind, BeadMeta, Budget, FactoryMeta, MergeMeta, NonEmpty, Priority, Sha,
    TaskState, Title, Usage, VerifyCommand, VerifyMeta,
};
use infra::app::{BeadStatus, BeadStore, NewBead, StoreError};

fn ledger() -> Option<PathBuf> {
    if Command::new("bd").arg("version").output().is_err() {
        eprintln!("bd not installed; skipping");
        return None;
    }
    let dir = std::env::temp_dir().join(format!("factory-bd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new("bd")
        .args(["init", "--prefix", "t", "--non-interactive", "--skip-hooks"])
        .current_dir(&dir)
        .env("BD_NON_INTERACTIVE", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = Command::new("bd")
        .args(["metrics", "off"])
        .current_dir(&dir)
        .output();
    Some(dir)
}

fn sha(c: char) -> Sha {
    Sha::try_new(core::iter::repeat_n(c, 40).collect::<String>()).unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn full_ledger_roundtrip() {
    let Some(dir) = ledger() else { return };
    let store = BdCli::new(&dir).with_actor("test");

    let epic = store
        .create(NewBead {
            title: Title::derived("epic"),
            description: "d".into(),
            kind: BeadKind::Epic,
            priority: Priority::HIGH,
            parent: None,
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await
        .unwrap();
    let verify = store
        .create(NewBead {
            title: Title::derived("verify"),
            description: String::new(),
            kind: BeadKind::Verify,
            priority: Priority::MEDIUM,
            parent: Some(epic.clone()),
            needs: vec![],
            acceptance: None,
            meta: None,
            deferred: false,
        })
        .await
        .unwrap();
    let meta = FactoryMeta {
        verify_bead: verify.clone(),
        base: sha('a'),
        budget: Budget::default(),
        usage: Usage::default(),
        lease_expiries: Attempts::new(0),
        state: TaskState::Open,
    };
    let task = store
        .create(NewBead {
            title: Title::derived("task"),
            description: "do it".into(),
            kind: BeadKind::Task,
            priority: Priority::HIGH,
            parent: Some(epic.clone()),
            needs: vec![],
            acceptance: Some("works".into()),
            meta: Some(BeadMeta::Task(meta.clone())),
            deferred: false,
        })
        .await
        .unwrap();
    // A dependent task, created deferred until its edge exists, then un-deferred.
    let dependent = store
        .create(NewBead {
            title: Title::derived("after"),
            description: String::new(),
            kind: BeadKind::Task,
            priority: Priority::HIGH,
            parent: Some(epic.clone()),
            needs: vec![task.clone()],
            acceptance: None,
            meta: Some(BeadMeta::Task(meta.clone())),
            deferred: false,
        })
        .await
        .unwrap();
    store
        .set_verify(
            &verify,
            &VerifyMeta {
                task: task.clone(),
                commands: NonEmpty::singleton(VerifyCommand::try_new("true").unwrap()),
                timeout: infra::app::domain::Duration::from_seconds(5),
            },
        )
        .await
        .unwrap();
    store.add_needs(&verify, &task).await.unwrap();
    let merge = store
        .create(NewBead {
            title: Title::derived("merge"),
            description: String::new(),
            kind: BeadKind::Merge,
            priority: Priority::HIGH,
            parent: None,
            needs: vec![],
            acceptance: None,
            meta: Some(BeadMeta::Merge(MergeMeta {
                task: task.clone(),
                branch: infra::app::domain::BranchName::try_new("task/x").unwrap(),
                head: sha('b'),
            })),
            deferred: false,
        })
        .await
        .unwrap();

    // show: kind, metadata, labels not inherited from the epic.
    let t = store.show(&task).await.unwrap();
    assert_eq!(t.kind, Some(BeadKind::Task));
    assert_eq!(t.labels, vec!["fac:kind=task"]);
    assert_eq!(t.meta.as_ref().unwrap().verify_bead, verify);
    assert_eq!(t.parent.as_ref(), Some(&epic));
    assert_eq!(t.acceptance.as_deref(), Some("works"));
    assert_eq!(
        store.show(&verify).await.unwrap().verify.unwrap().task,
        task
    );
    assert_eq!(
        store.show(&merge).await.unwrap().merge.unwrap().head,
        sha('b')
    );

    // ready: the dependent task and the verify bead are hidden by their edges.
    let ready: Vec<_> = store
        .ready(BeadKind::Task)
        .await
        .unwrap()
        .into_iter()
        .map(|b| b.id)
        .collect();
    assert!(
        ready.contains(&task) && !ready.contains(&dependent),
        "{ready:?}"
    );
    assert!(store.ready(BeadKind::Verify).await.unwrap().is_empty());

    // metadata + notes roundtrip.
    let leased = FactoryMeta {
        state: TaskState::InVerify {
            branch: infra::app::domain::BranchName::try_new("task/x").unwrap(),
            head: sha('b'),
        },
        ..meta
    };
    store.set_meta(&task, &leased).await.unwrap();
    store.note(&task, "first note").await.unwrap();
    store.note(&task, "second note").await.unwrap();
    let t = store.show(&task).await.unwrap();
    assert!(matches!(t.meta.unwrap().state, TaskState::InVerify { .. }));
    assert!(t.notes.unwrap().contains("second note"));

    // list_active / children / close.
    assert_eq!(store.list_active(BeadKind::Task).await.unwrap().len(), 2);
    assert_eq!(store.children(&epic).await.unwrap().len(), 3);
    // A bead cannot close while a blocker is open: task first, then its dependents.
    assert!(matches!(
        store.close(&dependent, "early").await,
        Err(StoreError::Blocked { ref id, ref by }) if *id == dependent && by.contains(&task)
    ));
    store.close(&task, "done").await.unwrap();
    store.close(&dependent, "done").await.unwrap();
    store.close(&verify, "done").await.unwrap();
    assert_eq!(store.show(&task).await.unwrap().status, BeadStatus::Closed);
    assert_eq!(store.list_active(BeadKind::Task).await.unwrap().len(), 0);
    assert_eq!(
        store.children(&epic).await.unwrap().len(),
        3,
        "children includes closed"
    );

    // errors.
    assert!(matches!(
        store.show(&BeadId::try_new("t-nope").unwrap()).await,
        Err(StoreError::NotFound { .. })
    ));
    let broken = BdCli::new(&dir).with_bin("/nonexistent/bd");
    assert!(matches!(
        store_err(&broken).await,
        StoreError::Unavailable { .. }
    ));
}

async fn store_err(s: &BdCli) -> StoreError {
    s.show(&BeadId::try_new("t-1").unwrap()).await.unwrap_err()
}
