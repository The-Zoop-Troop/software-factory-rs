//! `Runner` over `sh -c` with a hard timeout. Output is captured, not streamed.

use std::path::Path;
use std::process::Stdio;

use app::domain::Duration;
use app::{RunError, RunOutput, Runner, Unavailable};
use async_trait::async_trait;

use crate::child_env::ChildEnv;
use crate::confine::Sandbox;

/// Runs commands through `/bin/sh -c`. Kills the process group on timeout. The commands are the
/// candidate's own code, so they start without the role's secrets and, when configured, confined.
#[derive(Debug, Clone, Default)]
pub struct ShellRunner {
    sandbox: Sandbox,
}

impl ShellRunner {
    /// This process's environment without its secrets, unconfined.
    #[must_use]
    pub fn from_process() -> Self {
        Self::with_sandbox(Sandbox {
            env: ChildEnv::from_process(&[]),
            confine: None,
        })
    }

    #[must_use]
    pub const fn with_sandbox(sandbox: Sandbox) -> Self {
        Self { sandbox }
    }
}

#[async_trait]
impl Runner for ShellRunner {
    async fn run(
        &self,
        cwd: &Path,
        command: &str,
        timeout: Duration,
    ) -> Result<RunOutput, RunError> {
        let path = path_with_worktree(self.sandbox.env.get("PATH"), cwd);
        let child = self
            .sandbox
            .command(Path::new("/bin/sh"), cwd)
            .env("PATH", path)
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| RunError {
                command: command.to_owned(),
                cause: crate::classify_io(e.kind()),
                detail: e.to_string(),
            })?;

        let wait = child.wait_with_output();
        let limit = std::time::Duration::from_secs(timeout.seconds());
        match tokio::time::timeout(limit, wait).await {
            Ok(Ok(out)) => Ok(RunOutput {
                exit_code: out.status.code(),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                timed_out: false,
            }),
            Ok(Err(e)) => Err(RunError {
                command: command.to_owned(),
                cause: Unavailable::Io,
                detail: e.to_string(),
            }),
            // The future (and the child, via kill_on_drop) is dropped here.
            Err(_elapsed) => Ok(RunOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: format!("timed out after {}s", timeout.seconds()),
                timed_out: true,
            }),
        }
    }
}

/// Pure: the worktree goes last on `PATH`. POSIX `.` searches `PATH`, not the current directory,
/// so `. lib.sh` still resolves inside the worktree, but a committed `cargo`, `npm` or `sh` can
/// no longer replace the real one: the candidate under test must not choose what checks it.
fn path_with_worktree(path: Option<&str>, cwd: &Path) -> String {
    let worktree = cwd.to_string_lossy();
    match path {
        Some(p) if !p.is_empty() => format!("{p}:{worktree}"),
        _ => worktree.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_goes_last_on_path() {
        let cwd = Path::new("/work/wt");
        assert_eq!(
            path_with_worktree(Some("/usr/bin:/bin"), cwd),
            "/usr/bin:/bin:/work/wt"
        );
        assert_eq!(path_with_worktree(None, cwd), "/work/wt");
        assert_eq!(path_with_worktree(Some(""), cwd), "/work/wt");
    }

    #[tokio::test]
    async fn a_committed_tool_cannot_replace_a_real_one() {
        let dir = std::env::temp_dir().join(format!("factory-sh-shadow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("ls");
        std::fs::write(&fake, "#!/bin/sh\necho hijacked\n").unwrap();
        std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let out = ShellRunner::from_process()
            .run(&dir, "ls /", Duration::from_seconds(5))
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(out.succeeded(), "{}", out.stderr);
        assert!(!out.stdout.contains("hijacked"), "{}", out.stdout);
    }

    #[tokio::test]
    async fn captures_exit_and_output() {
        let out = ShellRunner::from_process()
            .run(
                Path::new("/"),
                "echo hi; echo err >&2; exit 3",
                Duration::from_seconds(5),
            )
            .await
            .unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.stdout, "hi\n");
        assert_eq!(out.stderr, "err\n");
        assert!(!out.succeeded());
    }

    #[tokio::test]
    async fn cwd_is_on_path_so_dot_source_works() {
        let dir = std::env::temp_dir().join(format!("factory-sh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lib.sh"), "f() { echo ok; }\n").unwrap();
        let out = ShellRunner::from_process()
            .run(&dir, ". lib.sh && f", Duration::from_seconds(5))
            .await
            .unwrap();
        assert!(out.succeeded(), "{}", out.stderr);
        assert_eq!(out.stdout, "ok\n");
    }

    #[tokio::test]
    async fn verify_commands_start_without_the_roles_secrets() {
        let env = ChildEnv::from_vars(
            [
                ("PATH", "/usr/bin:/bin"),
                ("BEADS_DOLT_PASSWORD", "ledger"),
                ("RIG_GIT_TOKEN", "git"),
                ("DATABASE_URL", "postgresql://postgres:5432/db"),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned())),
            &[],
        );
        let out = ShellRunner::with_sandbox(Sandbox { env, confine: None })
            .run(
                Path::new("/"),
                "env | cut -d= -f1",
                Duration::from_seconds(5),
            )
            .await
            .unwrap();
        let names: Vec<&str> = out.stdout.lines().collect();
        assert!(names.contains(&"DATABASE_URL"), "{names:?}");
        assert!(!names.contains(&"BEADS_DOLT_PASSWORD"), "{names:?}");
        assert!(!names.contains(&"RIG_GIT_TOKEN"), "{names:?}");
    }

    #[tokio::test]
    async fn times_out() {
        let out = ShellRunner::from_process()
            .run(Path::new("/"), "sleep 5", Duration::from_seconds(1))
            .await
            .unwrap();
        assert!(out.timed_out);
        assert!(!out.succeeded());
    }
}
