# Scaling and tuning: read the metrics first

Never scale on a hunch. The event log knows how long every stop took and whether more
workers would have helped.

## The throughput report

```sh
factory --rig https://<console>/rigs/<rig> --token $TOKEN metrics          # every epic
factory --rig … metrics --epic <id> --json                                  # one epic, machine-readable
$R exec steward sh -c 'cd /work/rig && factory metrics --csv'               # inside the rig
```

Console: `GET /rigs/<rig>/metrics?epic=…`; the epic page's **throughput →** link draws every
attempt by stage (queue wait → session → verify wait → verify → integrate wait → integrate).

How to read it:

| Number | Meaning | Act when… |
|---|---|---|
| wall-clock vs **work** | elapsed vs summed active stage time | big gap + low parallelism → workers idle or too few |
| **parallelism %** | work ÷ wall-clock | ≪ 100% with runnable tasks → add a worker |
| **critical path** | longest dependent chain of tasks | wall-clock ≈ critical path → more workers CANNOT help; restructure the plan into independent slices instead |
| "more workers could save up to" | wall-clock − critical path | > minutes → raise `RIG_WORKERS` |
| **retry tax** | time in attempts that did not land | high → read those tasks' verify outputs; usually a plan or environment fix, not a scaling fix |
| **first pass** | tasks landing on attempt 1 | low → plan quality (see plan-writing.md) |
| queue-wait p50/max | time tasks sat ready | high with idle workers → check lease/claim issues, not scaling |

## Workers

```sh
# in ~/.factory/<rig>/compose.env:
RIG_WORKERS=2
$R up -d            # compose scales the worker service (deploy.replicas)
```

Safe by construction: the ledger's atomic claim keeps replicas off the same task, worktrees
are per task branch, the shared cache volume is safe under the package managers' own
locking. Each replica is its own agent (`worker-<container>`). Measure again after raising
it — a second worker that only adds queue-wait means the critical path was the limit.

Mixed harnesses scale independently: `$R up -d worker-opencode`,
`$R --profile codex up -d worker-codex`, `$R up -d --scale worker=3`.

## Models and effort

Plan strong, work cheap. Per rig (`rig.env`) or per invocation:

- `RIG_PLANNER_MODEL` / `RIG_PLANNER_EFFORT` — decomposition quality compounds; a weak plan
  wastes every downstream session.
- `RIG_WORKER_MODEL` / `RIG_WORKER_EFFORT` — most tasks are small; raise effort only where
  retry tax shows the cheap setting failing.
- Effort: `low | medium | high | max` (mapped per harness: claude `--effort`, OpenCode
  variant, Codex reasoning effort).

## Budgets and cost

- Per task: tokens / wall-clock / attempts, default **400k / 45 min / 3**. `RIG_TASK_TOKENS`
  raises the token budget the planner writes; the worker's `--max-budget-usd` (claude, default 5)
  and the planner's `--max-budget-usd` (default 2) cap spend per session.
- Rig-level caps for the console's display live in the console registry (`max_tokens`,
  `max_usd_micros` per `[[rig]]`).
- The lifetime totals on the rig page (tokens, work, retry tax, first-pass rate) are the
  cost dashboard; retry tax is literally money burned on attempts that did not land.
- Provider-side limits are part of the budget story too (`docs/SECURITY.md`): the rig cannot
  cap what the account allows.

## Scaling the host

- More rigs, not bigger rigs: one rig per repository is the unit. Each brings ~6 containers
  + volumes; ledgers are lightweight but Dolt servers add up — watch host RAM
  (each rig's services carry mem limits in compose.yaml; sum them before adding rigs).
- Run one rig's workers at a time when providers rate-limit; `factory rig stop` between
  phases costs nothing (history stays).
- The console scales by reading ledgers directly; it needs no rig running.
