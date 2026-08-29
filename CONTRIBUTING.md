# Contributing

Humans steer; agents execute; verification decides. Contributions from people and from agents
follow the same path.

1. **Orient.** Read [`AGENTS.md`](AGENTS.md) (the map), then the docs it points to for your area.
   Run `bd prime` and `bd ready` — work is tracked in Beads, not in issues or TODO files. File a
   bead before writing code; link discovered work with `--deps discovered-from:<id>`.
2. **Follow the standard.** Rust is written railway-oriented per
   [`skills/rust-fp-skill/SKILL.md`](skills/rust-fp-skill/SKILL.md). Typed errors with payloads,
   parse at the boundary, no `unwrap` outside tests/binaries, injected clock, exhaustive matches.
   Exceptions need `// fp-allow: <why>` and must survive `cargo xtask lint-fp`.
3. **Run the gate locally before opening a PR** — exactly what CI runs:
   ```sh
   cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings
   cargo deny check && cargo nextest run --workspace --all-features
   cargo xtask lint-fp && cargo xtask lint-taste && cargo xtask lint-docs && cargo xtask gen-docs --check
   cargo xtask coverage
   ```
4. **Self-review** with the skill's [`code-review.md`](skills/rust-fp-skill/references/code-review.md)
   checklist. Report honestly: a claimed-green report that is red is worse than no report.
5. **Document decisions** in the relevant exec plan under `docs/exec-plans/` (decision log), and
   keep `Status`/`Verified` lines current. `docs/generated/` is regenerated, never hand-edited.
6. **PRs** are small and short-lived; commit messages are imperative with no attribution
   trailers. Merge policy: [`docs/design-docs/merge-policy.md`](docs/design-docs/merge-policy.md).

Security issues: see [`docs/SECURITY.md`](docs/SECURITY.md) — do not open public exploits.
