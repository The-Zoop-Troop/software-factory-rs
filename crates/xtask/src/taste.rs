//! `lint-taste`: the golden principles that are cheap to check mechanically.
//!
//! - crate layering: `domain` depends on no workspace crate; `app` only on `domain`; `infra` never
//!   appears in `app`'s dependencies; binaries are the only crates depending on `infra`;
//! - source files ≤ 600 lines;
//! - no `println!`/`eprintln!` outside binaries and `xtask`;
//! - no `pkill -f` in any script or Dockerfile.

use std::path::{Path, PathBuf};

const MAX_LINES: usize = 600;

pub(crate) fn lint(root: &Path) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    layering(root, &mut errors)?;
    for file in rust_files(&root.join("crates"))? {
        let text = std::fs::read_to_string(&file)?;
        let lines = text.lines().count();
        if lines > MAX_LINES {
            errors.push(format!("{} has {lines} lines (max {MAX_LINES}). Split it by concern (e.g. tests into a sibling module) so it fits an agent's context.", rel(root, &file)));
        }
        let path = file.to_string_lossy();
        // Binary crates and integration tests may print; library crates must trace.
        let is_bin = [
            "/crates/factory/",
            "/crates/stewardd/",
            "/crates/console/",
            "/crates/xtask/",
        ]
        .iter()
        .any(|c| path.contains(c))
            || path.contains("/tests/");
        if !is_bin && (text.contains("println!(") || text.contains("eprintln!(")) {
            errors.push(format!("{}: println!/eprintln! outside a binary. Use `tracing` with structured fields so the output is queryable.", rel(root, &file)));
        }
    }
    for file in scripts(root)? {
        let text = std::fs::read_to_string(&file)?;
        if text.contains("pkill -f") {
            errors.push(format!("{}: `pkill -f` matches the calling shell's own command line. Track the PID and `kill` it instead.", rel(root, &file)));
        }
    }
    if errors.is_empty() {
        println!("lint-taste: ok");
        Ok(())
    } else {
        anyhow::bail!(
            "lint-taste: {} problem(s)\n  - {}",
            errors.len(),
            errors.join("\n  - ")
        )
    }
}

fn layering(root: &Path, errors: &mut Vec<String>) -> anyhow::Result<()> {
    let deps = |krate: &str| -> anyhow::Result<Vec<String>> {
        let manifest: toml::Value = toml::from_str(&std::fs::read_to_string(
            root.join("crates").join(krate).join("Cargo.toml"),
        )?)?;
        Ok(manifest
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default())
    };
    let workspace = [
        "domain", "app", "infra", "factory", "stewardd", "console", "xtask",
    ];
    let rule = |krate: &str, allowed: &[&str], errors: &mut Vec<String>| -> anyhow::Result<()> {
        for d in deps(krate)? {
            if workspace.contains(&d.as_str()) && !allowed.contains(&d.as_str()) {
                errors.push(format!("crates/{krate} depends on `{d}`, which the layering forbids (allowed: {allowed:?}). Move the code behind a port in `app` or into the right layer."));
            }
        }
        Ok(())
    };
    rule("domain", &[], errors)?;
    rule("app", &["domain"], errors)?;
    rule("infra", &["app", "domain"], errors)?;
    rule("console", &["app", "domain", "infra"], errors)?;
    Ok(())
}

fn rust_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            out.extend(rust_files(&p)?);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
    Ok(out)
}

fn scripts(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root.join("docker"))? {
        let p = entry?.path();
        if p.is_file() {
            out.push(p);
        }
    }
    Ok(out)
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}
