---
name: factory-operator
description: Operate a running software factory — write and submit good plans, watch epics, act on incidents, scale workers, rebuild/upgrade/restart, back up and restore, and troubleshoot rigs. Use when asked to manage a factory, submit a plan or epic, handle an incident or "needs you" item, scale or tune a rig, upgrade or restart factory containers, or debug why a rig is stuck.
---

# Factory operator — run the factory day to day

The factory turns a prose plan into an epic of small verified tasks, lands each on the
feature branch, and asks a human only when it is stuck. Your job as operator is five verbs:
**plan → watch → resolve → review → stop**. Everything else here (scaling, upgrades,
backups, troubleshooting) keeps that loop healthy.

**Sources of truth**: `docs/DEPLOYMENT.md` · `docs/guides/first-project.md` ·
`docs/RELIABILITY.md` (failure table + restore drill) · `docs/generated/cli.md` ·
`docs/generated/console-api.md`. When this skill and those disagree, they win.

## Where everything is

- Host: this repository (compose file, images) + `~/.factory` (registry, per-rig
  `compose.env` + `rig.env`, `console/` config, `secrets/`).
- Handles: `R="docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml"`
  — every per-rig command is `$R <verb>`. The console is its own project:
  `-p factory-console --env-file ~/.factory/console/compose.env -f ~/.factory/console/compose.yaml`.
- Remote (token in hand): `export FACTORY_RIG=https://<console>/rigs/<rig> FACTORY_TOKEN=…`
  then `factory doctor|watch|inbox|plan|stop|metrics` work from anywhere.

## The operating loop

1. **Plan** — write the epic per `references/plan-writing.md` (this is the highest-leverage
   document in the skill; a good plan is most of a good run). Submit from the console's Plan
   form, `factory plan --text/--file`, or the A2A API. Gate phases across rigs with
   **After** / `--after rig:epic` — the request waits, then receives the upstream contracts.
2. **Watch** — console rig page (cards, live feed, epic tables, task drawer) or
   `factory watch [--interval 30]`. The header badge answers "does anything need me?".
3. **Resolve** — incidents and questions: read the **last verify output first**, then pick
   Resume-from-branch / Retry / Retry-with-guidance / Re-plan / Stop. Guidance notes are the
   most productive lever. Full decision table: `references/troubleshooting.md` §Incidents.
4. **Review** — an epic closing means verified and landed, not read. Review the branch like a
   colleague's: verify beads first, then that tests were *added*, then docs against code.
   Wanted changes go back through the factory as a new plan slice, not hand edits.
5. **Stop / gate** — `factory rig stop <rig>` between phases (roles down, ledger up: history
   stays readable in the console). `factory stop <epic>` cancels one epic.

## Scaling and tuning

Never guess: `factory metrics [--epic <id>]` gives per-stage p50/max, wall-clock vs work,
the critical path, retry tax, and peak live sessions. "More workers could save up to" =
wall-clock − critical path; when it says a second worker pays, set `RIG_WORKERS=2` in the
rig's `compose.env` and `$R up -d`. Models and effort are per role (plan strong, work
cheap): `RIG_PLANNER_MODEL/EFFORT`, `RIG_WORKER_MODEL/EFFORT`. Budgets: `RIG_TASK_TOKENS`,
worker `--max-budget-usd`. Details: `references/scaling-and-metrics.md`.

## Upgrades, backups, lifecycle

Rebuild/upgrade (the UI and binaries are **baked into the images** — a restart without a
rebuild changes nothing), image hygiene, backup/restore + the quarterly drill, credential
and token rotation, teardown: `references/operations.md`. The non-obvious rule from a real
run: recreate only what was running — a stopped rig's workers must not come back as a side
effect of an upgrade.

## When it breaks

`references/troubleshooting.md` — the diagnostic ladder (doctor → watch/inbox → logs →
events.jsonl → ledger) and the symptom table (environment incidents, noexec `/tmp`, token
mis-counting, Playwright downloads, 401s, stale plan containers, egress denials, lease
storms, merge conflicts vs incidents).

## References

- `references/plan-writing.md` — how to write plans the planner turns into good epics.
- `references/operations.md` — start/stop, upgrade+rebuild+restart, backup/restore, rotation, teardown.
- `references/scaling-and-metrics.md` — reading the throughput report; workers, models, effort, budgets, cost.
- `references/troubleshooting.md` — diagnostic ladder, symptom table, incident playbook.
