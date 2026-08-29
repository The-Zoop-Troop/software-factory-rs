#![allow(
    clippy::too_many_lines,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests"
)]

use std::path::Path;

use app::domain::Timestamp;
use app::testing::FakeHostDocker;

use crate::rig::{
    CreateSpec, Layout, RigCmdError, backup, console, create, destroy, doctor, list, restore,
};

fn spec(
    name: &str,
    repo: &str,
    runtime: &str,
    harness: &str,
    secrets: Option<&Path>,
    start: bool,
) -> CreateSpec {
    CreateSpec {
        name: name.into(),
        repo_url: repo.into(),
        runtime: runtime.into(),
        harness: harness.into(),
        main: "main".into(),
        secrets: secrets.map(Path::to_path_buf),
        start,
    }
}

fn layout(tag: &str) -> Layout {
    let root = std::env::temp_dir().join(format!("factory-rig-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    Layout {
        root,
        compose_file: Path::new("/repo/compose.yaml").to_path_buf(),
    }
}

#[tokio::test]
async fn create_list_doctor_backup_restore_destroy() {
    let l = layout("full");
    let docker = FakeHostDocker::default();
    assert!(list(&l).unwrap().starts_with("no rigs"));
    let out = create(
        &docker,
        &l,
        &spec("toy", "git@x:y.git", "rust", "claude", None, true),
    )
    .await
    .unwrap();
    assert!(out.contains("project factory-toy, console port 7700"));
    assert!(out.contains("started"));
    assert!(l.root.join("toy/compose.env").exists());
    assert!(
        std::fs::read_to_string(l.root.join("toy/rig.env"))
            .unwrap()
            .contains("RIG_REPO_URL=git@x:y.git")
    );
    assert!(l.root.join("console/tokens.toml").exists());
    assert!(
        std::fs::read_to_string(l.root.join("console/compose.yaml"))
            .unwrap()
            .contains("factory-toy_ledger")
    );
    assert!(matches!(
        create(
            &docker,
            &l,
            &spec("toy", "u", "rust", "claude", None, false)
        )
        .await,
        Err(RigCmdError::Registry(app::RegistryError::Exists { .. }))
    ));
    assert!(matches!(
        create(
            &docker,
            &l,
            &spec("Bad Name", "u", "rust", "claude", None, false)
        )
        .await,
        Err(RigCmdError::Name { .. })
    ));
    // Secrets file is copied.
    let secrets = l.root.join("my.env");
    std::fs::write(&secrets, "ANTHROPIC_API_KEY=k\n").unwrap();
    create(
        &docker,
        &l,
        &spec("api", "u", "node", "codex", Some(&secrets), false),
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(l.root.join("api/rig.env")).unwrap(),
        "ANTHROPIC_API_KEY=k\n"
    );
    let listed = list(&l).unwrap();
    assert!(
        listed.contains("toy  git@x:y.git  runtime=rust harness=claude console=127.0.0.1:7700")
    );
    assert!(listed.contains("api  u  runtime=node harness=codex console=127.0.0.1:7701"));
    docker
        .volumes
        .lock()
        .await
        .insert("factory-toy_ledger".into());
    docker
        .ps
        .lock()
        .await
        .insert("factory-toy".into(), "steward\n".into());
    let report = doctor(&docker, &l).await.unwrap();
    assert!(report.contains("ok   toy  ledger=yes running=[steward]"));
    assert!(report.contains("!!   api  ledger=missing running=[]"));
    let b = backup(
        &docker,
        &l,
        "toy",
        &l.root.join("backups"),
        Timestamp::from_unix_seconds(1),
    )
    .await
    .unwrap();
    assert!(b.contains("toy-ledger-1.tgz") && b.contains("toy-repo-1.tgz"));
    assert!(matches!(
        backup(
            &docker,
            &l,
            "nope",
            &l.root,
            Timestamp::from_unix_seconds(1)
        )
        .await,
        Err(RigCmdError::Registry(_))
    ));
    // Restore refuses while running, works when stopped.
    let err = restore(&docker, &l, "toy", Path::new("/b/l.tgz"), None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("is running"));
    docker.ps.lock().await.remove("factory-toy");
    let out = restore(
        &docker,
        &l,
        "toy",
        Path::new("/b/l.tgz"),
        Some(Path::new("/b/r.tgz")),
    )
    .await
    .unwrap();
    assert!(out.contains("factory-toy_repo"));
    assert_eq!(docker.restored.lock().await.len(), 2);
    assert!(
        console(&docker, &l, true)
            .await
            .unwrap()
            .contains("2 rig(s)")
    );
    assert!(console(&docker, &l, false).await.unwrap().contains("down"));
    let out = destroy(&docker, &l, "toy", true).await.unwrap();
    assert!(out.contains("and its volumes"));
    assert!(!l.root.join("toy").exists());
    assert!(
        !std::fs::read_to_string(l.root.join("console/compose.yaml"))
            .unwrap()
            .contains("factory-toy_ledger")
    );
    assert!(destroy(&docker, &l, "toy", false).await.is_err());
    let calls = docker.calls.lock().await.clone();
    assert!(
        calls
            .iter()
            .any(|c| c.contains("factory-toy") && c.contains("up -d"))
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("factory-console") && c.contains("up -d"))
    );
    assert!(calls.iter().any(|c| c.contains("down -v")));
    let _ = std::fs::remove_dir_all(&l.root);
}

#[tokio::test]
async fn docker_failures_and_malformed_registry_surface() {
    let l = layout("broken");
    let docker = FakeHostDocker {
        fail: true,
        ..FakeHostDocker::default()
    };
    // Registration still happens; the start fails.
    assert!(matches!(
        create(&docker, &l, &spec("toy", "u", "rust", "claude", None, true)).await,
        Err(RigCmdError::Docker(_))
    ));
    assert!(matches!(
        doctor(&docker, &l).await,
        Err(RigCmdError::Docker(_))
    ));
    assert!(matches!(
        destroy(&docker, &l, "toy", false).await,
        Err(RigCmdError::Docker(_))
    ));
    std::fs::write(l.root.join("rigs.toml"), "not = [toml").unwrap();
    assert!(matches!(list(&l), Err(RigCmdError::Malformed { .. })));
    let _ = std::fs::remove_dir_all(&l.root);
}
