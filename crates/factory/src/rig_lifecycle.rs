//! Rig lifecycle: start (with the doctor gate) and stop (ledger stays up).

use std::path::Path;

use app::domain::RigName;
use app::{HostDocker, HostRig};

use super::{Layout, RigCmdError};

/// `docker compose` arguments that stop everything able to work on a task. Compose stops a named
/// service whether or not its profile is active, so every worker profile is listed; a worker left
/// running keeps spending tokens with nothing supervising it. `compose_workers_are_all_stopped`
/// reads `compose.yaml`, so a new worker profile fails the build until it is added here.
pub(crate) const STOP_ROLES: [&str; 9] = [
    "stop",
    "steward",
    "verifier",
    "integrator",
    "worker",
    "worker-codex",
    "worker-opencode",
    "planner",
    "egress",
];

/// Roles and egress down, `ledger` left up: a stopped rig keeps its history readable by the
/// console (server mode reads beads through the ledger service).
pub(crate) async fn stop(
    docker: &dyn HostDocker,
    layout: &Layout,
    name: &str,
) -> Result<String, RigCmdError> {
    let name = RigName::try_new(name).map_err(|e| RigCmdError::Name {
        detail: e.to_string(),
    })?;
    let registry = layout.load()?;
    let rig = registry
        .get(&name)
        .cloned()
        .ok_or_else(|| app::RegistryError::Unknown { name: name.clone() })?;
    let env = layout.rig_dir(&name).join("compose.env");
    docker
        .compose(&rig.project(), &env, &layout.compose_file, &STOP_ROLES)
        .await?;
    Ok(format!(
        "stopped rig {name} (ledger up: history stays readable)\n"
    ))
}

/// Everything up again (ledger, egress, roles), profiles included as configured.
pub(crate) async fn start(
    docker: &dyn HostDocker,
    layout: &Layout,
    name: &str,
) -> Result<String, RigCmdError> {
    let name = RigName::try_new(name).map_err(|e| RigCmdError::Name {
        detail: e.to_string(),
    })?;
    let registry = layout.load()?;
    let rig = registry
        .get(&name)
        .cloned()
        .ok_or_else(|| app::RegistryError::Unknown { name: name.clone() })?;
    let env = layout.rig_dir(&name).join("compose.env");
    checked_up(docker, &rig, &env, layout).await?;
    Ok(format!("started rig {name} (doctor ok)\n"))
}

/// Bring a rig up and run its doctor; a rig that cannot plan (missing harness, credential, or
/// runtime tool) is stopped again and the doctor report is the error, instead of a rig that
/// fails every plan quietly.
pub(super) async fn checked_up(
    docker: &dyn HostDocker,
    rig: &HostRig,
    env: &Path,
    layout: &Layout,
) -> Result<(), RigCmdError> {
    docker
        .compose(&rig.project(), env, &layout.compose_file, &["up", "-d"])
        .await?;
    let report = docker
        .compose(
            &rig.project(),
            env,
            &layout.compose_file,
            &["run", "--rm", "-T", "shell", "doctor"],
        )
        .await?;
    let failing = report
        .lines()
        .any(|l| l.starts_with("!!") || l.starts_with("FAIL"));
    if failing {
        docker
            .compose(&rig.project(), env, &layout.compose_file, &STOP_ROLES)
            .await?;
        return Err(RigCmdError::Doctor {
            rig: rig.name.to_string(),
            report,
        });
    }
    Ok(())
}
