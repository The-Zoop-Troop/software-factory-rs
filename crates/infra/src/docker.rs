//! The host's docker CLI behind `app::HostDocker`.

use std::path::{Path, PathBuf};

use app::{HostDocker, HostError};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct DockerCli {
    bin: PathBuf,
    /// Image used for the tar helper container.
    helper: String,
}

impl Default for DockerCli {
    fn default() -> Self {
        Self {
            bin: PathBuf::from("docker"),
            helper: "alpine:3".to_owned(),
        }
    }
}

impl DockerCli {
    #[must_use]
    pub fn with_bin(self, bin: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            ..self
        }
    }

    async fn run(&self, what: &str, args: &[String]) -> Result<String, HostError> {
        let out = tokio::process::Command::new(&self.bin)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| HostError::Missing {
                detail: format!("{}: {e}", self.bin.display()),
            })?;
        if !out.status.success() {
            return Err(HostError::Command {
                what: what.to_owned(),
                detail: String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .last()
                    .unwrap_or("exit status non-zero")
                    .to_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn tar_args(&self, volume: &str, host_dir: &Path, script: &str) -> Vec<String> {
        vec![
            "run".into(),
            "--rm".into(),
            "-v".into(),
            format!("{volume}:/v"),
            "-v".into(),
            format!("{}:/b", host_dir.display()),
            self.helper.clone(),
            "sh".into(),
            "-c".into(),
            script.to_owned(),
        ]
    }
}

fn split_dest(path: &Path) -> Result<(PathBuf, String), HostError> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| HostError::Command {
            what: "archive".to_owned(),
            detail: format!("{} has no file name", path.display()),
        })?;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    Ok((dir, name))
}

#[async_trait]
impl HostDocker for DockerCli {
    async fn compose(
        &self,
        project: &str,
        env_file: &Path,
        compose_file: &Path,
        args: &[&str],
    ) -> Result<String, HostError> {
        let mut full = vec![
            "compose".to_owned(),
            "-p".to_owned(),
            project.to_owned(),
            "--env-file".to_owned(),
            env_file.display().to_string(),
            "-f".to_owned(),
            compose_file.display().to_string(),
        ];
        full.extend(args.iter().map(|a| (*a).to_owned()));
        self.run("compose", &full).await
    }

    async fn volume_exists(&self, name: &str) -> Result<bool, HostError> {
        let out = self
            .run("volume ls", &["volume", "ls", "-q"].map(str::to_owned))
            .await?;
        Ok(out.lines().any(|l| l.trim() == name))
    }

    async fn archive_volume(&self, volume: &str, dest: &Path) -> Result<(), HostError> {
        let (dir, name) = split_dest(dest)?;
        std::fs::create_dir_all(&dir).map_err(|e| HostError::Command {
            what: "archive".to_owned(),
            detail: e.to_string(),
        })?;
        let dir = dir.canonicalize().unwrap_or(dir);
        self.run(
            "archive",
            &self.tar_args(volume, &dir, &format!("tar czf /b/{name} -C /v .")),
        )
        .await
        .map(|_| ())
    }

    async fn restore_volume(&self, volume: &str, src: &Path) -> Result<(), HostError> {
        let (dir, name) = split_dest(src)?;
        let dir = dir.canonicalize().unwrap_or(dir);
        self.run(
            "restore",
            &self.tar_args(
                volume,
                &dir,
                &format!("rm -rf /v/* /v/.[!.]* 2>/dev/null; tar xzf /b/{name} -C /v"),
            ),
        )
        .await
        .map(|_| ())
    }
}
