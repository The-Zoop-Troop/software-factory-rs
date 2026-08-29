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
| Crash between persisting `mergeable` and creating the merge bead | Steward sweep (task mergeable, no merge bead) | merge bead re-created (idempotent) |
| Push fails after fast-forward | Integrator saga | `main` rolled back by compare-and-swap; task stays mergeable; retry next pass |
| Planner emits an unrunnable verify command | Verifier failure ×3 | incident; fix the prompt or runner, then reopen |
| A human is needed (incident, question) or an epic ends | console alert sweep / Telegram bot (poll + diff, closed epics fetched once) | webhook / chat message with the task id |
| Remote client misuses the console | scope check before every action | HTTP 401/403, refusal audited in the rig's event log |
| Ledger or repo volume lost | `factory rig doctor` (`ledger=missing`) | restore from the latest `factory rig backup` tarballs |

Budgets default to 400k tokens, 45 min, 3 attempts per task (`domain::Budget`). Every transition is appended to `.factory/events.jsonl`. Known gaps: no flaky-test detection, no batch merge, no automatic re-planning on stalled epics (Phase 1).

## Restore drill

Run this quarterly per rig; it takes a few minutes and proves the backups are real.

1. `factory rig backup toy --to backups/` → two tarballs (`toy-ledger-<ts>.tgz`, `toy-repo-<ts>.tgz`).
2. `docker compose -p factory-toy down` (the restore refuses while anything runs).
3. `factory rig restore toy --ledger backups/toy-ledger-<ts>.tgz --repo backups/toy-repo-<ts>.tgz`.
4. `docker compose -p factory-toy up -d && factory rig doctor` → `ok   toy  ledger=yes running=[…]`.
5. `factory --rig … watch` (or `docker compose -p factory-toy exec steward factory watch`) shows the same epics and counts as before the drill; `bd doctor` inside the rig is clean.

Record the date and outcome in `docs/QUALITY_SCORE.md`. A failed step is an incident: file a bead, fix, repeat.
