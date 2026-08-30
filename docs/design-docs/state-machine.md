# Task state machine

- **Status:** accepted · **Verified:** `cargo test -p domain` (`task::tests`) 2026-08-29; diagram mirrors `domain::task`

```
open ──claim──▶ leased ──submit──▶ in_verify ──pass──▶ mergeable ──merged──▶ closed
                 │  │                  │                   │
   lease expired │  │ release          │ fail              │ merge failed
                 ▼  ▼                  ▼                   ▼
                open (attempt++ on release/fail; note appended)
                 └── attempts/tokens/wall-clock budget exceeded, or ≥3 lease expiries ──▶ incident
```

- `Task::apply(Event) -> Result<Transition, IllegalTransition>` is total over (state, event); illegal pairs are listed exhaustively so a new variant breaks the build.
- A `Transition` carries **effects** (`AppendNote`, `OpenMergeBead`, `OpenIncidentBead`, `CloseTaskBead`, `CloseVerifyBead`) that the imperative shell (`app::transition::run_effect`) executes after persisting the new state — persist first, so a crash leaves a detectable gap rather than an undoable one.
- **Heartbeat** renews a lease from *now*, never from the old expiry. Only the lease holder may heartbeat, submit, or release. The Worker samples the worktree on every heartbeat (`Repo::diff_stat`) and records a `progress` event — files touched, lines added/removed — so an operator can see a session working without reading the harness log.
- **Stage edges for metrics.** Every stop has both edges in the event log: `task_planned {epic, needs}` (planner), `claimed`, `submitted`, `verify_started`, `verified`, `integrate_started`, `integrated`, with `released`, `lease_reaped`, `escalated` ending an attempt. A task is *ready* at `max(task_planned, integrated of each need)`, so queue wait, work, and idle time per role are all derivable without any role keeping state (see `docs/exec-plans/active/throughput-metrics.md`).
- **Blocked by the agent.** A worker that writes a `FACTORY_BLOCKED` note file at the repo root is released with the file's text as the note (`released: blocked: …`); the Worker removes the file before committing, so it never reaches the branch. Repeated releases become an incident through the attempt budget as usual.
- **VerifyBlocked** (`in_verify` → `incident`, reason `environment`) is for checks the rig *could not run*: exit 126/127, "permission denied", "no space left", a read-only filesystem, a missing command, or an unreachable host (`app::verifier::environmental`). No attempt is charged — the model was never given a chance — and the incident offers **resume from the branch** first: the next session starts from `task/<id>` instead of the integration branch, keeping the work already committed there.
- **Budgets** (`domain::Budget`): attempts are checked first because they mean "the model keeps failing".
