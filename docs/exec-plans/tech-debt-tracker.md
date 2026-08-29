# Tech debt
| Item | Where | Why it matters | Bead |
|---|---|---|---|
| `factory cli::run` dispatch arms untested (need adapter injection) | `crates/factory/src/cli.rs` | 68% file coverage | tbd |
| Incident beads have no parent | `app::transition` | hard to find per epic | tbd |
| Steward escalates wall-clock via `Escalate{Manual}` | `app::steward` | reason should be `Budget` | tbd |
| OpenTelemetry export (traces/metrics) not implemented; JSON logs + events.jsonl only | binaries, `app::events` | agents cannot query timing across roles | tbd |
