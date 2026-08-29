# Reliability

- **Status:** accepted · **Verified:** unattended run `rig-v3e` 2026-08-28 (5/5 tasks, 0 incidents)

| Failure | Detection | Response |
|---|---|---|
| Worker process dies mid-task | lease expiry (Steward sweep) | task → `open`; ≥3 expiries → incident |
| Harness errors with no changes | worker | task released (attempt++) |
| Verify fails | Verifier | task → `open` with output; attempts exhausted → incident |
| Rebase conflict / project checks fail | Integrator | task → `open` with output (attempt++) |
| Remote/git/ledger unavailable | Integrator/Worker | nothing changes; retried next pass |
| Runaway session (wall clock) | Steward, mid-lease projection | incident |
| Planner emits an unrunnable verify command | Verifier failure ×3 | incident; fix the prompt or runner, then reopen |

Budgets default to 400k tokens, 45 min, 3 attempts per task (`domain::Budget`). Every transition is appended to `.factory/events.jsonl`. Known gaps: no flaky-test detection, no batch merge, no automatic re-planning on stalled epics (Phase 1).
