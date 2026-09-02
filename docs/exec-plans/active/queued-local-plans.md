# Exec plan: queue plan requests from inside the rig

- **Status:** in progress · **Verified:** design verified against `crates/app/src/plan_queue.rs` and `crates/infra/src/bd.rs` 2026-09-02

## Goal

`factory plan --queued` writes the same `plan_request` bead a console submission creates —
straight to the local ledger, with optional `--after rig:epic` gates — so an operator inside
the rig gets queue visibility (console request cards) and After-gating without holding a
console token. The planner's existing `--queue` service serves it; deferred cross-rig
requests wait for the console's dependency sweep exactly as remote ones do. Zero console
changes.

## Why

Operating a rig without a console token forces the inline planner one-shot, which creates
epics directly: queued slices are invisible until submitted, and `--after` is silently
ignored in local mode (a footgun found while running a multi-slice plan for a client rig).
The bead (`plan_request_with_needs`) and the sweep already support everything needed; only
the CLI entry point is missing.

## Change set

1. **Extract `plan_cmd.rs`** — `cli.rs` sits at the 600-line lint-taste cap; the Plan
   variant's args move to a `#[derive(clap::Args)] PlanArgs` and the arm body to
   `plan_cmd::run`. No behavior change; the generated CLI reference must be identical apart
   from the new flag in step 2.
2. **`--queued`** (conflicts with `--queue`): parse text + `--after`, create the bead via a
   new `app::submit_plan_request` (typed error: `EmptyText | Store`), print the request id
   and state. `--after` without `--queued`/`--rig` errors instead of being ignored;
   `--queued` with `--rig` is rejected as local-only.
3. **Docs** — regenerated CLI reference, first-project guide, README operating table.

## Decision log

- **2026-09-02** — Reuse `plan_request_with_needs` verbatim rather than a new bead shape:
  the console lists request cards straight from the ledger, so a locally created bead is
  indistinguishable from a remote one, and the dependency sweep needs no changes.
- **2026-09-02** — `--after` in plain local mode becomes an error, not a warning: it has
  been silently ignored, which cost a real run its gating.
- **2026-09-02** — Client string for locally queued requests is `cli` (audit trail parity
  with remote client names).

## Progress log

- **2026-09-02** — Plan authored; epic `fac-cxz` with three tasks.
