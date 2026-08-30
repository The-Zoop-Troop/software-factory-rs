# Exec plan: throughput metrics

- **Status:** active · **Owner:** human steers, agents execute · **Started:** 2026-08-30
- **Beads epic:** `fac-n7j`
- **Depends on:** the per-rig event log (`.factory/events.jsonl` on the ledger volume), the console

## Goal

Know how long every task spent at every stop of the factory — waiting for a worker, in a
session, waiting for the Verifier, being verified, waiting for the Integrator, landing — and
turn that into the numbers that decide where concurrency pays: critical path vs wall-clock,
live sessions over time, retry tax, role idle time. Derived from the event log we already
write; no collector, no dashboard service, nothing new the rig can reach.

## Non-goals

- OpenTelemetry / Prometheus export. A `tracing` OTLP exporter can follow once the report
  says what is worth graphing live.
- Changing scheduling policy in this plan. Measure first; `RIG_WORKERS` is the one lever
  added here, and only with a before/after table.

## What the first log already says (Phase 0 of the guide project, epic `run-83d`)

| | |
|---|---|
| epic wall-clock | 69.8 min |
| planning | 7.7 min before the first claim |
| worker sessions (one worker, strictly serial) | 50.8 min — 3.1 to 11.3 min each |
| of which retries / rebase after conflict | ≈ 25 min |
| verify | 0.1 – 1.7 min per pass |
| integrate | 0.2 – 2.2 min per landing |
| operator wait (incident → retry) | ≈ 5 min + 23 min |
| tasks with no mutual deps run serially | `.5 .7 .9`: 10.9 min that three workers would do in ≈ 4 |

The Verifier and Integrator are idle almost all of the time; the worker is the bottleneck and
the operator is the second one. Missing from the log: when a task *became* ready, and when the
Verifier/Integrator *picked it up* — so queue wait and work cannot yet be separated for those
two roles.

## Shape

```
events.jsonl ──▶ app::metrics (pure)  ──▶ factory metrics <rig>   (CLI: table / --json / --csv)
                      │                ──▶ GET /rigs/<rig>/metrics (console)
                      └──▶ Throughput page: Gantt per epic, one row per task, segments by stage
```

Stage edges per attempt: `ready → claimed → submitted → verify_started → verified →
integrate_started → integrated` (`escalated`, `lease_reaped`, `released` end an attempt).

Per epic: wall-clock, sum of work, critical path (longest `needs` chain by actual durations),
parallelism = work / wall-clock, concurrency profile (live sessions per minute), first-pass
verify rate, incidents by reason, tokens per landed task, role idle time, retry tax.

## Tasks

1. `fac-n7j.1` stage-boundary events (`ready`, `verify_started`, `integrate_started`)
2. `fac-n7j.2` closed tasks keep usage; `submitted` carries tokens/turns/wall-clock
3. `fac-n7j.3` `app::metrics` — the pure fold, tested on the anonymised Phase 0 log
4. `fac-n7j.4` `factory metrics` + console route
5. `fac-n7j.5` console Throughput view
6. `fac-n7j.6` `RIG_WORKERS` replicas, measured on a later phase of the guide project
7. `fac-n7j.7` history read model: closed epics in the A2A read models + a server-side,
   epic-filtered read of the full event log (the list of past epics derives from
   `task_planned` … `epic_closed`, no `bd` calls)
8. `fac-n7j.8` console history: Completed section, closed epic page, history-fed timeline
   (sequenced before the Gantt so it draws real closed epics)

## Decision log

- 2026-08-30 — derive from the event log, no telemetry stack: the questions are offline
  questions and the rig must not gain an egress path for metrics.
- 2026-08-30 — history is part of this plan: a closed epic vanished from the console because
  the read models list active beads only and the timeline is an in-memory ring; the data was
  never lost (ledger + events.jsonl on the volume). The throughput page is a per-epic history
  view, so both ship here. Events, not `bd`, feed history lists (ledger latency, `fac-crw`).
- 2026-08-30 — the first measurement that mattered was not in the log: a `bd` call took
  3–16 s (embedded Dolt, six processes on one volume lock), which stretched `/rigs` to 100 s+
  and reaped a live lease. Fixed as `fac-crw`: one Dolt SQL server per rig (`ledger` service),
  45–200 ms per call measured on a copy of the Phase 0 ledger.
- 2026-08-30 — stage events are additive `EventKind` variants; the metrics module is pure and
  lives in `app`, so the planner can consume the same numbers later.
