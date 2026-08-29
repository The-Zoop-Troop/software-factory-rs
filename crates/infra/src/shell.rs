//! `Runner` over `sh -c` with a hard timeout. Output is captured, not streamed.

use std::path::Path;
use std::process::Stdio;

use app::domain::Duration;
use app::{RunError, RunOutput, Runner, Unavailable};
use async_trait::async_trait;
use tokio::process::Command;

/// Runs commands through `/bin/sh -c`. Kills the process group on timeout.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShellRunner;

#[async_trait]
impl Runner for ShellRunner {
    async fn run(
        &self,
        cwd: &Path,
        command: &str,
        timeout: Duration,
    ) -> Result<RunOutput, RunError> {
        // Verify commands run from the repo root; putting it first on PATH makes `. lib.sh`
        // (POSIX `.` searches PATH, not cwd) and `./tool`-less invocations resolve inside the
        // worktree. The worktree is the sandbox, so this widens nothing that matters.
        let path = std::env::var("PATH").map_or_else(
            |_| cwd.to_string_lossy().into_owned(),
            |p| format!("{}:{p}", cwd.to_string_lossy()),
        );
        let child = Command::new("/bin/sh")
            .env("PATH", path)
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_exit_and_output() {
        let out = ShellRunner
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
        let out = ShellRunner
            .run(&dir, ". lib.sh && f", Duration::from_seconds(5))
            .await
            .unwrap();
        assert!(out.succeeded(), "{}", out.stderr);
        assert_eq!(out.stdout, "ok\n");
    }

    #[tokio::test]
    async fn times_out() {
        let out = ShellRunner
            .run(Path::new("/"), "sleep 5", Duration::from_seconds(1))
            .await
            .unwrap();
        assert!(out.timed_out);
        assert!(!out.succeeded());
    }
}
