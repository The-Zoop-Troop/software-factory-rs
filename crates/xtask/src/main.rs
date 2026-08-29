//! `cargo xtask <task>` — the repository's own lints. Every failure message says how to fix it,
//! because the reader is usually an agent.
#![forbid(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::print_stdout,
    clippy::print_stderr
)]

mod docs;
mod taste;

use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let root = repo_root();
    let result = match task.as_str() {
        "lint-docs" => docs::lint(&root),
        "lint-taste" => taste::lint(&root),
        "coverage" => coverage(),
        _ => Err(anyhow::anyhow!(
            "usage: cargo xtask <lint-docs|lint-taste|coverage>"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(std::path::Path::to_path_buf)
        .expect("invariant: crates/xtask is two levels below the root")
}

/// Line coverage with the gate from `AGENTS.md` (85%).
fn coverage() -> anyhow::Result<()> {
    let status = std::process::Command::new("cargo")
        .args([
            "llvm-cov",
            "nextest",
            "--workspace",
            "--exclude",
            "xtask",
            "--all-features",
            "--fail-under-lines",
            "85",
            "--summary-only",
        ])
        .status()?;
    anyhow::ensure!(
        status.success(),
        "coverage below 85% — add tests for the uncovered lines (see `cargo llvm-cov --html`), or move true I/O shims behind #[cfg_attr(coverage, coverage(off))] with a justification"
    );
    Ok(())
}
