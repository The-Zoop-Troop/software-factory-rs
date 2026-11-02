# Task state machine

- **Status:** accepted · **Verified:** 2026-11-02 against `domain::task`, `app::console::resolve` and `app::steward`; the workspace test suite passes; diagram mirrors `domain::task`

```
open ──claim──▶ leased ──submit──▶ in_verify ──pass──▶ mergeable ──merged──▶ closed
                 │  │                  │                   │
   lease expired │  │ release          │ fail              │ merge failed
                 ▼  ▼                  ▼                   ▼
                open (attempt++ on release/fail; note appended)
                 └── attempts/tokens/wall-clock budget exceeded, or ≥3 lease expiries ──▶ incident
```

- `Task::apply(Event) -> Result<Transition, IllegalTransition>` is total over (state, event); illegal pairs are listed exhaustively so a new variant breaks the build.
- **Leaving `incident`.** `Task::apply` rejects every event from `incident`. The one way back is an operator's retry: `app::console::resolve` writes `open` directly, with fresh usage, lease expiries and blocked releases.
- A `Transition` carries **effects** (`AppendNote`, `OpenMergeBead`, `OpenIncidentBead`, `CloseTaskBead`, `CloseVerifyBead`) that the imperative shell (`app::transition::run_effect`) executes after persisting the new state — persist first, so a crash leaves a detectable gap rather than an undoable one.
- **Heartbeat** renews a lease from *now*, never from the old expiry. Only the lease holder may heartbeat, submit, or release. The Worker samples the worktree on every heartbeat (`Repo::diff_stat`) and records a `progress` event — files touched, lines added/removed — so an operator can see a session working without reading the harness log.
- **Stage edges for metrics.** Every stop has both edges in the event log: `task_planned {epic, needs}` (planner), `claimed`, `submitted`, `verify_started`, `verified`, `integrate_started`, `integrated`, with `released`, `lease_reaped`, `escalated` ending an attempt. A task is *ready* at `max(task_planned, integrated of each need)`, so queue wait, work, and idle time per role are all derivable without any role keeping state (see `docs/exec-plans/completed/throughput-metrics.md`).
- **Refused by the provider.** A session the provider refused (a usage or spend limit, a rate limit, an outage) goes back to `open` through `provider_stopped`, which charges no attempt and no wall clock, adds no note and opens no incident. A partial commit is not verified: the retry recuts the branch from main, so every verified result comes from one uninterrupted session. The role waits `RIG_LIMIT_WAIT` seconds (default 900) before polling again, and a `provider_stopped` event records the tokens the discarded session used. Faults a person must fix, such as a bad credential, still charge attempts and reach an incident. A provider that never recovers therefore waits forever; only the event log and the role's log show it.
- **Blocked by the agent.** A worker that writes a `FACTORY_BLOCKED` note file at the repo root is released with the file's text as the note (`released: blocked: …`); the Worker removes the file before committing, so it never reaches the branch. Two consecutive blocked releases become a `ReleaseLoop` incident without waiting for the attempt budget.
- **VerifyBlocked** (`in_verify` → `incident`, reason `environment`) is for checks the rig *could not run*: exit 126/127, "permission denied", "no space left", a read-only filesystem, a missing command, or an unreachable host (`app::verifier::environmental`). No attempt is charged — the model was never given a chance — and the incident offers **resume from the branch** first: the next session starts from `task/<id>` instead of the integration branch, keeping the work already committed there.
- **Budgets** (`domain::Budget`): attempts are checked first because they mean "the model keeps failing". They are checked after a failed verify, a failed merge or a release. The Steward's mid-lease check adds only wall clock, so the token limit never stops a running session or a passing result.
