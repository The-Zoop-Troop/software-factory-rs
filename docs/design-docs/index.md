# Design docs

Each doc carries `Status` (draft | accepted | superseded) and `Verified` (how and when its claims were last checked against code). If a doc disagrees with the code, the code wins and the doc is a bug — file it.

| Doc | Covers | Status |
|---|---|---|
| `core-beliefs.md` | Agent-first operating principles | accepted |
| `golden-principles.md` | Mechanical code rules and why | accepted |
| `ledger.md` | How Beads is used: kinds, metadata, deps, deferral | accepted |
| `state-machine.md` | Task lifecycle, events, effects, budgets, leases | accepted |
| `harness-port.md` | The `Harness` port and the three adapters | accepted |
| `rig-sandbox.md` | Container, egress, credentials, volumes | accepted |
| `merge-policy.md` | Integrator behaviour and PR/merge philosophy | accepted |
| `railway.md` | Railway-oriented control flow as a product principle; compatibility rule for stored metadata | accepted |
| `remote-control.md` | The console: A2A over rigs, plan queue, scoped tokens, audit, budgets | accepted |
