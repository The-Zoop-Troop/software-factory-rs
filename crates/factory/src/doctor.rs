//! `factory doctor`: is this rig (or host) able to run the factory? Each check is a fact with
//! a remediation, so an agent can act on the output without a human.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

/// One health check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
    pub fix: &'static str,
}

fn version(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned()
    })
}

fn tool(name: &'static str, bin: &str, args: &[&str], fix: &'static str) -> Check {
    match version(bin, args) {
        Some(v) => Check {
            name,
            ok: true,
            detail: v,
            fix,
        },
        None => Check {
            name,
            ok: false,
            detail: format!("`{bin}` not runnable"),
            fix,
        },
    }
}

/// Run every check. `workdir` holds `.beads`; `repo` is the project clone.
pub(crate) fn run_checks(workdir: &Path, repo: &Path) -> Vec<Check> {
    let mut checks = vec![
        tool(
            "bd",
            "bd",
            &["version"],
            "install beads: https://github.com/gastownhall/beads (pinned in docker/Dockerfile.rig)",
        ),
        tool("git", "git", &["--version"], "install git"),
        tool(
            "claude",
            "claude",
            &["--version"],
            "harness optional: install Claude Code or use --harness opencode|codex",
        ),
        tool(
            "opencode",
            "opencode",
            &["--version"],
            "harness optional: install OpenCode or use --harness claude|codex",
        ),
        tool(
            "codex",
            "codex",
            &["--version"],
            "harness optional: install Codex CLI or use --harness claude|opencode",
        ),
    ];
    checks.push(Check {
        name: "ledger",
        ok: workdir.join(".beads").is_dir(),
        detail: workdir.join(".beads").display().to_string(),
        fix: "run `bd init --prefix <p>` in --workdir (the rig entrypoint does this)",
    });
    checks.push(Check {
        name: "repo",
        ok: repo.join(".git").exists(),
        detail: repo.display().to_string(),
        fix: "clone the project at --repo, or set RIG_REPO_URL for the rig",
    });
    let creds = [
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "OPENCODE_API_KEY",
        "OPENAI_API_KEY",
    ]
    .into_iter()
    .filter(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
    .collect::<Vec<_>>();
    checks.push(Check {
        name: "credentials",
        ok: !creds.is_empty(),
        detail: if creds.is_empty() {
            "none set".into()
        } else {
            creds.join(", ")
        },
        fix: "put one harness credential in docker/rig.env (see docs/SECURITY.md)",
    });
    checks
}

/// Human/agent-readable table; returns whether every check passed.
pub(crate) fn render(checks: &[Check]) -> (String, bool) {
    let mut out = String::new();
    let mut all_ok = true;
    for c in checks {
        all_ok &= c.ok;
        let _ = writeln!(
            out,
            "{} {:<12} {}",
            if c.ok { "ok  " } else { "FAIL" },
            c.name,
            c.detail
        );
        if !c.ok {
            let _ = writeln!(out, "     fix: {}", c.fix);
        }
    }
    (out, all_ok)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn checks_report_missing_ledger_and_repo_with_fixes() {
        let dir = std::env::temp_dir().join(format!("factory-doctor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let checks = run_checks(&dir, &dir.join("repo"));
        let ledger = checks.iter().find(|c| c.name == "ledger").unwrap();
        assert!(!ledger.ok);
        let (text, all_ok) = render(&checks);
        assert!(!all_ok);
        assert!(text.contains("FAIL ledger") && text.contains("fix: run `bd init"));
        assert!(text.contains("git"));
    }

    #[test]
    fn tool_check_handles_missing_binary() {
        let c = tool("x", "/nonexistent/x", &["--version"], "install x");
        assert!(!c.ok && c.detail.contains("not runnable"));
    }
}
