# Troubleshooting and debugging

## The diagnostic ladder

Work down; stop at the first rung that explains the symptom.

```sh
factory rig doctor                                   # 1. registry view: ledger? services?
$R run --rm shell doctor [--probe]                   # 2. inside: tools, ledger, repo, credentials
$R exec steward sh -c 'cd /work/rig && factory watch'      # 3. epics/tasks by state
$R exec steward sh -c 'cd /work/rig && factory inbox'      # 4. incidents + questions
$R logs -f <service>                                 # 5. role logs (RUST_LOG=info; FACTORY_LOG_FORMAT=json | jq)
$R exec steward sh -c 'tail -f /work/rig/.factory/events.jsonl'   # 6. every state transition
$R exec steward sh -c 'cd /work/rig && bd ready && bd list --status blocked'   # 7. the ledger itself
```

Watch a live session's actual work:

```sh
$R exec worker sh -c 'git -C /work/rig/.factory/worktrees/task__<task-id> diff --stat'
```

Console-side: task drawer (verify pass/fail panels with per-command exit + output tail),
epic timeline, `GET /rigs/<rig>/beads/<id>?full=1`. Console API errors and scopes:
`docs/generated/console-api.md`. UI debugging without a rig:
`cargo run -p console --features fake -- serve --fake` (token `fake`).

## Incidents: the decision table

An incident = the factory gave up: verify failed ×attempts, merge no longer applies, budget
exhausted, lease kept expiring, sessions kept declaring the task blocked (release loop),
or the checks could not run at all. **Read the last verify
output first** — real runs show first incidents are usually infrastructure, not model code.

| Choose | When |
|---|---|
| **Resume from the branch** | environment incident (exit 126/127, permission denied, no space, missing tool or interpreter module — these charge no attempt); fix the rig, keep the commits |
| **Retry with guidance / re-plan** | release-loop incident: 2 sessions declared the contract unsatisfiable — the contract, not the code, is usually what needs fixing |
| **Retry** | transient failure; fresh attempts + budget from the integration branch |
| **Retry with guidance** | the model misread the task; the note is read first next session — the most productive lever |
| **Re-plan** | the decomposition itself was wrong; stops the epic, queues a new plan from its goal + your note |
| **Stop the epic** | the work is no longer wanted |

A merge conflict is NOT an incident: the Integrator reopens the task with the conflicting
paths in a note; the next session rebases. A `question` bead is answered the same way
(`factory inbox --resolve <id> --note "…"`); the session that asked reads the answer.

## Symptom table

| Symptom | Cause | Fix |
|---|---|---|
| role logs `nothing ready` forever | tasks blocked or all closed | `bd ready` / `bd list --status blocked` in the rig; check inbox |
| harness 401 / `revoked` | stale credential in rig.env | fix the file, `$R up -d` the affected service, Resume-from-branch on fallout |
| `plan` run hangs | killed client left a `plan-run-*` container | `docker ps`, remove the stale container |
| verify: `command not found` | tool not in the runtime image | wrong runtime, or add via `.factory/Dockerfile`; `factory doctor` names the missing tool |
| `permission denied` executing from `/tmp` | rig `/tmp` is noexec tmpfs (Go test binaries, etc.) | scratch dirs belong on `/work/cache` (go image sets `GOTMPDIR`); classed as environment incident |
| token budget exceeded absurdly fast | harness reports cached prefix as input (seen with Codex) | adapter counts uncached only now; raise `RIG_TASK_TOKENS` if genuinely large |
| Playwright wants to download a browser | rigs cannot download browsers | runtime `web-e2e`; pin `playwright` to the preinstalled version in the plan |
| network failures during build/tests | egress default-deny | add the host to `.factory/allowlist` (project) or runtime fragment; rebuild with `build.sh <rt> --project <dir>`; check `$R logs egress` for denials |
| task bounces between workers | lease expiry storm — session slower than TTL | check worker logs for crashes; ≥3 expiries → incident by design |
| epic closed but branch missing commits | you fetched before integration finished | `git fetch`; one squash commit per task, `main` untouched by design |
| console shows rig "unavailable" | rig stopped (expected) or ledger down | `factory rig doctor`: `ledger=missing` → restore from backup; `running=[]` → `factory rig start` |
| console 401/403 | token wrong or scope missing | scopes are per rig per verb in tokens.toml; refusals are audited in the event log |
| whole rig gone after reboot | systemd units not installed | `restart: unless-stopped` needs the daemon up; install the user units + linger (bootstrap skill §5) |
| ledger connection refused | Dolt server not healthy | `$R ps`, `$R logs ledger`; healthcheck gates the roles — wait or restart ledger |
| disk filling | old images, dangling volumes, worktrees | `docker image prune`, check `.factory/worktrees` in the ledger volume, `docker volume ls` |

## Reliability model (what the factory already self-heals)

`docs/RELIABILITY.md` is the failure table: lease expiry → reopen; verify fail → reopen with
output; push race → compare-and-swap rollback; crash mid-transition → idempotent steward
sweep. If you are about to intervene by hand, check the table first — most transients heal
on the next pass, and manual surgery on the ledger (`bd edit`) is never the answer.

## Escalation

Reproducible factory bug → file a bead in THIS repo (`bd create`) with the events.jsonl
excerpt and the task's verify output; security-relevant → `docs/SECURITY.md` §Reporting.
