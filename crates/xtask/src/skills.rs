//! `skills --check`: the pinned `skills/rust-fp-skill` submodule vs upstream `main`.
//! Drift is reported, never applied: a skill bump can change what `lint-fp` demands.

use std::path::Path;

pub(crate) fn check(root: &Path) -> anyhow::Result<()> {
    let sub = root.join("skills/rust-fp-skill");
    anyhow::ensure!(
        sub.join("SKILL.md").exists(),
        "skills/rust-fp-skill is empty. Run `git submodule update --init`."
    );
    let pinned = git(&sub, &["rev-parse", "HEAD"])?;
    let upstream = git(&sub, &["ls-remote", "origin", "refs/heads/main"])?;
    let upstream = upstream
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
    for (name, target) in [
        (".claude", root.join(".claude/skills/rust-fp-skill")),
        (".codex", root.join(".codex/skills/rust-fp-skill")),
        (".opencode", root.join(".opencode/skills/rust-fp-skill")),
    ] {
        anyhow::ensure!(
            target.join("SKILL.md").exists(),
            "{name}/skills/rust-fp-skill does not resolve. Recreate the symlink: ln -sfn ../../skills/rust-fp-skill {name}/skills/rust-fp-skill"
        );
    }
    if pinned == upstream {
        println!(
            "skills: rust-fp-skill pinned at {} (up to date)",
            &pinned[..12]
        );
        Ok(())
    } else {
        anyhow::bail!(
            "skills: rust-fp-skill pinned at {} but upstream main is {}. Review the diff, then bump deliberately: `git -C skills/rust-fp-skill checkout {} && git add skills/rust-fp-skill` and re-run lint-fp.",
            &pinned[..12],
            &upstream[..12.min(upstream.len())],
            &upstream[..12.min(upstream.len())]
        )
    }
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()?;
    anyhow::ensure!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
