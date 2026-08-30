//! `ui-build` / `ui-check`: the web console's gate, run from the repo root with pnpm.
//! ui-check = install (frozen) → typecheck → lint → unit tests with coverage → e2e → build →
//! bundle budget. The Rust gate does not depend on it, but CI runs both.

use std::path::Path;
use std::process::Command;

const BUNDLE_BUDGET_GZ: u64 = 250 * 1024;

fn pnpm(root: &Path, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("pnpm")
        .args(args)
        .current_dir(root.join("crates/console/ui"))
        .status()
        .map_err(|e| anyhow::anyhow!("pnpm not found ({e}); install with `corepack enable`"))?;
    anyhow::ensure!(status.success(), "pnpm {} failed", args.join(" "));
    Ok(())
}

/// Build `crates/console/ui/dist`; the console binary embeds it on its next build.
pub(crate) fn build(root: &Path) -> anyhow::Result<()> {
    pnpm(root, &["install", "--frozen-lockfile"])?;
    pnpm(root, &["build"])?;
    budget(root)
}

/// The full frontend gate.
pub(crate) fn check(root: &Path, e2e: bool) -> anyhow::Result<()> {
    pnpm(root, &["install", "--frozen-lockfile"])?;
    pnpm(root, &["typecheck"])?;
    pnpm(root, &["lint"])?;
    pnpm(root, &["test:cov"])?;
    pnpm(root, &["build"])?;
    budget(root)?;
    if e2e {
        pnpm(root, &["e2e"])?;
    }
    println!("ui-check: ok");
    Ok(())
}

/// Sum of gzipped JS + CSS under dist/assets must stay under the budget.
fn budget(root: &Path) -> anyhow::Result<()> {
    let assets = root.join("crates/console/ui/dist/assets");
    let mut total = 0u64;
    for entry in std::fs::read_dir(&assets)? {
        let p = entry?.path();
        let is_code = p.extension().is_some_and(|e| e == "js" || e == "css");
        if is_code {
            total += gzipped_len(&std::fs::read(&p)?)?;
        }
    }
    println!(
        "ui bundle: {} kB gzipped (budget {} kB)",
        total / 1024,
        BUNDLE_BUDGET_GZ / 1024
    );
    anyhow::ensure!(total <= BUNDLE_BUDGET_GZ, "ui bundle over budget");
    Ok(())
}

fn gzipped_len(bytes: &[u8]) -> anyhow::Result<u64> {
    let mut child = Command::new("gzip")
        .args(["-9", "-c"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        std::io::Write::write_all(&mut stdin, bytes)?;
    }
    let out = child.wait_with_output()?;
    Ok(u64::try_from(out.stdout.len())?)
}
