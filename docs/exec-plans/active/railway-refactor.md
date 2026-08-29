# Exec plan: railway-oriented refactor

- **Status:** active · **Owner:** human steers, agents execute · **Started:** 2026-08-29
- **Standard:** `skills/rust-fp-skill` (git submodule of <https://github.com/mikezupper/rust-fp-skill>, pinned)
- **Beads epic:** `fac-cfb`

## Goal

Make railway-oriented, functional Rust the product's own control-flow model, not just a coding
style: every role is a total function from (ledger state, fact) to (new state, effects); every
failure is a typed value carrying what the next agent needs to act on; every boundary — ledger,
git, harness output, CLI — is parsed exactly once into domain types. Measured against the skill's
`skills/rust-fp-skill/references/code-review.md`, walked in full and recorded here.

## Non-goals

- A rewrite. Crates, ports, and the state machine stay; bodies get stricter types and errors.
- Breaking stored ledger metadata (`META_VERSION` stays 1; see decision log).

## Baseline (2026-08-29, `cargo xtask lint-fp`: 54 unexplained hits)

26 string-payload error variants; 13 unjustified `let _ =` discards; 9 substring-based
error classifications; 2 `as` casts; 2 clock calls outside the adapter; 1 catch-all arm; ~20
naked-primitive public fields; 1 proptest module; 0 atomicity tests. `lint-fp` runs in CI as
non-blocking until epic 5 flips it.

## Epics (in order; each a beads epic)

1. **skill-in-repo** — submodule at `skills/rust-fp-skill` pinned to a commit; symlinks so
   Claude Code, Codex and OpenCode all load it; `cargo xtask skills --check` (drift vs upstream
   `main` → gardening files a bead); `cargo xtask lint-fp` = the skill's §1 greps as a CI gate;
   `docs/design-docs/railway.md`; vision/core-beliefs/golden-principles updated.
2. **error-tracks** — replace every `Variant(String)` in `app::ports`, `integrator`, `steward`
   with structured variants (`Blocked { by }`, `NotFastForward { branch, to }`, `Conflict { paths }`,
   `HarnessError::Auth`, …); adapters parse `bd`/`git`/CLI stderr once into them; `#[from]` at most
   once per source per enum; one test per reachable variant asserting payload, not `Display`.
3. **boundary-types** — `VerifyCommand`, `Title`, `Priority`, `Attempts`, `NonEmpty<T>`,
   `MicroUsd`; `Raw*` + `TryFrom` for `HarnessOutcome`, `NewBead`, CLI args; no `as` on external
   data; every domain `pub` field is a newtype or an enum.
4. **effects-and-sagas** — effect interpreter with atomicity tests (force a failure between
   persist and each effect; assert the ledger is detectably half-done, never silently wrong);
   Integrator as a compensation stack (rebase → checks → ff → push, undo on failure); every
   `let _ =` either justified in a comment or turned into a typed outcome.
5. **proof** — proptests: round-trip for every newtype, invariants for `Budget`, `Lease`, `Plan`
   topo-sort, `Task::apply` totality; mutation gate ≥ 90 % on `domain`; `skills/rust-fp-skill/references/code-review.md`
   walked with each item fixed or explicitly not applicable; `lint-fp` green in CI.

## Acceptance

- `cargo xtask lint-fp` has zero unexplained hits (each justified hit has a `// fp-allow: <why>` comment the lint recognises).
- No `String` payload in any `thiserror` enum outside `xtask`.
- Every `pub` field of a `domain` struct is a newtype, enum, or `NonEmpty`.
- `cargo mutants -p domain` ≥ 90 % caught; proptests exist for every newtype.
- The skill's checklist is reproduced in this file with every box checked or annotated.
- A v1 metadata fixture decodes unchanged (`crates/domain/tests/fixtures/meta_v1.json`).

## Decision log

- 2026-08-29 — Railway-oriented programming is a **product** principle: typed failures are what
  make incidents and the inbox machine-actionable (user).
- 2026-08-29 — Ledger metadata stays backward compatible. `META_VERSION` bumps only when a field's
  meaning changes; then a `factory migrate` command rewrites under the Steward, with a decode test
  for the old fixture. Tightening lives in `Raw*` decoders.
- 2026-08-29 — Skill ships as a pinned submodule, not a copy: reproducible, and bumps are
  deliberate because a skill change can change what the lints demand.

- 2026-08-29 — `lint-fp` became a blocking CI gate at the end of epic 2 (already green) instead of epic 5.

- 2026-08-29 — Prose stays `String`: `description`, `acceptance`, `reference`, incident `detail` and
  bead notes are free text by nature; newtyping them would add ceremony without a validator. Every
  field that carries a *rule* (title length, command non-empty, priority range, counts) is typed.

## Progress

- [x] skill-in-repo (submodule pinned 2d34087, harness symlinks, `xtask skills --check`, `xtask lint-fp` non-blocking in CI, railway.md, vision/beliefs/principles/AGENTS updated) — 2026-08-29
- [x] error-tracks (26 string payloads → structured variants with `op`/`cause`/ids/paths; `bd`/`git` stderr parsed once in `parse_*_stderr` with per-variant tests; `LandRejection`, `Decision`, typed events; `lint-fp` green and blocking in CI) — 2026-08-29
- [x] boundary-types (`NonEmpty<T>`, `Title`, `VerifyCommand`, `Priority`, transparent `Tokens`/`Attempts`/`Turns`/`MicroUsd`; `VerifyMeta.commands` and `Plan.tasks` non-empty by type; harness outcomes and CLI inputs parsed once; v1 metadata fixture round-trips byte-identically) — 2026-08-29
- [x] effects-and-sagas (FlakyStore atomicity test proves persist-then-effect leaves a detectable gap; Steward repairs missing merge beads idempotently; Integrator rolls `main` back on push failure via CAS `Repo::rollback`, proven against real git; every `let _` justified) — 2026-08-29
- [ ] proof
