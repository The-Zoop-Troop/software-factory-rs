# Remote control

- **Status:** accepted · **Verified:** 2026-08-29 (`crates/console` tests over the app fakes; `docker compose up console`)

The console (`crates/console`) is the only surface of a rig reachable from outside its network. It is an A2A server (`docs/references/a2a.md`): every client — the `factory` CLI, a chat bot, a browser, another agent — is an A2A client with a bearer token. The exec plan is `docs/exec-plans/active/remote-control.md`.

## Shape

```
client ──HTTPS, Bearer──▶ console ──bd / events.jsonl──▶ rig ledger (volume)
                            │
                            └── plan_request bead ──▶ rig `planner` service ──▶ epic
```

- **Read models** live in `app::remote::a2a`: an epic is an A2A `Task` (`SUBMITTED` → `WORKING` → `COMPLETED`; `INPUT_REQUIRED` while a child is in incident; `CANCELED` after `CancelTask`). Incidents and questions are their own `INPUT_REQUIRED` tasks with the epic as `contextId`.
- **Workflows** live in `app::remote::service`: each A2A operation is one function `authorize → act → audit`. Refusals are audited too (`EventKind::Remote { action: "refused" }`).
- **Ports**: `Authenticator` (token → `Principal`), `RigRegistry`, `EventTail` (cursor over `events.jsonl`), `PlanSubmitter`. The console implements them with `bd`, files, and the plan queue; tests use `app::testing::remote` fakes.

## Plan queue

The Planner needs the rig's harness credential, which the console must never hold. So `SendMessage` without `taskId` creates a `plan_request` bead (`app::plan_request`) and waits for the rig's `planner` service (`factory plan --queue --interval N`, `docker compose up planner`) to plan it and close it with `epic <id>` in its notes (`app::plan_queued_once`). Pull-based, like every other role; a registry entry may instead name a `plan_cmd` for hosts that run the planner locally.

## Auth, audit, budgets

- Tokens are stored as sha256 (`console hash-token`), compared in constant time, and grant scopes per rig: `watch` (list, get, subscribe), `plan` (submit, cancel), `resolve` (answer inbox), `admin` (all). No scope on a rig means the rig does not exist for that client (A2A §13.1).
- Every remote action appends a `FactoryEvent` with `actor = remote:<client>` to the rig's `events.jsonl`.
- `RigBudget` (`max_tokens`, `max_usd_micros` in the registry) is checked against the ledger's summed usage before a plan is accepted.

## What it refuses by design

Merging, force-closing tasks, editing beads, reading provider credentials. Done means verified, remotely too.
