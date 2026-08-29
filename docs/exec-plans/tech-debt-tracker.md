# Tech debt
| Item | Where | Why it matters | Bead |
|---|---|---|---|
| `task.rs` exceeds 600-line cap | `crates/domain/src/task.rs` | legibility | tbd |
| Binaries untested | `crates/{factory,stewardd}/src/main.rs` | coverage gate | epic coverage-85 |
| Per-project toolchain baked into one image | `docker/Dockerfile.rig` | projects need different tools | tbd |
| Incident beads have no parent | `app::transition` | hard to find per epic | tbd |
| Steward escalates wall-clock via `Escalate{Manual}` | `app::steward` | reason should be `Budget` | tbd |
