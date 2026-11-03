//! Confinement for children that run untrusted work: a harness session (an agent with a shell)
//! and the Verifier's commands (the candidate's own code). Each runs under Landlock, which lets
//! an unprivileged process restrict its own writes, inherited by every descendant. The child may
//! write its worktree, the refs of its own branch, its home, `/tmp`, `/dev` and the shared cache;
//! the ledger, the event log, other worktrees and the integration branch stay out of reach.
//!
//! The role wraps the child as `factory confine --write <path>... -- <program> <args>`: the helper
//! restricts itself, then execs the program, so the role itself stays unconfined.

use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::child_env::ChildEnv;

/// Set to `off` to run sessions and verify commands unconfined, on a host without Landlock.
pub const CONFINE_SWITCH: &str = "RIG_CONFINE";
/// Extra writable directories, colon-separated, for a project whose tools write elsewhere.
pub const EXTRA_WRITE: &str = "RIG_CONFINE_WRITE";
/// The rig's shared package cache (`compose.yaml` mounts it here).
pub const CACHE_DIR: &str = "/work/cache";

/// How a role starts an untrusted child: with what environment, and under what confinement.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sandbox {
    pub env: ChildEnv,
    pub confine: Option<Confine>,
}

impl Sandbox {
    /// A command for `program` in `cwd`: confined when configured, with exactly `env`.
    /// Arguments added afterwards go to `program`.
    #[must_use]
    pub fn command(&self, program: &Path, cwd: &Path) -> Command {
        self.command_writing(program, cwd, &[])
    }

    /// As `command`, with `extra` writable too: a harness's own scratch files outside `cwd`'s
    /// writable set, such as Codex's output file in the main checkout when it plans.
    #[must_use]
    pub fn command_writing(&self, program: &Path, cwd: &Path, extra: &[PathBuf]) -> Command {
        let mut cmd = match &self.confine {
            None => Command::new(program),
            Some(c) => {
                let mut cmd = Command::new(&c.helper);
                cmd.arg("confine");
                for dir in c.writable_for(cwd).iter().chain(extra) {
                    cmd.arg("--write").arg(dir);
                }
                cmd.arg("--").arg(program);
                cmd
            }
        };
        self.env.apply(&mut cmd);
        cmd.current_dir(cwd);
        cmd
    }
}

/// What a confined child may write, besides the directory it runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confine {
    /// The `factory` binary that applies the restriction and execs the child.
    pub helper: PathBuf,
    /// Writable everywhere this role confines: home, temp, devices, cache.
    pub writable: Vec<PathBuf>,
}

impl Confine {
    /// The rig's layout, read from `env`: `$HOME`, `$TMPDIR` or `/tmp`, `/dev`, the cache, and
    /// anything `RIG_CONFINE_WRITE` adds. `None` when `RIG_CONFINE=off`.
    #[must_use]
    pub fn from_env(helper: PathBuf, env: &ChildEnv) -> Option<Self> {
        if env.get(CONFINE_SWITCH) == Some("off") {
            return None;
        }
        let mut writable: Vec<PathBuf> = [
            env.get("HOME"),
            Some(env.get("TMPDIR").unwrap_or("/tmp")),
            Some("/dev"),
            Some(CACHE_DIR),
        ]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .collect();
        writable.extend(
            env.get(EXTRA_WRITE)
                .unwrap_or_default()
                .split(':')
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
        );
        Some(Self { helper, writable })
    }

    /// Everything a child in `cwd` may write: the fixed list, `cwd`, and when `cwd` is a git
    /// worktree, what committing to its own branch touches. Never the main checkout: its `.git`
    /// directory holds the integration branch, so a child there (the Planner) writes nothing in it.
    #[must_use]
    pub fn writable_for(&self, cwd: &Path) -> Vec<PathBuf> {
        let mut dirs = self.writable.clone();
        if !cwd.join(".git").is_dir() {
            dirs.push(cwd.to_path_buf());
            dirs.extend(worktree_git_dirs(cwd));
        }
        dirs
    }
}

/// Pure over the file system: the git directories a commit in the worktree at `cwd` writes.
/// The worktree's own metadata, the object store, and the ref directories of its branch: for
/// `task/x` that is `refs/heads/task`, so other task branches are writable too, which is harmless
/// because the Verifier and Integrator check out the recorded commit, never a branch name.
/// The integration branch's ref, `packed-refs` and the config stay read-only.
#[must_use]
pub fn worktree_git_dirs(cwd: &Path) -> Vec<PathBuf> {
    let Some(gitdir) = std::fs::read_to_string(cwd.join(".git"))
        .ok()
        .and_then(|t| t.strip_prefix("gitdir:").map(|g| PathBuf::from(g.trim())))
    else {
        return Vec::new();
    };
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        cwd.join(gitdir)
    };
    let common = std::fs::read_to_string(gitdir.join("commondir"))
        .ok()
        .map_or_else(|| gitdir.clone(), |c| gitdir.join(c.trim()));
    let mut dirs = vec![gitdir.clone(), common.join("objects")];
    if let Some(branch) = std::fs::read_to_string(gitdir.join("HEAD"))
        .ok()
        .and_then(|h| h.trim().strip_prefix("ref: refs/heads/").map(str::to_owned))
    {
        let parent = Path::new(&branch)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf);
        if let Some(parent) = parent {
            dirs.push(common.join("refs/heads").join(&parent));
            dirs.push(common.join("logs/refs/heads").join(&parent));
        }
    }
    dirs
}

/// Why the restriction could not be applied.
#[derive(Debug, thiserror::Error)]
pub enum ConfineError {
    #[error(
        "this kernel cannot confine sessions (Landlock {0}); set RIG_CONFINE=off to run them unconfined"
    )]
    Unsupported(&'static str),
    #[error("Landlock refused the restriction: {detail}")]
    Refused { detail: String },
}

/// Restrict this thread, and everything it execs, to writing only beneath `writable`.
/// Reads and execution stay open. Requires Landlock ABI 3 or later, so truncating a file needs
/// write access too; later ABIs add device ioctls when the kernel has them.
///
/// # Errors
/// `Unsupported` when the kernel cannot enforce it; `Refused` on any other Landlock error.
#[cfg(target_os = "linux")]
pub fn confine_self(writable: &[PathBuf]) -> Result<(), ConfineError> {
    use landlock::{
        ABI, AccessFs, CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetCreatedAttr,
        RulesetStatus, path_beneath_rules,
    };
    let refused = |e: landlock::RulesetError| ConfineError::Refused {
        detail: e.to_string(),
    };
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(AccessFs::from_write(ABI::V3))
        .map_err(|_| ConfineError::Unsupported("ABI 3 or later is required"))?
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_write(ABI::V5))
        .map_err(refused)?
        .create()
        .map_err(refused)?
        .add_rules(path_beneath_rules(writable, AccessFs::from_write(ABI::V5)))
        .map_err(refused)?
        .restrict_self()
        .map_err(refused)?;
    match status.ruleset {
        RulesetStatus::FullyEnforced | RulesetStatus::PartiallyEnforced => Ok(()),
        RulesetStatus::NotEnforced => Err(ConfineError::Unsupported("not enforced")),
    }
}

/// Prove at role startup that `confine` works here, so a host without Landlock fails once with a
/// clear message rather than failing every session.
///
/// # Errors
/// `Unsupported` or `Refused` with the helper's own message.
pub async fn probe(confine: &Confine) -> Result<(), ConfineError> {
    let out = Command::new(&confine.helper)
        .args(["confine", "--write", "/tmp", "--", "/bin/true"])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| ConfineError::Refused {
            detail: format!("{}: {e}", confine.helper.display()),
        })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(ConfineError::Refused {
            detail: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        })
    }
}

/// Outside Linux there is no Landlock.
///
/// # Errors
/// Always `Unsupported`.
#[cfg(not(target_os = "linux"))]
pub fn confine_self(_writable: &[PathBuf]) -> Result<(), ConfineError> {
    Err(ConfineError::Unsupported("not Linux"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> ChildEnv {
        ChildEnv::from_vars(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
            &[],
        )
    }

    #[test]
    fn layout_comes_from_the_environment_and_off_disables_it() {
        let c = Confine::from_env(
            "/usr/local/bin/factory".into(),
            &env(&[("HOME", "/home/factory"), (EXTRA_WRITE, "/opt/a::/opt/b")]),
        )
        .unwrap();
        assert_eq!(
            c.writable,
            [
                "/home/factory",
                "/tmp",
                "/dev",
                CACHE_DIR,
                "/opt/a",
                "/opt/b"
            ]
            .map(PathBuf::from)
        );
        assert_eq!(
            Confine::from_env("f".into(), &env(&[(CONFINE_SWITCH, "off")])),
            None
        );
    }

    #[test]
    fn a_worktree_may_write_its_own_branch_refs_and_metadata_only() {
        let root = std::env::temp_dir().join(format!("factory-confine-git-{}", std::process::id()));
        let common = root.join("repo/.git");
        let gitdir = common.join("worktrees/task__x");
        let wt = root.join("wt");
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/task/x\n").unwrap();
        let dirs = worktree_git_dirs(&wt);
        let common = gitdir.join("../..");
        assert_eq!(
            dirs,
            [
                gitdir.clone(),
                common.join("objects"),
                common.join("refs/heads/task"),
                common.join("logs/refs/heads/task"),
            ]
        );
        assert!(worktree_git_dirs(&root).is_empty(), "not a worktree");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_confined_command_wraps_the_program_and_carries_only_the_scrubbed_env() {
        let sandbox = Sandbox {
            env: env(&[("PATH", "/bin"), ("BEADS_DOLT_PASSWORD", "x")]),
            confine: Some(Confine {
                helper: "/usr/local/bin/factory".into(),
                writable: vec!["/tmp".into()],
            }),
        };
        let cmd = sandbox.command(Path::new("claude"), Path::new("/work/wt"));
        let std = cmd.as_std();
        assert_eq!(std.get_program(), "/usr/local/bin/factory");
        let args: Vec<_> = std
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "confine", "--write", "/tmp", "--write", "/work/wt", "--", "claude"
            ]
        );
        let envs: Vec<_> = std
            .get_envs()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert_eq!(envs, ["PATH"]);
        let plain = Sandbox::default().command(Path::new("claude"), Path::new("/w"));
        assert_eq!(plain.as_std().get_program(), "claude");
    }

    #[test]
    fn the_main_checkout_is_never_writable() {
        let main =
            std::env::temp_dir().join(format!("factory-confine-main-{}", std::process::id()));
        std::fs::create_dir_all(main.join(".git/refs/heads")).unwrap();
        let c = Confine {
            helper: "f".into(),
            writable: vec!["/tmp".into()],
        };
        let dirs = c.writable_for(&main);
        std::fs::remove_dir_all(&main).unwrap();
        assert_eq!(dirs, [PathBuf::from("/tmp")]);
    }
}
