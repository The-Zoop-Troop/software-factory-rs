# Ledger: how Beads is used

- **Status:** accepted · **Verified:** 2026-08-29 against `crates/domain/src/{kind,meta}.rs`, `crates/infra/src/bd.rs`

Beads (`bd`, Dolt-backed) is used unmodified. The factory adds conventions:

- **Kind** is a label `fac:kind=<epic|task|verify|merge|question|incident|reference>` (`domain::BeadKind`). Children are created with `--no-inherit-labels` so a kind is never ambiguous.
- **Task metadata** lives under `metadata.fac` (`domain::FactoryMeta`, versioned): verify bead id, base sha, budget, usage, lease-expiry count, and the task state. **Verify** beads carry `metadata.fac_verify` (task id, commands, timeout); **merge** beads `metadata.fac_merge` (task, branch, head).
- **Server mode**: every rig runs one `ledger` service — `dolt sql-server` over the ledger
  volume's `embeddeddolt` store, reachable on the rig's private network as `ledger-<rig>:3307`
  (user `factory`, one password per host in `~/.factory/ledger.password`, passed as
  `BEADS_DOLT_PASSWORD`). Every role and the console (which joins each rig's network) talk to
  it; `bd` in embedded mode opened the engine in-process on every call and, with six processes
  on one lock, cost 3–16 s per call (2026-08-30 measurement; server mode: 45–200 ms). The
  `ledger` role in `docker/entrypoint.sh` flips the ledger's metadata to server mode and
  creates the user idempotently, so an existing embedded ledger migrates on first start.
- **Contracts**: when the Steward closes an epic it writes a `fac:kind=contract` child (closed): the landed commit range, files, added public surface, the tasks with their landed shas, and the plan/reference text — what a downstream planner reads instead of the request (`docs/exec-plans/active/cross-rig-dependencies.md`).
- **Dependencies**: `needs` = `blocks` edges added with `bd dep add <dependent> <blocker>` after creation (`--deps blocks:` means the opposite). Beads with `needs` are created `--defer`red and un-deferred once edges exist. Verify beads need their task so `bd ready` hides them.
- **Reference** beads (Planner context) are created closed; the Steward ignores them for epic closure.
- **Ready** = `bd ready --label fac:kind=task`; the Worker additionally requires task state `open`.
- All writes go through the `BeadStore` port (`app::ports`); the adapter shells out to `bd … --json`. Nothing touches Dolt directly.
