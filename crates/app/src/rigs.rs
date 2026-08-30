//! Many rigs on one host (`factory rig …`): a host registry of rigs, the compose project each
//! one becomes, and the console that fronts them all. Pure rendering plus a `HostDocker` port;
//! the CLI in `factory` is the imperative shell.

use core::fmt::Write as _;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use domain::{RigName, Timestamp};

/// One rig as the host knows it. Secrets live in `<dir>/rig.env`, never here.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostRig {
    pub name: RigName,
    pub repo_url: String,
    pub runtime: String,
    pub harness: String,
    pub main: String,
    /// Host port the rig's console listens on (loopback).
    pub console_port: u16,
    /// The registry root the rig's files live under.
    #[cfg_attr(feature = "serde", serde(default = "default_root"))]
    pub root: PathBuf,
}

fn default_root() -> PathBuf {
    std::env::var_os("HOME").map_or_else(
        || PathBuf::from(".factory"),
        |h| PathBuf::from(h).join(".factory"),
    )
}

impl HostRig {
    /// The compose project name; volumes become `<project>_ledger`, `<project>_repo`.
    #[must_use]
    pub fn project(&self) -> String {
        format!("factory-{}", self.name)
    }

    #[must_use]
    pub fn volume(&self, which: &str) -> String {
        format!("{}_{which}", self.project())
    }

    /// Where the rig's secrets/env live (`<root>/<rig>/rig.env`); absolute so compose finds it
    /// regardless of the working directory.
    #[must_use]
    pub fn env_file(&self) -> PathBuf {
        self.root.join(self.name.as_ref()).join("rig.env")
    }

    /// `compose.env`: everything compose needs to bring this rig up from the shared file.
    #[must_use]
    pub fn compose_env(&self) -> String {
        format!(
            "COMPOSE_PROJECT_NAME={}\nRIG_NAME={}\nRIG_IMAGE=factory-rig:{}\nRIG_HARNESS={}\nRIG_REPO_URL={}\nRIG_MAIN={}\nCONSOLE_PORT={}\nRIG_ENV_FILE={}\n",
            self.project(),
            self.name,
            self.runtime,
            self.harness,
            self.repo_url,
            self.main,
            self.console_port,
            self.env_file().display()
        )
    }
}

/// The host registry file (`rigs.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostRegistry {
    #[cfg_attr(feature = "serde", serde(default))]
    pub rig: Vec<HostRig>,
}

/// Registry edits that cannot be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("rig `{name}` already exists")]
    Exists { name: RigName },
    #[error("rig `{name}` is not registered")]
    Unknown { name: RigName },
    #[error("no free console port in {from}..={to}")]
    NoPort { from: u16, to: u16 },
}

const PORT_FROM: u16 = 7700;
const PORT_TO: u16 = 7799;

impl HostRegistry {
    #[must_use]
    pub fn get(&self, name: &RigName) -> Option<&HostRig> {
        self.rig.iter().find(|r| &r.name == name)
    }

    /// Add a rig, assigning the lowest free console port.
    ///
    /// # Errors
    /// `Exists` or `NoPort`.
    pub fn add(
        &self,
        name: RigName,
        repo_url: String,
        runtime: String,
        harness: String,
        main: String,
        root: PathBuf,
    ) -> Result<(Self, HostRig), RegistryError> {
        if self.get(&name).is_some() {
            return Err(RegistryError::Exists { name });
        }
        let used: BTreeSet<u16> = self.rig.iter().map(|r| r.console_port).collect();
        let console_port =
            (PORT_FROM..=PORT_TO)
                .find(|p| !used.contains(p))
                .ok_or(RegistryError::NoPort {
                    from: PORT_FROM,
                    to: PORT_TO,
                })?;
        let rig = HostRig {
            name,
            repo_url,
            runtime,
            harness,
            main,
            console_port,
            root,
        };
        let mut rigs = self.rig.clone();
        rigs.push(rig.clone());
        Ok((Self { rig: rigs }, rig))
    }

    /// # Errors
    /// `Unknown`.
    pub fn remove(&self, name: &RigName) -> Result<(Self, HostRig), RegistryError> {
        let rig = self
            .get(name)
            .cloned()
            .ok_or_else(|| RegistryError::Unknown { name: name.clone() })?;
        Ok((
            Self {
                rig: self
                    .rig
                    .iter()
                    .filter(|r| &r.name != name)
                    .cloned()
                    .collect(),
            },
            rig,
        ))
    }

    /// The console's `rigs.toml`: every rig's ledger as the console container sees it.
    #[must_use]
    pub fn console_registry(&self) -> String {
        self.rig.iter().fold(String::new(), |mut s, r| {
            let _ = write!(
                s,
                "[[rig]]\nname = \"{}\"\nledger = \"/work/rigs/{}\"\nevents = \"/work/rigs/{}/.factory/events.jsonl\"\n\n",
                r.name, r.name, r.name
            );
            s
        })
    }

    /// A compose file for one console over every rig's ledger volume (external volumes).
    #[must_use]
    pub fn console_compose(&self, image: &str, port: u16) -> String {
        let mounts = self.rig.iter().fold(String::new(), |mut s, r| {
            let _ = writeln!(s, "      - {}:/work/rigs/{}", r.volume("ledger"), r.name);
            s
        });
        let volumes = self.rig.iter().fold(String::new(), |mut s, r| {
            let _ = writeln!(s, "  {}:\n    external: true", r.volume("ledger"));
            s
        });
        format!(
            "# Generated by `factory rig`; edit rigs.toml and re-run instead.\nservices:\n  console:\n    image: {image}\n    command: [console]\n    environment:\n      CONSOLE_URL: ${{CONSOLE_URL:-http://127.0.0.1:{port}}}\n      RUST_LOG: ${{RUST_LOG:-info}}\n    volumes:\n      - ./console:/work/console\n{mounts}    ports:\n      - \"127.0.0.1:{port}:7700\"\n    user: \"10001:10001\"\n    cap_drop: [ALL]\n    security_opt: [no-new-privileges:true]\n    read_only: false\nvolumes:\n{volumes}"
        )
    }
}

/// Where a backup of one volume lands: `<dir>/<rig>-<volume>-<unix>.tgz`.
#[must_use]
pub fn backup_path(dir: &Path, rig: &HostRig, which: &str, at: Timestamp) -> PathBuf {
    dir.join(format!("{}-{which}-{}.tgz", rig.name, at.unix_seconds()))
}

/// Host docker failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("docker {what} failed: {detail}")]
    Command { what: String, detail: String },
    #[error("docker not found: {detail}")]
    Missing { detail: String },
}

/// The host's docker, as the rig commands need it. Shelled out in `infra`; faked in tests.
#[async_trait]
pub trait HostDocker: Send + Sync {
    /// `docker compose -p <project> --env-file <env> -f <file> <args>`; returns stdout.
    ///
    /// # Errors
    /// Non-zero exit or missing docker.
    async fn compose(
        &self,
        project: &str,
        env_file: &Path,
        compose_file: &Path,
        args: &[&str],
    ) -> Result<String, HostError>;
    /// # Errors
    /// Missing docker.
    async fn volume_exists(&self, name: &str) -> Result<bool, HostError>;
    /// Tar a volume's contents into `dest`.
    ///
    /// # Errors
    /// Non-zero exit or missing docker.
    async fn archive_volume(&self, volume: &str, dest: &Path) -> Result<(), HostError>;
    /// Replace a volume's contents from a tarball made by `archive_volume`.
    ///
    /// # Errors
    /// Non-zero exit or missing docker.
    async fn restore_volume(&self, volume: &str, src: &Path) -> Result<(), HostError>;
}

/// `factory rig doctor` line for one rig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigHealth {
    pub name: RigName,
    pub ledger_volume: bool,
    pub running: Vec<String>,
}

/// Ask docker about every rig: does its ledger exist, which services are up.
///
/// # Errors
/// Missing docker; a rig whose compose call fails is reported with no services.
pub async fn doctor(
    docker: &dyn HostDocker,
    registry: &HostRegistry,
    root: &Path,
    compose_file: &Path,
) -> Result<Vec<RigHealth>, HostError> {
    let mut out = Vec::new();
    for rig in &registry.rig {
        let ledger_volume = docker.volume_exists(&rig.volume("ledger")).await?;
        let env = root.join(rig.name.as_ref()).join("compose.env");
        let running = docker
            .compose(
                &rig.project(),
                &env,
                compose_file,
                &["ps", "--services", "--status", "running"],
            )
            .await
            .map(|s| s.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        out.push(RigHealth {
            name: rig.name.clone(),
            ledger_volume,
            running,
        });
    }
    Ok(out)
}

/// Archive the ledger and repo volumes of a rig.
///
/// # Errors
/// Any docker failure.
pub async fn backup(
    docker: &dyn HostDocker,
    rig: &HostRig,
    dir: &Path,
    at: Timestamp,
) -> Result<Vec<PathBuf>, HostError> {
    let mut paths = Vec::new();
    for which in ["ledger", "repo"] {
        let path = backup_path(dir, rig, which, at);
        docker.archive_volume(&rig.volume(which), &path).await?;
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeHostDocker;

    fn name(s: &str) -> RigName {
        RigName::try_new(s).expect("rig")
    }

    fn registry() -> (HostRegistry, HostRig) {
        HostRegistry::default()
            .add(
                name("toy"),
                "git@x:y.git".into(),
                "rust".into(),
                "claude".into(),
                "main".into(),
                PathBuf::from("/root"),
            )
            .expect("added")
    }

    #[test]
    fn add_remove_and_ports() {
        let (reg, toy) = registry();
        assert_eq!(toy.console_port, 7700);
        assert_eq!(toy.project(), "factory-toy");
        assert_eq!(toy.volume("ledger"), "factory-toy_ledger");
        assert!(toy.compose_env().contains("RIG_IMAGE=factory-rig:rust\n"));
        assert!(
            toy.compose_env()
                .contains("RIG_ENV_FILE=/root/toy/rig.env\n")
        );
        assert_eq!(toy.env_file(), PathBuf::from("/root/toy/rig.env"));
        let (reg2, api) = reg
            .add(
                name("api"),
                "u".into(),
                "node".into(),
                "codex".into(),
                "main".into(),
                PathBuf::from("/root"),
            )
            .expect("added");
        assert_eq!(api.console_port, 7701);
        assert_eq!(
            reg2.add(
                name("toy"),
                "u".into(),
                "r".into(),
                "h".into(),
                "m".into(),
                PathBuf::from("/root")
            ),
            Err(RegistryError::Exists { name: name("toy") })
        );
        let (reg3, removed) = reg2.remove(&name("toy")).expect("removed");
        assert_eq!(removed.name, name("toy"));
        assert_eq!(reg3.rig.len(), 1);
        assert_eq!(
            reg3.remove(&name("toy")),
            Err(RegistryError::Unknown { name: name("toy") })
        );
        let full = HostRegistry {
            rig: (PORT_FROM..=PORT_TO)
                .map(|p| HostRig {
                    name: name(&format!("r{p}")),
                    repo_url: String::new(),
                    runtime: "rust".into(),
                    harness: "claude".into(),
                    main: "main".into(),
                    console_port: p,
                    root: PathBuf::from("/root"),
                })
                .collect(),
        };
        assert!(matches!(
            full.add(
                name("more"),
                "u".into(),
                "r".into(),
                "h".into(),
                "m".into(),
                PathBuf::from("/root")
            ),
            Err(RegistryError::NoPort { .. })
        ));
    }

    #[test]
    fn console_files_render_every_rig() {
        let (reg, _) = registry();
        let toml = reg.console_registry();
        assert!(toml.contains("ledger = \"/work/rigs/toy\""));
        let compose = reg.console_compose("factory-rig:base", 7700);
        assert!(compose.contains("factory-toy_ledger:/work/rigs/toy"));
        assert!(compose.contains("external: true"));
        assert!(compose.contains("127.0.0.1:7700:7700"));
        let (_, toy) = registry();
        assert_eq!(
            backup_path(
                Path::new("/b"),
                &toy,
                "ledger",
                Timestamp::from_unix_seconds(5)
            ),
            PathBuf::from("/b/toy-ledger-5.tgz")
        );
    }

    #[tokio::test]
    async fn doctor_and_backup_use_the_port() {
        let (reg, toy) = registry();
        let docker = FakeHostDocker::default();
        docker
            .volumes
            .lock()
            .await
            .insert("factory-toy_ledger".into());
        docker
            .ps
            .lock()
            .await
            .insert("factory-toy".into(), "steward\nworker\n".into());
        let health = doctor(&docker, &reg, Path::new("/root"), Path::new("/c.yaml"))
            .await
            .expect("ok");
        assert_eq!(health[0].running, vec!["steward", "worker"]);
        assert!(health[0].ledger_volume);
        let paths = backup(
            &docker,
            &toy,
            Path::new("/b"),
            Timestamp::from_unix_seconds(9),
        )
        .await
        .expect("ok");
        assert_eq!(paths.len(), 2);
        assert_eq!(docker.archived.lock().await.len(), 2);
        let broken = FakeHostDocker {
            fail: true,
            ..FakeHostDocker::default()
        };
        assert!(
            doctor(&broken, &reg, Path::new("/r"), Path::new("/c"))
                .await
                .is_err()
        );
        assert!(
            backup(
                &broken,
                &toy,
                Path::new("/b"),
                Timestamp::from_unix_seconds(9)
            )
            .await
            .is_err()
        );
        let calls = docker.calls.lock().await.clone();
        assert!(calls[0].contains("factory-toy") && calls[0].contains("ps"));
    }
}
