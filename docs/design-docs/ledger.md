# Ledger: how Beads is used

- **Status:** accepted · **Verified:** 2026-08-29 against `crates/domain/src/{kind,meta}.rs`, `crates/infra/src/bd.rs`

Beads (`bd`, Dolt-backed) is used unmodified. The factory adds conventions:

- **Kind** is a label `fac:kind=<epic|task|verify|merge|question|incident|reference>` (`domain::BeadKind`). Children are created with `--no-inherit-labels` so a kind is never ambiguous.
- **Task metadata** lives under `metadata.fac` (`domain::FactoryMeta`, versioned): verify bead id, base sha, budget, usage, lease-expiry count, and the task state. **Verify** beads carry `metadata.fac_verify` (task id, commands, timeout); **merge** beads `metadata.fac_merge` (task, branch, head).
- **Dependencies**: `needs` = `blocks` edges added with `bd dep add <dependent> <blocker>` after creation (`--deps blocks:` means the opposite). Beads with `needs` are created `--defer`red and un-deferred once edges exist. Verify beads need their task so `bd ready` hides them.
- **Reference** beads (Planner context) are created closed; the Steward ignores them for epic closure.
- **Ready** = `bd ready --label fac:kind=task`; the Worker additionally requires task state `open`.
- All writes go through the `BeadStore` port (`app::ports`); the adapter shells out to `bd … --json`. Nothing touches Dolt directly.
