# Exec plan: cross-rig dependencies

- **Status:** active · **Owner:** human steers, agents execute · **Started:** 2026-08-30
- **Beads epic:** `fac-e8o`
- **Depends on:** `docs/exec-plans/completed/remote-control.md` (console, rig registry), `docs/exec-plans/completed/web-console-v2.md`

## Goal

Let one submission drive a change that spans several repositories (several rigs), with the
ordering the plan needs: an epic on rig B may **need** an epic on rig A, and B's planner should
see A's landed result (the contract) when it plans. Today ordering across rigs is hand-gated from
the console.

## Motivating case

A feature across a Rust backend, three frontends, a Go runner and a parent docs/submodules repo:
runner ∥ backend-1 → backend-2 → {portal, admin} ∥ legal → parent end-state sweep. The portal
planner needs the backend's landed API shape (error type, models fields, balance response); the
parent sweep needs every other epic's final commit.

## Design (sketch — refine before starting)

1. **Contract artifacts.** When an epic closes, the Steward writes a `contract` reference bead on
   the epic's rig: landed commit range, changed public surface (API routes/types, CLI flags, env
   vars) extracted by a small per-runtime summariser, and the epic's plan text. Stored on the
   ledger so it is auditable and versioned.
2. **Cross-rig `needs`.** The console keeps a host-level dependency table
   (`~/.factory/deps.toml`, or a bead on a designated coordination rig): `rig-b/epic-x needs
   rig-a/epic-y`. A dependent epic is submitted as a `plan_request` **deferred** until every
   need is closed; the console's sweep un-defers it and injects the needed contracts into the plan
   text before the rig's planner runs.
3. **Submission.** `SendMessage` gains `needs: [{rig, task}]` (A2A `referenceTaskIds` across rigs,
   qualified by rig); the UI's plan form offers "after …" pickers; the overview shows the
   cross-rig graph and what is blocked on what.
4. **Failure semantics.** A needed epic that ends canceled/failed blocks its dependents with an
   attention item ("upstream epic failed") offering *re-plan without it* / *cancel dependents*.
5. **No shared ledger.** Rigs stay isolated; only the console reads across them. This keeps
   the security posture (one credential per rig, no cross-rig writes) unchanged.

## Non-goals

- Concurrent multi-rig execution tuning (resource scheduling) — separate plan.
- Cross-repo atomic commits — each rig lands on its own branch; the parent sweep records pointers.

## Acceptance

- Submitting the motivating case from the console with needs declared runs end to end without
  hand-gating; each dependent planner sees the upstream contract in its plan text.
- A failed upstream epic surfaces as an attention item on every dependent with the two options.
- Rigs remain isolated (no new credentials or mounts).

## Decision log

- 2026-08-30 — the summariser is a language-agnostic filter over added lines (`pub fn`,
  `export`, exported Go names, `def`/`class`, route registrations, env-var reads), capped at 200
  lines, rather than per-runtime parsers: cheap, good enough for a planner, and honest about
  what it is. Contract beads are children of the epic, closed on creation, ignored by
  `bd ready` and by the epic-close rule.

- 2026-08-30 — Planned after a hand-gated multi-rig run showed the gap; contracts are extracted
  by the factory rather than written by workers so they cannot drift from what landed.

## Progress

- [x] contract artifacts — `fac-e8o.1`: `contract` bead per closed epic (range, files, added public surface across Rust/TS/Go/Python + routes + env vars, tasks with landed shas, plan + reference text); `stewardd --repo --main`; event `contract_written` (2026-08-30)
- [x] cross-rig needs + deferred plan requests — `fac-e8o.2`: `SendMessage` `metadata.needs: [{rig, epic}]` creates the request deferred with `fac_needs`; the console sweeps every 30 s, resolves each need on its rig (closed + contract child), appends `## Upstream contracts` to the request and un-defers it; event `remote/deps_ready` (2026-08-30)
- [x] submission (A2A + UI) — `fac-e8o.3`: `SendMessage` `metadata.needs`, `factory --rig plan --after rig:epic`, the plan form's **After** picker (open epics on other rigs), request cards show *after … waiting*, deferred requests listed with `needs`/`waiting` (2026-08-30)
- [ ] failure semantics
