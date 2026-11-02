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
    CreateSpec, Layout, RigCmdError, STOP_ROLES, backup, console, create, destroy, doctor, list,
    restore, start, stop,
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
    assert!(
        stop(&docker, &l, "toy")
            .await
            .unwrap()
            .contains("ledger up")
    );
    assert!(
        start(&docker, &l, "toy")
            .await
            .unwrap()
            .contains("started rig toy")
    );
    assert!(stop(&docker, &l, "nope").await.is_err());
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

/// Top-level compose services whose command is a task-working role, read from the real file.
fn compose_worker_services(compose: &str) -> Vec<&str> {
    let mut current = "";
    let mut workers = Vec::new();
    for line in compose.lines() {
        if let Some(name) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':'))
            && !name.starts_with([' ', '#'])
        {
            current = name;
        }
        if line.trim_start().starts_with("command: [work,") {
            workers.push(current);
        }
    }
    workers
}

#[test]
fn compose_workers_are_all_stopped() {
    let compose = include_str!("../../../compose.yaml");
    let workers = compose_worker_services(compose);
    assert_eq!(workers, ["worker", "worker-codex", "worker-opencode"]);
    for worker in workers {
        assert!(STOP_ROLES.contains(&worker), "{worker} survives rig stop");
    }
}

#[tokio::test]
async fn stop_names_every_worker_profile_and_leaves_the_ledger_up() {
    let l = layout("stop-all");
    let docker = FakeHostDocker::default();
    create(
        &docker,
        &l,
        &spec("toy", "u", "rust", "opencode", None, false),
    )
    .await
    .unwrap();
    stop(&docker, &l, "toy").await.unwrap();
    let calls = docker.calls.lock().await.clone();
    let call = calls.iter().find(|c| c.contains(" stop ")).unwrap();
    for service in ["worker-opencode", "worker-codex", "worker", "planner"] {
        assert!(
            call.split_whitespace().any(|w| w == service),
            "{service} not in {call}"
        );
    }
    assert!(!call.split_whitespace().any(|w| w == "ledger"));

    // A rig whose doctor fails is stopped again, by the same list.
    docker.calls.lock().await.clear();
    docker
        .ps
        .lock()
        .await
        .insert("factory-toy".into(), "FAIL auth\n".into());
    assert!(matches!(
        start(&docker, &l, "toy").await,
        Err(RigCmdError::Doctor { .. })
    ));
    let calls = docker.calls.lock().await.clone();
    let call = calls.iter().find(|c| c.contains(" stop ")).unwrap();
    assert!(call.split_whitespace().any(|w| w == "worker-opencode"));
    let _ = std::fs::remove_dir_all(&l.root);
}
