# Exec plan: rescaffold for harness engineering

- **Status:** active · **Owner:** human steers, agents execute · **Started:** 2026-08-29
- **Source of the approach:** `docs/references/harness-engineering.md` (OpenAI, Feb 2026)
- **Beads epic:** `fac-2qi` (see `bd dep tree`)

## Goal

Make this repository the agent's entire world: a ≤100-line `AGENTS.md` map, a `docs/`
system of record, invariants enforced mechanically (lints, structural tests, CI), line
coverage ≥ 85% with a hard gate, and a recurring garbage-collection loop — without rewriting
the working Rust or breaking the rig.

## Non-goals

- Rewriting crates. The `domain → app → infra → bins` layering already matches the model.
- "Zero human-written code" as a rule. Our rule is *no undocumented decisions*.
- Phase 1 features (batch-bisect integrator, flaky detection). Those get their own plans.

## Epics (in order; each is a beads epic with tasks)

1. **repo-split** — own repo, `main`, `.github/` skeleton, `cargo deny`/`nextest` installed. *(done except CI)*
2. **knowledge-base** — `AGENTS.md` as table of contents; `docs/` per the layout below; product
   vision + product spec; design docs with `status`/`verified` headers; references for every
   external tool (bd, A2A, Claude headless, OpenCode server API, Codex exec); `DEPLOYMENT.md`,
   `SECURITY.md`, `RELIABILITY.md`, `QUALITY_SCORE.md`, `tech-debt-tracker.md`.
3. **enforcement** — CI (fmt, clippy deny tier, deny, nextest, llvm-cov `--fail-under-lines 85`,
   docs lint, compose config, image build); `xtask lint-docs` (reachability from `AGENTS.md`,
   no dangling links, required headers); structural tests for crate layering with
   remediation messages; taste lints (file ≤ 600 lines, no `println!` outside bins,
   structured tracing fields, `pkill -f` banned in scripts).
4. **coverage-85** — move `factory`/`stewardd` `main.rs` logic into testable modules; adapter
   tests against real `bd`/`git` in temp dirs; `coverage(off)` only on true I/O shims;
   `cargo-mutants` on `domain`. Gate rises 77 → 80 → 85 across PRs.
5. **legibility** — `factory doctor`, `factory watch`, `factory inbox`; OpenTelemetry export +
   `--profile observe` stack; compose project per worktree; `docs/generated/` from types.
6. **gardening** — golden principles doc; scheduled quality-grade + doc-drift run that opens
   fix-up beads/PRs; merge policy doc; skills mirrored for all three harnesses.

## Docs layout (target)

```
AGENTS.md  ARCHITECTURE.md  README.md
docs/
  design-docs/{index,core-beliefs,golden-principles,ledger,state-machine,harness-port,rig-sandbox,merge-policy}.md
  product-specs/{index,vision,product,roles}.md
  exec-plans/{active,completed}/  exec-plans/tech-debt-tracker.md
  references/{harness-engineering,beads,a2a,claude-headless,opencode-server,codex-exec}.md
  generated/{bead-schema,state-machine,cli}.md
  DEPLOYMENT.md  SECURITY.md  RELIABILITY.md  QUALITY_SCORE.md  PLANS.md
```

## Acceptance

- `AGENTS.md` ≤ 100 lines and every `docs/**/*.md` reachable from it (lint-enforced).
- CI green on `main` with the 85% gate; `cargo mutants -p domain` ≥ 80% caught.
- A fresh agent session, given only the repo, can run a rig end-to-end from `DEPLOYMENT.md`.
- Quality score file lists every crate and role with a grade and last-verified date.

## Decision log

- 2026-08-29 — Split into own repo at `the-zoop-troop/software-factory-rs`; notes repo history
  kept, project removed going forward. Coverage gate 85% (user).
- 2026-08-29 — Keep existing crate layering; scaffolding refactor only, shipped as small PRs.
- 2026-08-29 — Human rule is "no undocumented decisions", not "no human code".

- 2026-08-29 — Audience: open-source project. Positioning: the three inversions (no orchestrator,
  done-means-verified, YOLO only inside a rig). **No references to prior-art tools by name anywhere
  in the repo** (user). License: MIT.

## Progress

- [x] repo split, `main`, beads moved, builds/tests green standalone
- [ ] knowledge-base
- [ ] enforcement
- [ ] coverage-85
- [ ] legibility
- [ ] gardening
