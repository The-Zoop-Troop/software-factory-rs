# Exec plan: proven gates

- **Status:** active · **Owner:** human steers, agents execute · **Started:** 2026-11-02
- **Beads epic:** `fac-8xl`
- **Baseline tag:** `PRE_PROVEN_GATES` (`c263883`)
- **Design:** `docs/design-docs/proven-gates.md`

## Goal

Know how often each gate catches bad work, close the routes by which a Worker can alter what judges
it, and let measured catch rates decide how much runs without a human.

## Non-goals

- Any change to the lab project's real repository until a setup is proven.
- Task data, audit labels or per-task results in this repository. This repo is public and the lab
  project is private, so only aggregate numbers come back here.
- Treating these outcomes as predictions. The approach itself is unproven, and a stop rule may end
  it early.

## Phases

| Phase | What changes | Why | Expected outcome |
|---|---|---|---|
| 0. Green `main` | Fix formatting and stale docs, update harness pins, alert on red `main` | A broken baseline proves nothing | All three workflows pass |
| 1. Baseline | Run the unchanged factory on about 30 past tasks with three Worker setups, grade with held-back tests, audit 30 diffs blind | "Before" numbers are needed before gates change | Pass rate, cost per accepted result and audit verdict per setup |
| 2. Close holes | Repo root last on `PATH`, prepare commands from the base, scrubbed harness environment, git token for the Integrator only, Worker sessions kept off the ledger or caught changing it, typed event for test edits | The Worker can alter what judges it | A test proves each hole closed |
| 3. Checks that can fail | Split `verify` into `checks` and `guards`, add hidden checks on a volume only the Verifier mounts | Only about 11 of 69 past tasks had a check known to fail first | A vacuous check cannot start a task; fewer diffs pass checks yet fail the audit |
| 4. Shadow reviewer | Narrow reviewer: code assembles diff and requirements, the model returns one structured verdict, code checks its citations | Tests miss requirement failures | Catch and false-alarm rates per model; it blocks only if it earns that |
| 5. Evidence and lab | Gate results and cost in events, cached tokens counted separately, planted faults and impossible tasks in a lab profile the Integrator refuses | A gate needs proof before it is trusted | Each gate has a catch rate with a bound |
| 6. Autonomy policy | Change classes, lanes, a policy file read from the integration branch, an approval wait, an audit queue; the default reproduces today | Every change gets the same end-of-branch review | One low-risk class lands without a human, on evidence |
| 7. Replay | Frozen tasks for removal tests and model swaps; test moving routine steps out of Worker sessions | Models disappear and harnesses change | A swap produces before-and-after numbers |

Stop rules and the definition of proven live in the design doc.

## Phase 1 lab

The lab project is a private TypeScript service with a public API contract. A clean clone
type-checked and passed 168 of 168 tests in 98 seconds, with no network and a throwaway Postgres.
Its browser tests have not been run in the lab. It has 61 commits that pair code with tests, 48 of
which change at most 400 source lines.

- **Isolation:** a separate clone on the host with no remote. Each run gets a fresh rig state seeded
  from the parent of the original commit, as one commit with no later history, so a Worker cannot
  read the answer from git. `docker/entrypoint.sh` skips its clone when the repo already exists, and
  an unset `RIG_REMOTE` keeps a rig local. This path has not yet run in a live rig; the first smoke
  test covers it.
- **Egress:** model hosts and the package registry only. The clone's tracked `.beads` sync setting
  is removed.
- **Grading:** the original commit's tests are held back and run after landing, outside the factory,
  in a no-network container.
- **Audit:** 30 diffs reviewed blind, mixing all three setups with commits already accepted as
  controls. Two questions each: is it wrong or does it need reverting, and would you have asked for
  changes?
- **Mutation testing:** Stryker in the lab image only, on a sample, against the tests relevant to
  each task, because a full run takes 98 seconds per mutant.
- **Calibration:** 5 tasks per setup first, then a cost report to the human before spending more. A
  setup that lands 0 or 1 of the 5 stops there.

| Setup | Planner | Worker | Harness |
|---|---|---|---|
| A | Opus 5.5, high effort | Sonnet 5.5, medium effort | Claude Code |
| B | Opus 5.5, high effort | Haiku 5.5, medium effort | Claude Code |
| C | Opus 5.5, high effort | `Qwen3.8-27B`, OpenAI-compatible provider | OpenCode |

The Planner runs once per task, so all three Workers get identical tasks. Setup C changes the harness
and the model together, so a difference there could come from either. A probe on 2026-11-02 ran all
three Claude model IDs through Claude Code 2.1.316; `Qwen3.8-27B` has not yet been called.

## Budgets during the lab

The token budget is checked only after a failed verify, a failed merge or a release
(`crates/domain/src/task.rs:376`). It never stops a running session or a pass. Claude's count also
includes cache reads at full weight (`crates/infra/src/claude.rs:98`), and a one-word probe already
reports about 7,000 of them, so a Claude Worker could lose retries that an OpenCode Worker keeps.
`RIG_TASK_TOKENS` is therefore set high for the baseline, and phase 5 fixes the count the way the
Codex adapter already does (`crates/infra/src/codex.rs:151`).

The stops that apply during a session are 45 minutes of wall clock per task, 200 turns, and $5 per
session on Claude only. Events do not yet record model, harness or cost per attempt, so the baseline
keeps a manifest beside its results until phase 5 adds them.

## Decision log

- **2026-11-02: the original test project was retired.** Its 68 landed tasks stay as the static
  evidence in the design doc, but the executed sweep and audit move to the new lab project.
- **2026-11-02: a separate clone, not a git worktree.** The human asked for no changes to the real
  repository and first suggested a worktree. A worktree shares `.git` with the real repository, so
  its branches and commits would land there and `core.hooksPath` would run the real hooks. The
  factory still makes per-task worktrees, inside its own volume.
- **2026-11-02: worker models.** The human chose Sonnet 5.5, Haiku 5.5 and `Qwen3.8-27B` for Workers
  and Opus 5.5 at high effort for the Planner.
- **2026-11-02: subscription and API key are both options, chosen per rig with `RIG_AUTH`.** Left to
  the variables present, Claude Code prefers an API key and the Codex login prefers a subscription,
  so one env file could bill the wrong account without saying so. A rig holding both kinds and no
  choice refuses to start, and each baseline manifest records the mode. A subscription shares limits
  with the human's own work, so calibration also measures how much of them a run uses.
- **2026-11-02: a doc goes stale only when code has changed since it was verified.** Age alone made
  `gardening` go red on a schedule with nothing wrong, and a red that means nothing teaches people to
  ignore red. The check does not map docs to the code they describe, so any non-prose commit counts.
  It needs full git history, and refuses a shallow clone, which would read as "nothing changed".
- **2026-11-02: phases 0–3 are ordinary pull requests, one per phase, merged by the human.** They
  change the factory's own gates, so the factory does not build them. Dogfooding starts at phase 5.
- **2026-11-02: the Planner's spend cap is raised from $2 to $5 per plan** with the existing
  `--max-budget-usd` flag. At that cap, 30 plans could reach $150, so calibration comes first.
- **2026-11-02: "proven" was defined before any results exist.** See the design doc. The thresholds
  are judgment and change only with a recorded reason.
- **2026-11-02: the reviewer stays narrow, and phase 7 tests moving routine steps out of Workers.**
  Both come from outside work showing large token savings when deterministic code takes over
  bookkeeping. Neither is adopted on that evidence; each is scored here.
