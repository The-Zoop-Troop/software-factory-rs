# Tech debt
| Item | Where | Why it matters | Bead |
|---|---|---|---|
| `factory cli::run` dispatch arms untested (need adapter injection) | `crates/factory/src/cli.rs` | 68% file coverage | tbd |
| Incident beads have no parent | `app::transition` | hard to find per epic | tbd |
| Steward escalates wall-clock via `Escalate{Manual}` | `app::steward` | reason should be `Budget` | tbd |
| OpenTelemetry: traces exported (`infra::telemetry`, `OTEL_EXPORTER_OTLP_ENDPOINT`); metrics still only in events.jsonl | binaries | span timing across roles works; no counters/histograms yet | when a dashboard needs them |
