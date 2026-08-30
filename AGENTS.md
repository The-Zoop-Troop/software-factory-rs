# AGENTS.md — the map

You are working in **software-factory-rs**: an autonomous software factory (plan in → verified,
merged code out). This file is a table of contents, not a manual. Read what the task needs.

## Start here

1. `bd prime` — session contract + memories. Re-run after compaction. Never `bd edit`.
2. `bd ready` — what is claimable. Claim with `bd update <id> --claim`. Close with a reason.
3. Find the plan: `docs/exec-plans/active/` (one file per piece of work in flight).

## Where truth lives

| Question | Read |
|---|---|
| What is this and why | `docs/product-specs/vision.md`, `docs/product-specs/product.md` |
| How it is built (map of crates and roles) | `ARCHITECTURE.md` |
| Principles we hold and rules we enforce | `docs/design-docs/core-beliefs.md`, `docs/design-docs/golden-principles.md`, `docs/design-docs/railway.md` |
| The Rust standard (railway-oriented FP) | `skills/rust-fp-skill/SKILL.md` — read it before touching Rust; walk `references/code-review.md` before declaring done |
| Detailed design per pillar | `docs/design-docs/index.md` |
| Running / deploying a rig | `docs/DEPLOYMENT.md` |
| Threat model and sandbox rules | `docs/SECURITY.md` |
| Failure modes and budgets | `docs/RELIABILITY.md` |
| How good each part is right now | `docs/QUALITY_SCORE.md` |
| Known debt | `docs/exec-plans/tech-debt-tracker.md` |
| External tools' exact contracts | `docs/references/` (bd, A2A, Claude headless, OpenCode server, Codex exec) |
| Facts derived from code | `docs/generated/index.md` (regenerate with `cargo xtask gen-docs`, never hand-edit) |

## Rules that are enforced mechanically (CI fails otherwise)

- Layering: `domain` (leaf) ← `app` ← `infra` ← binaries. `app` never names `infra`.
- Lints: `cargo clippy --all-targets --all-features` at the deny tier in `Cargo.toml`;
  `clippy.toml` bans `unwrap`/`expect`/`SystemTime::now`/unbounded channels by path.
- Coverage: `cargo llvm-cov` line coverage ≥ 85% (`xtask coverage`).
- Docs: every file under `docs/` is reachable from this file or an `index.md`; design docs carry
  `Status`/`Verified` lines; links resolve (`xtask lint-docs`).
- Files ≤ 600 lines; no `println!` outside binaries; no `pkill -f` in scripts (`xtask lint-taste`).
- The skill's mechanical sweep (`xtask lint-fp`): no `String` error payloads, no substring error classification, no unjustified `let _ =`, no `as` on external data; exceptions need `// fp-allow: <why>`.

## How to work

- Create the bead before the code. File discovered work with `--deps discovered-from:<id>`.
- Decisions go in the exec plan's decision log, not in chat.
- Parse at the boundary; errors are typed data; time and IDs are injected. See golden principles.
- Self-review with `skills/rust-fp-skill/references/code-review.md`; run its §1 greps via `cargo xtask lint-fp`.
- Verify your change the way the factory would: `cargo test`, `cargo clippy`, `xtask lint-docs`,
  and for rig changes `docker compose config` + a `factory doctor` run.
- Commit messages: imperative, no attribution trailers.

## Quick commands

```
cargo build && cargo test && cargo clippy --all-targets --all-features
cargo xtask lint-docs | lint-taste | lint-fp | coverage | gen-docs [--check] | quality [--check] | skills --check
docker compose build && docker compose up -d      # a rig; see docs/DEPLOYMENT.md
factory doctor && factory watch && factory inbox     # health, progress, what needs a human
factory plan --harness opencode --model provider/model --text "..."
```
- `docs/guides/first-project.md` — walkthrough: a real multi-repo change through the factory, end to end
