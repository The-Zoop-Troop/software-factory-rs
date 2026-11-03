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

**Tasks.** 41 of those change at most 400 source lines and have non-browser tests. In a no-network
container, 36 passed all three checks: the commit's tests fail on its parent, pass on the commit, and
the parent's whole suite is green. All 36 passed two more runs. 30 were selected, leaving out the one
whose parent carries the project's own `bd` setup and five near-duplicates from one feature family.
The first 5 tasks for calibration are the ones at the 10th, 30th, 50th, 70th and 90th percentiles of
size, so the choice involves no judgment about which tasks look easy.

**Task text.** Each task is the original commit message, with attribution and test-count lines
removed, plus an acceptance contract written from the held-back tests alone: endpoints, status codes,
field names and exact error codes, with no test code and no implementation hints.

- **Isolation:** a separate clone on the host with no remote. Each run gets a fresh rig state seeded
  from the parent of the original commit, as one commit with no later history, so a Worker cannot
  read the answer from git. `docker/entrypoint.sh` skips its clone when the repo already exists, and
  an unset `RIG_REMOTE` keeps a rig local. A live rig confirmed this on 2026-11-02, along with the
  egress list, the database sidecar, `RIG_AUTH`, and `rig stop` on every Worker profile.
- **Known exposure:** `bd` run from a worktree reaches the factory's own ledger. This was confirmed
  live and phase 2 closes it. The baseline keeps each run's bead dump so tampering can be checked.
- **Egress:** model hosts and the package registry only. The clone's tracked `.beads` sync setting
  is removed.
- **Grading:** the original commit's tests are held back and run after landing, outside the factory,
  in a no-network container, with its test files and test runner replacing whatever the Worker left.
  Browser tests are not run, so UI work is judged only by the audit.
- **Pipeline test:** a stand-in model ran the whole path at no cost, on two tasks. The original diff
  landed and passed. A harmless diff landed and failed grading, which is the escape the baseline
  exists to count. No change reached an incident after three attempts. A plan limit charged nothing,
  and the retry landed with no partial work.
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
and the model together, so a difference there could come from either.

## Calibration results

Five tasks ran through all three setups on 2026-11-02: 15 runs in 2 hours 22 minutes, one setup per
rig and three rigs at a time. No setup reached the stop rule.

| Setup | Landed | Passed held-back tests | Failed verifies | Worker cost at API prices | Worker time |
|---|---|---|---|---|---|
| A | 5 of 5 | 5 of 5 | 0 of 11 | $2.53 | 25 minutes |
| B | 5 of 5 | 5 of 5 | 0 of 11 | $0.27 | 48 minutes |
| C | 5 of 5 | 5 of 5 | 1 of 12 | $30.18 on 4 tasks, at list price | 1 hour 55 minutes |

Planning cost $6.13, which is 69% of the $8.93 the Claude setups spent at API prices. On the
subscription, the weekly limit read 6% before and 7% after, in whole percents and shared with the
human's other work.

The human chose Alibaba Cloud's international list price for setup C: $0.50 per million input tokens
and $3.00 per million output. C's token records were kept for 4 of the 5 tasks. On those 4, C cost
$30.18, about 15 times what A cost on the same tasks, although its list price is lower. The provider
reported no cache hits, so every request's whole context was charged in full, while Claude reads most
of its context from cache. A cheaper C needs caching more than a cheaper model. Every
run also passed the whole suite, and the session logs confirm each setup used its intended model.

**What this does not show.** Zero escapes says little about the gates, because only one bad submission
reached a Verifier in 34 checks. The whole suite caught it, with real 500 responses from a broken
query, and the retry fixed it. Three things make the high pass rate expected:

- The contract tells each Worker what the held-back tests check.
- Only 11 of the 38 held-back tests that ran on a task's parent failed there, so the grade rests on
  few tests that measure the new behaviour.
- Of the 70 verify commands the Planner wrote, 43 only check that a file or text exists, 12 type-check,
  7 run the whole suite, 4 run one named test file, and 4 build or list things. That is the same
  pattern the design doc found in the old project's history.

Evidence about the gates has to come from planted faults and the blind audit.

**Other findings.**

- **Size:** Workers added more lines than the original commits did: A 1.6 times, B 2.4 times, C 3.3
  times. On one task, C changed about 700 non-whitespace lines of a central file where A changed under
  20. Tests cannot see this, so it is a question for the audit.
- **Tampering:** none found. After every run, each Planner bead's title, description, acceptance text
  and verify commands were unchanged, and no Worker created a bead. C's 351 captured shell commands
  include no `bd` call. Claude Workers left no transcript, so their read-only `bd` use is unknown.
- **Idle time:** two B sessions took about 12.5 minutes with under 90 seconds of API time. The cause
  is unknown for the same reason.
- **Token counts:** the factory's event log undercounts both harnesses. OpenCode's count covers only
  each session's final message: C's real use on 4 of the tasks was 59.2M input and 189K output tokens,
  against about 700K logged, with no cache reads reported. Claude's envelope can cover only the last
  segment of a session, though its cost and `modelUsage` are cumulative. The real counts come from
  each harness's own records until phase 5 fixes this.
- **A harmless race:** a steward sweep opened a second merge bead while a Verifier was finishing a
  pass, in 1 of 34 checks. The Integrator landed one and closed the other as stale.

## Budgets during the lab

The token budget is checked only after a failed verify, a failed merge or a release
(`crates/domain/src/task.rs:376`). It never stops a running session or a pass. Claude's count also
includes cache reads at full weight (`crates/infra/src/claude.rs:98`), and a one-word probe already
reports about 7,000 of them, so a Claude Worker could lose retries that an OpenCode Worker keeps.
`RIG_TASK_TOKENS` is therefore set high for the baseline, and phase 5 fixes the count the way the
Codex adapter already does (`crates/infra/src/codex.rs:151`).

The stops that apply during a session are 45 minutes of wall clock per task, 200 turns, and $5 per
session on Claude only. Events do not yet record model, harness or cost per attempt, so the baseline
keeps a manifest beside its results until phase 5 adds them. Claude cost comes from a logger around
`claude` in a lab-only image, because the factory drops it; OpenCode has only total tokens, so its
cost is tokens times the provider's price.

## Decision log

- **2026-11-02: lab rigs get only a short-lived access token from the human's login.** The runner
  removes the refresh token, so a rig cannot rotate that login out from under the human, and anything
  a rig leaks expires soon. It reads the login fresh at each rig start and waits when too little time
  is left. A rejected credential sets that run aside and runs it again once.
- **2026-11-02: the lab logger keeps Claude transcripts.** The factory passes
  `--no-session-persistence`; calibration then left two idle sessions and any read-only `bd` use
  unexplained. The lab logger drops the flag from the remaining runs, and the factory's own behaviour
  does not change.
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
- **2026-11-02: a provider refusing a session charges the task nothing.** Otherwise a used-up
  subscription, a reached spend limit or an outage looks like model failure and corrupts the baseline.
  A partial session is discarded rather than verified, so every verified result comes from one
  uninterrupted session, and its tokens are logged so cost per accepted result still counts them.
  The behaviour was captured against a stub provider, not a real limit. A provider that never
  recovers waits forever instead of reaching an incident; a refused request costs nothing, so the
  price of that is a quiet rig, visible in the event log.
- **2026-11-02: tasks carry a contract written from the held-back tests.** Grading a Worker on names
  it was never told would measure guessing, and 14 of the 30 commits have no message body at all. The
  contract raises pass rates for every setup alike, so the comparison stays fair, but those rates
  should not be read as what the same models would do from a thin ticket. A subagent wrote the
  contracts from the tests only, without reading the original source changes.
- **2026-11-02: the lab runner stays on the host.** It names the private project, and phase 5's lab
  profile replaces it. The cost is that this repository alone cannot reproduce the baseline.
- **2026-11-02: phases 0–3 are ordinary pull requests, one per phase, merged by the human.** They
  change the factory's own gates, so the factory does not build them. Dogfooding starts at phase 5.
- **2026-11-02: the Planner's spend cap is raised from $2 to $5 per plan** with the existing
  `--max-budget-usd` flag. At that cap, 30 plans could reach $150, so calibration comes first.
- **2026-11-02: "proven" was defined before any results exist.** See the design doc. The thresholds
  are judgment and change only with a recorded reason.
- **2026-11-02: the reviewer stays narrow, and phase 7 tests moving routine steps out of Workers.**
  Both come from outside work showing large token savings when deterministic code takes over
  bookkeeping. Neither is adopted on that evidence; each is scored here.
