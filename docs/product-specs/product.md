# Product spec

- **Status:** accepted · **Verified:** against the running system, 2026-08-29 · **Owner:** product
- See `vision.md` for why; this is what.

## Inputs and outputs

| | |
|---|---|
| Input | A plan in prose (or a hand-written epic bead) for a new or existing git repository |
| Output | Commits on the project's integration branch, each task verified and landed; a closed epic in the ledger; an append-only event log |
| Operator surface | `factory` CLI, `bd` (the ledger), `docker compose` (the rig), `.factory/events.jsonl` |

## Roles (each a process in the rig)

| Role | Responsibility | LLM? |
|---|---|---|
| Planner | Plan → epic of tasks with acceptance criteria, verify commands, and ordering | yes |
| Worker | Claim a ready task, cut a branch, run one harness session with a curated packet, commit, submit | yes |
| Verifier | Run the task's verify commands verbatim in a clean worktree; pass/fail is a fact | no |
| Integrator | Rebase onto main, run project checks, fast-forward, push; the only thing that pushes | no |
| Steward | Reap expired leases, enforce budgets, close finished epics, write the event log | no |

## Guarantees

- A task closes only after its verify commands pass and the Integrator lands it.
- A worker holds a task only while its lease is alive; dead workers lose the task automatically.
- Budgets (tokens, wall clock, attempts) are enforced; exhaustion produces an incident, not a loop.
- Agents run only inside the rig: non-root, no capabilities, allowlisted egress, no host credentials.
- Every state transition is in the ledger and the event log; nothing happens off the record.

## Harnesses

`--harness claude | opencode | codex`, with `--model`. OpenCode works with any OpenAI-compatible provider via `OPENCODE_*` env; Codex requires an OpenAI Responses-API endpoint.

## Human touchpoints

- Write the plan; optionally hand-edit the epic before workers start.
- Answer `question` beads and resolve `incident` beads (surfaced by `factory inbox`, planned).
- Set budgets, choose harnesses/models, and decide what lands where (remote, branch protection).

## Non-goals (v1)

Hosted operation; multi-rig federation; human-free plan authoring; UI beyond the CLI.

## Roadmap pointers

`docs/PLANS.md` and `docs/exec-plans/active/` are the source of truth for what is being built next.
