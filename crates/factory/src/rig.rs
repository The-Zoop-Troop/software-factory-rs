//! `factory rig …`: many rigs on one host. Each rig is a compose project (`factory-<name>`)
//! driven from the shared `compose.yaml` with its own env, secrets, and volumes; one console
//! (`factory rig console`) fronts all of them. Files live under `--root` (default `~/.factory`).

use std::path::{Path, PathBuf};

use app::domain::{RigName, Timestamp};
use app::{HostDocker, HostError, HostRegistry, HostRig, RegistryError};
use clap::Subcommand;

const TOKENS_EXAMPLE: &str = include_str!("../../../docker/console/tokens.toml.example");
const CONSOLE_PROJECT: &str = "factory-console";

#[derive(Debug, thiserror::Error)]
pub(crate) enum RigCmdError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Docker(#[from] HostError),
    #[error("invalid rig name: {detail}")]
    Name { detail: String },
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("registry {path} is malformed: {detail}")]
    Malformed { path: PathBuf, detail: String },
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> RigCmdError + '_ {
    move |source| RigCmdError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Host-side layout under the root directory.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub root: PathBuf,
    /// The shared rig compose file (this repository's `compose.yaml`).
    pub compose_file: PathBuf,
}

impl Layout {
    fn registry_path(&self) -> PathBuf {
        self.root.join("rigs.toml")
    }
    fn rig_dir(&self, name: &RigName) -> PathBuf {
        self.root.join(name.as_ref())
    }
    fn console_dir(&self) -> PathBuf {
        self.root.join("console")
    }

    pub(crate) fn load(&self) -> Result<HostRegistry, RigCmdError> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(HostRegistry::default());
        }
        let text = std::fs::read_to_string(&path).map_err(io(&path))?;
        toml::from_str(&text).map_err(|e| RigCmdError::Malformed {
            path,
            detail: e.to_string(),
        })
    }

    fn save(&self, registry: &HostRegistry, image: &str) -> Result<(), RigCmdError> {
        std::fs::create_dir_all(&self.root).map_err(io(&self.root))?;
        let path = self.registry_path();
        let text = toml::to_string_pretty(registry).map_err(|e| RigCmdError::Malformed {
            path: path.clone(),
            detail: e.to_string(),
        })?;
        std::fs::write(&path, text).map_err(io(&path))?;
        // The console sees every rig's ledger; regenerate its files on each change.
        let console = self.console_dir();
        std::fs::create_dir_all(&console).map_err(io(&console))?;
        let write = |name: &str, body: String| -> Result<(), RigCmdError> {
            let p = console.join(name);
            std::fs::write(&p, body).map_err(io(&p))
        };
        write("rigs.toml", registry.console_registry())?;
        write("compose.yaml", registry.console_compose(image, 7700))?;
        write(
            "compose.env",
            format!("COMPOSE_PROJECT_NAME={CONSOLE_PROJECT}\n"),
        )?;
        let tokens = console.join("tokens.toml");
        if !tokens.exists() {
            std::fs::write(&tokens, TOKENS_EXAMPLE).map_err(io(&tokens))?;
        }
        Ok(())
    }
}

/// What `rig create` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateSpec {
    pub name: String,
    pub repo_url: String,
    pub runtime: String,
    pub harness: String,
    pub main: String,
    pub secrets: Option<PathBuf>,
    pub start: bool,
}

/// `rig create`: register, write env + secrets, bring the project up.
pub(crate) async fn create(
    docker: &dyn HostDocker,
    layout: &Layout,
    spec: &CreateSpec,
) -> Result<String, RigCmdError> {
    let CreateSpec {
        name,
        repo_url,
        runtime,
        harness,
        main,
        secrets,
        start,
    } = spec;
    let (secrets, start) = (secrets.as_deref(), *start);
    let name = RigName::try_new(name).map_err(|e| RigCmdError::Name {
        detail: e.to_string(),
    })?;
    let (registry, rig) = layout.load()?.add(
        name.clone(),
        repo_url.to_owned(),
        runtime.to_owned(),
        harness.to_owned(),
        main.to_owned(),
    )?;
    let dir = layout.rig_dir(&name);
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let env = dir.join("compose.env");
    std::fs::write(&env, rig.compose_env()).map_err(io(&env))?;
    let rig_env = dir.join("rig.env");
    match secrets {
        Some(src) => {
            std::fs::copy(src, &rig_env).map_err(io(src))?;
        }
        None if !rig_env.exists() => {
            std::fs::write(&rig_env, format!("# secrets for rig {name}; see docker/rig.env.example\nRIG_REPO_URL={repo_url}\n"))
                .map_err(io(&rig_env))?;
        }
        None => {}
    }
    layout.save(&registry, &format!("factory-rig:{runtime}"))?;
    let mut out = format!(
        "created rig {name}: project {}, console port {}, files in {}\n",
        rig.project(),
        rig.console_port,
        dir.display()
    );
    if start {
        docker
            .compose(&rig.project(), &env, &layout.compose_file, &["up", "-d"])
            .await?;
        out.push_str("started steward, verifier, integrator, worker, planner\n");
    }
    Ok(out)
}

/// `rig destroy`: bring the project down (optionally with volumes), forget it.
pub(crate) async fn destroy(
    docker: &dyn HostDocker,
    layout: &Layout,
    name: &str,
    volumes: bool,
) -> Result<String, RigCmdError> {
    let name = RigName::try_new(name).map_err(|e| RigCmdError::Name {
        detail: e.to_string(),
    })?;
    let (registry, rig) = layout.load()?.remove(&name)?;
    let dir = layout.rig_dir(&name);
    let env = dir.join("compose.env");
    let args: &[&str] = if volumes { &["down", "-v"] } else { &["down"] };
    docker
        .compose(&rig.project(), &env, &layout.compose_file, args)
        .await?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(io(&dir))?;
    }
    layout.save(&registry, "factory-rig:base")?;
    Ok(format!(
        "destroyed rig {name}{}\n",
        if volumes {
            " and its volumes"
        } else {
            " (volumes kept)"
        }
    ))
}

/// `rig list`.
pub(crate) fn list(layout: &Layout) -> Result<String, RigCmdError> {
    let registry = layout.load()?;
    if registry.rig.is_empty() {
        return Ok("no rigs; `factory rig create <name> --repo-url <url>`\n".to_owned());
    }
    Ok(registry.rig.iter().fold(String::new(), |mut s, r| {
        use core::fmt::Write as _;
        let _ = writeln!(
            s,
            "{}  {}  runtime={} harness={} console=127.0.0.1:{}",
            r.name, r.repo_url, r.runtime, r.harness, r.console_port
        );
        s
    }))
}

/// `rig doctor`: every rig's ledger volume and running services.
pub(crate) async fn doctor(
    docker: &dyn HostDocker,
    layout: &Layout,
) -> Result<String, RigCmdError> {
    let registry = layout.load()?;
    let health = app::rig_doctor(docker, &registry, &layout.root, &layout.compose_file).await?;
    Ok(health.iter().fold(String::new(), |mut s, h| {
        use core::fmt::Write as _;
        let _ = writeln!(
            s,
            "{}  {}  ledger={} running=[{}]",
            if h.ledger_volume && !h.running.is_empty() {
                "ok "
            } else {
                "!! "
            },
            h.name,
            if h.ledger_volume { "yes" } else { "missing" },
            h.running.join(", ")
        );
        s
    }))
}

fn find(layout: &Layout, name: &str) -> Result<HostRig, RigCmdError> {
    let name = RigName::try_new(name).map_err(|e| RigCmdError::Name {
        detail: e.to_string(),
    })?;
    layout
        .load()?
        .get(&name)
        .cloned()
        .ok_or(RigCmdError::Registry(RegistryError::Unknown { name }))
}

/// `rig backup`: ledger + repo tarballs into `dir`.
pub(crate) async fn backup(
    docker: &dyn HostDocker,
    layout: &Layout,
    name: &str,
    dir: &Path,
    at: Timestamp,
) -> Result<String, RigCmdError> {
    let rig = find(layout, name)?;
    let paths = app::backup(docker, &rig, dir, at).await?;
    Ok(paths.iter().fold(String::new(), |mut s, p| {
        s.push_str(&p.display().to_string());
        s.push('\n');
        s
    }))
}

/// `rig restore`: replace the ledger (and optionally repo) volume from tarballs. Stop the
/// rig first; the command refuses while any service runs.
pub(crate) async fn restore(
    docker: &dyn HostDocker,
    layout: &Layout,
    name: &str,
    ledger: &Path,
    repo: Option<&Path>,
) -> Result<String, RigCmdError> {
    let rig = find(layout, name)?;
    let env = layout.rig_dir(&rig.name).join("compose.env");
    let running = docker
        .compose(
            &rig.project(),
            &env,
            &layout.compose_file,
            &["ps", "--services", "--status", "running"],
        )
        .await?;
    if !running.trim().is_empty() {
        return Err(RigCmdError::Docker(HostError::Command {
            what: "restore".to_owned(),
            detail: format!(
                "rig {} is running ({}); `docker compose -p {} down` first",
                rig.name,
                running.trim().replace('\n', ", "),
                rig.project()
            ),
        }));
    }
    docker.restore_volume(&rig.volume("ledger"), ledger).await?;
    let mut out = format!(
        "restored {} from {}\n",
        rig.volume("ledger"),
        ledger.display()
    );
    if let Some(r) = repo {
        use core::fmt::Write as _;
        docker.restore_volume(&rig.volume("repo"), r).await?;
        let _ = writeln!(out, "restored {} from {}", rig.volume("repo"), r.display());
    }
    Ok(out)
}

/// `rig console`: bring the shared console up over every registered rig.
pub(crate) async fn console(
    docker: &dyn HostDocker,
    layout: &Layout,
    up: bool,
) -> Result<String, RigCmdError> {
    let registry = layout.load()?;
    layout.save(&registry, "factory-rig:base")?;
    let dir = layout.console_dir();
    let args: &[&str] = if up { &["up", "-d"] } else { &["down"] };
    docker
        .compose(
            CONSOLE_PROJECT,
            &dir.join("compose.env"),
            &dir.join("compose.yaml"),
            args,
        )
        .await?;
    Ok(format!(
        "console {} over {} rig(s); tokens in {}\n",
        if up { "up on 127.0.0.1:7700" } else { "down" },
        registry.rig.len(),
        dir.join("tokens.toml").display()
    ))
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub(crate) enum RigCommand {
    /// Register a rig, write its env + secrets, and start it.
    Create {
        name: String,
        /// Git URL the rig clones and pushes to.
        #[arg(long)]
        repo_url: String,
        /// Runtime image suffix (`factory-rig:<runtime>`).
        #[arg(long, default_value = "rust")]
        runtime: String,
        /// Harness for workers and the planner.
        #[arg(long, default_value = "claude")]
        harness: String,
        /// Integration branch.
        #[arg(long, default_value = "main")]
        main: String,
        /// Secrets file to copy as the rig's `rig.env` (see docker/rig.env.example).
        #[arg(long)]
        secrets: Option<PathBuf>,
        /// Register and write files only; do not `compose up`.
        #[arg(long)]
        no_start: bool,
    },
    /// Rigs on this host.
    List,
    /// Stop a rig and forget it; `--volumes` also deletes its ledger and repo.
    Destroy {
        name: String,
        #[arg(long)]
        volumes: bool,
    },
    /// Ledger volume and running services per rig.
    Doctor,
    /// Archive a rig's ledger and repo volumes into a directory.
    Backup {
        name: String,
        #[arg(long, default_value = "backups")]
        to: PathBuf,
    },
    /// Replace a stopped rig's ledger (and optionally repo) volume from `rig backup` tarballs.
    Restore {
        name: String,
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Bring the shared console up (or down) over every registered rig.
    Console {
        #[arg(long)]
        down: bool,
    },
}

/// Run one `factory rig` subcommand and return what to print.
pub(crate) async fn run(
    docker: &dyn HostDocker,
    layout: &Layout,
    command: RigCommand,
) -> Result<String, RigCmdError> {
    let out = match command {
        RigCommand::Create {
            name,
            repo_url,
            runtime,
            harness,
            main,
            secrets,
            no_start,
        } => {
            create(
                docker,
                layout,
                &CreateSpec {
                    name,
                    repo_url,
                    runtime,
                    harness,
                    main,
                    secrets,
                    start: !no_start,
                },
            )
            .await?
        }
        RigCommand::List => list(layout)?,
        RigCommand::Destroy { name, volumes } => destroy(docker, layout, &name, volumes).await?,
        RigCommand::Doctor => doctor(docker, layout).await?,
        RigCommand::Backup { name, to } => {
            backup(
                docker,
                layout,
                &name,
                &to,
                app::Clock::now(&infra::SystemClock),
            )
            .await?
        }
        RigCommand::Restore { name, ledger, repo } => {
            restore(docker, layout, &name, &ledger, repo.as_deref()).await?
        }
        RigCommand::Console { down } => console(docker, layout, !down).await?,
    };
    Ok(out)
}
