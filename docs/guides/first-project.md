# Guide: your first project through the factory

- **Status:** accepted · **Verified:** multi-repo feature run completed 2026-08-31 (six phases landed; incidents and timings below are from that run)

This walks one real change through the factory end to end: several repositories, a feature
branch that must never touch `main`, tests as the definition of done, and an operator who only
uses the browser console. Every command is copy-pasteable; placeholders are in `<angle brackets>`.

**Where this fits:** the [README quick start](../../README.md#quick-start) is the five-minute
single-rig path; [`docs/DEPLOYMENT.md`](../DEPLOYMENT.md) is the operational runbook; this
guide is the worked end-to-end example. The same workflows are packaged for agents as skills
— [`skills/factory-bootstrap`](../../skills/factory-bootstrap/SKILL.md) (host → running rigs,
with a verification gate per step) and
[`skills/factory-operator`](../../skills/factory-operator/SKILL.md) (plans, incidents,
scaling, upgrades, troubleshooting) — so a Claude/Codex/OpenCode session on the host can run
this guide instead of you.

## 0. What you need before you start

- Repositories the factory may **clone and push branches to**, each with a feature branch
  (`<feature-branch>`) checked out on the remote. `main` stays untouched: the rig refuses to land
  on a protected branch (`RIG_PROTECTED_BRANCHES`, default `main,master`) — protect `main` on the
  remote too.
- A credential scoped to exactly those repositories (a fine-grained GitHub token: *Contents:
  read/write*, nothing else). The rig never sees your SSH keys.
- A harness credential (Codex, Claude Code or an OpenAI-compatible provider for OpenCode).
- A verify command per repository that proves the change works (tests, not just a build). If a
  repository has no tests, the first epic adds them.
- A plan in prose: what to build, in phases, with acceptance criteria you would check yourself.

The example run: a feature that spans a Rust API, three Lit/Vite frontends, a Go service and a
parent repository that carries the docs and the four frontends/backend as git submodules. One
feature branch everywhere, one console, one rig per repository, run one rig at a time.

## 1. Prepare each repository

Do this once per repository, on the feature branch, and push it:

```sh
mkdir .factory
# `[verify] prepare` is optional: without it the Verifier runs the install your lockfile implies
# (`npm ci`, frozen pnpm/yarn, `go mod download`) before every verify.
cat > .factory/runtime.toml <<'EOF'
[runtime]
name = "rust"            # rust | python | node | go | jvm | c-cpp | ruby | php | elixir | web-e2e | polyglot

[[tools]]
bin = "cargo"            # what the tasks will call; `factory doctor` checks it exists in the image
EOF
printf '# extra egress hosts this repo needs (package registries come with the runtime)\n' > .factory/allowlist
git add .factory && git commit -m "chore: software-factory rig config" && git push origin <feature-branch>
```

Also decide, per repository, the **verify command** every task must pass (`cargo test`,
`go test ./...`, `npm test`…). If a repository has no tests, the first epic on it adds a test
runner — the factory's "done" is a passing verify command, and a build alone proves little.

For a parent repository with submodules, nothing extra is needed: the rig clones with
`RIG_SUBMODULES=1` and the token rewrites the submodules' SSH URLs at fetch time.

Write the execution plan **into the parent repository's docs** (its `docs/exec-plans/active/` directory, one file per feature):
which repos, which phase order, the verify command per repo, what external systems agents will
*not* have (databases other than the rig's throwaway one, payment/registry daemons, vendors), and a
progress list. It becomes the decision log for the run and part of the final doc sweep.

## 2. Create the rigs

One secrets file per rig, under a directory only you can read. Everything a rig may do is in it:

```sh
mkdir -p ~/.factory/secrets && chmod 700 ~/.factory/secrets
cat > ~/.factory/secrets/<rig>.env <<'EOF'
CODEX_AUTH_JSON=<base64 of ~/.codex/auth.json>   # or ANTHROPIC/CLAUDE_* / OPENCODE_* for the other harnesses
RIG_HARNESS=codex
CODEX_MODEL=<model>
RIG_EFFORT=medium                                # low | medium | high | max
RIG_PREFIX=<short-prefix>                        # bead ids on this rig
RIG_REPO_URL=https://github.com/<org>/<repo>.git
RIG_MAIN=<feature-branch>                        # the only branch the factory lands on
RIG_PROTECTED_BRANCHES=main,master               # the Integrator refuses these
RIG_GIT_TOKEN=<fine-grained token: this repo, contents read/write>
RIG_SUBMODULES=0                                 # 1 for the parent repo
EOF
chmod 600 ~/.factory/secrets/<rig>.env

# from the factory repository (it supplies the compose file)
factory rig create <rig> --repo-url https://github.com/<org>/<repo>.git \
  --runtime <runtime> --harness codex --main <feature-branch> \
  --secrets ~/.factory/secrets/<rig>.env --no-start
factory rig list
```

`--no-start` registers the rig without running it, so you can create all of them now and bring
them up one at a time.

Pick the runtime by what *verification* needs, not just the language: a frontend whose tests
run in a real browser (Vitest browser mode, Playwright) needs `--runtime web-e2e`, which ships
Chromium for one pinned Playwright version — rigs cannot download browsers. Say so in the epic
(or a reference bead): "pin `playwright` to the preinstalled version; never run `playwright
install`". This run lost one attempt learning that on the portal rig. Each rig is its own compose project (`factory-<rig>`) with its own
`ledger`, `repo` and `cache` volumes; the shared console is generated into `~/.factory/console/`.
Each rig also runs a `ledger` service — its Dolt SQL server — which every role and the console
use; the first `factory rig create` on a host writes `~/.factory/ledger.password` (0600) and
puts it in each rig's `compose.env`. A rig whose repository needs a database gets `DATABASE_URL=postgres://factory:factory@postgres:5432/factory`
in its env and is started with the `postgres` profile — a throwaway database on the rig network,
migrated by the tests themselves.

Notes from the run:
- A fine-grained token has one resource owner; repositories across two organisations need two tokens.
- Tokens never appear in URLs: the rig installs a git URL rewrite for the host, so `git@…:` and
  `https://…` both authenticate, including submodule fetches.
- The console token file (`~/.factory/console/tokens.toml`) holds sha256 hashes; make one entry per
  person with the scopes they should have (`watch`, `plan`, `resolve`, `admin`) per rig.

## 3. Open the console

Start the first rig and the shared console (the console mounts every registered rig's ledger,
so start it after at least one rig has run once):

```sh
# from the factory repository
docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml up -d
docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml run --rm shell doctor
factory rig console                     # http://127.0.0.1:7700
```

`doctor` must say the repository is on the feature branch, the runtime matches
`.factory/runtime.toml`, and **only** the intended credential is present. Then open the console in
a browser, paste your token, and you should see every registered rig on the overview — stopped
rigs show "unavailable", the running one shows counts.

Give the planner the context that lives outside the repository — the decisions table and the
phase text from your plan — as **reference beads** on the rig; every worker session reads them:

```sh
docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml \
  exec -T steward sh -c 'cd /work/rig && bd create "Plan: settled decisions" -t task -p 3 \
  -l fac:kind=reference --no-inherit-labels --body-file - --json' < decisions.md
```

## 4. Write and submit the epics

One epic per phase per repository. The text is the whole contract the planner works from, so it
carries: the goal in one paragraph, the repository facts it should rely on (file paths, existing
types), an explicit **deliver** list, the **tests** that define done (name the cases), the docs to
update, and constraints (language rules, what not to touch, what external systems are absent).
Say which verify command every task must leave passing. A good size is 300–600 words; the
planner splits it into 3–8 tasks with acceptance criteria and verify commands.

Submit from the console (Plan field on the rig page) or from the API:

```sh
curl -s -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d "$(jq -cn --arg t "$(cat epic.md)" '{jsonrpc:"2.0",id:1,method:"SendMessage",
        params:{message:{messageId:"m",role:"ROLE_USER",parts:[{text:$t}]},
                configuration:{returnImmediately:true}}}')" \
  http://127.0.0.1:7700/rigs/<rig>/a2a
```

The reply is the queued request (`TASK_STATE_SUBMITTED`); the rig's `planner` service picks it up
within seconds and the request card shows its progress until the epic exists.

**Phase gating across rigs**: pick the upstream epics in the plan form's **After** field (or
`factory --rig … plan --after backend:be-1`). The request stays *waiting* on the rig page until
every one of them closes; the console then appends their **contracts** — what actually landed:
commit range, files, public surface, tasks — to the request and the rig's planner takes it. No
hand-pasting; keep the order in your exec plan so anyone can see what waits on what.

## 5. Watch

What the console shows, in the order things happen:

| Moment | Where it shows |
|---|---|
| plan queued → planner reading the repo → epic created | request card on the rig page (progress line), then the epic card |
| a task claimed | epic page → Tasks table: `leased`, "held by worker-…", attempts, tokens; Live feed line |
| the session working | *nothing yet* between `claimed` and `submitted` (see the note below) |
| task submitted → verified → landed on the feature branch | Live feed / epic Timeline: `submitted`, `verified`, `landed on main`-style lines; the epic card's counter advances |
| something needs a human | header badge, rig page **Needs you**, epic page incident panel |

While a session runs, the code is real and visible from the host if you want proof:

```sh
R="docker compose -p factory-<rig> --env-file ~/.factory/<rig>/compose.env -f compose.yaml"
$R exec worker sh -c 'git -C /work/rig/.factory/worktrees/task__<task-id> diff --stat'   # the worktree the session edits
$R logs -f worker                                                                         # harness events
$R exec steward sh -c 'cd /work/rig && factory watch'                                     # ledger view
```

Landed work is on `<feature-branch>` at the remote after every `integrated` event (one squash
commit per task, message from the task title). `main` is untouched.

Every surface above has a deeper view one click away:

- **Rig page header** — a facts card: repo, feature branch, runtime, harness, budget caps,
  ledger latency, last activity, and lifetime totals (epics, tasks landed, first-pass rate,
  tokens, work, retry tax). Backed by `GET /rigs/<rig>/detail`; a stopped rig shows
  "stopped — history only" and still answers from its ledger.
- **Task drawer** — click any row in an epic's Tasks table: branch, base, landed sha, lease
  holder, budget-vs-used meters, verify commands, a per-attempt stage strip, and the task's
  notes parsed into collapsible verify pass/fail panels (per-command exit and output tail)
  with guidance and operator interventions highlighted. Backed by
  `GET /rigs/<rig>/beads/<id>` (`?full=1` for untruncated notes). Esc closes it.
- **Epic page** — a rollup header (wall-clock, work, parallelism, first-pass, retry tax,
  tokens) with planned/closed stamps; a **Plan** section (submitted plan text, reference
  beads, and the epic's contract once closed); and **Provenance** — the plan request it came
  from, and every rig whose queued plans build on it
  (`GET /rigs/<rig>/epics/<id>/consumers`).
- **Request card** — "show plan text" expands to the full submitted text, with any injected
  upstream contracts as collapsible sections.

How long each stop took — and whether more workers would have helped — comes from the event
log, not from watching:

```sh
factory --rig https://<console>/rigs/<rig> --token $TOKEN metrics            # every epic
factory --rig … metrics --epic <id> --json                                    # one epic, machine-readable
$R exec steward sh -c 'cd /work/rig && factory metrics --csv'                 # inside the rig
```

The table gives per-stage p50/max (queue wait, session, verify wait, verify, integrate wait,
integrate), wall-clock vs. work, the critical path along task dependencies, the retry tax, and
the peak number of live sessions. "More workers could save up to" is wall-clock minus the
critical path. The same report backs `GET /rigs/<rig>/metrics?epic=` on the console, and the
epic page's **throughput →** link draws every attempt by stage. When it says a second worker
would pay, set `RIG_WORKERS=2` in the rig's `compose.env` and `up -d` again.

## 6. Act on incidents

An incident is a task the factory gave up on: verify failed three times, a merge no longer
applies, the budget ran out, a lease kept expiring, or the rig could not run the checks at
all (an **environment** incident — exit 126/127, permission denied, no space, a missing tool;
these charge no attempt). The incident panel shows the reason in
plain words, the attempts/tokens used, the branch, and the **last verify output** — read that
first; in this run both first incidents were infrastructure, not the model's code:

- `fork/exec /tmp/go-build…/runner.test: permission denied` — the rig's `/tmp` is a `noexec`
  tmpfs; Go builds test binaries there. Fixed in the go runtime image (`GOTMPDIR` on the cache
  volume). Symptom of the same class for other stacks: anything that executes from `/tmp`.
- `token budget exceeded: 9,193,403 of 400,000` after one session — the Codex CLI reports the
  cached prompt prefix as input on every turn; the adapter now counts uncached tokens only, and
  `RIG_TASK_TOKENS` raises the per-task budget written by the planner (2,000,000 for this run).

Both of those are now caught by the Verifier's environment classifier, so a repeat shows up as
an environment incident rather than a burned attempt.

Then pick an option on the panel:

- **Resume from the branch** (environment incidents) — start the next session from the task's
  own branch, keeping the commits already made; use it once the rig is fixed.
- **Retry** — reopen with fresh attempts and budget from the integration branch.
- **Retry with guidance** — same, plus a note the next session reads first ("use POSIX sh",
  "the fixture lives in tests/data") — the most productive lever when the model misread the task.
- **Re-plan** — stop the epic and queue a new plan from its goal plus your note, when the
  decomposition itself was wrong.
- **Stop the epic** — when the work is no longer wanted.

A question from an agent (`question` kind) is answered the same way; the answer is recorded on
the ledger and read by the session that asked.

A merge conflict is *not* an incident on its own: the Integrator reopens the task with the
conflicting paths in a note and the next session rebases. In this run task `.3` conflicted
with `.11` (both touched the changelog, the compose file, and the runner config) and was
re-leased within a minute.

While a session runs, the epic page's **Working** column shows what it has changed so far
(files, +/− lines, sampled on every lease heartbeat); the feed shows the same as quiet lines.

## 7. Review what landed

An epic closing means every task was verified and landed on the integration branch; it does
not mean you have read the code. Review the branch as you would a colleague's — the factory
made the diff small enough to do that:

```sh
git fetch origin
git log --oneline origin/main..origin/feat/<feature>     # one commit per task, titled by task
git diff --stat origin/main...origin/feat/<feature>
```

What the first phase of this run looked like: 5 tasks, 5 commits, first-pass verification on
3 of them, one rebase after a conflict, two infrastructure incidents that cost a retry each,
and `main` untouched (`git log -1 origin/main` still shows the pre-run commit). Read in this
order:

1. **The verify beads** (`bd show <epic>.2` etc. in the rig, or the console's task rows): the
   commands that actually ran are the contract. If a command was weaker than you wanted, fix
   the plan slice for the next phase rather than the code now.
2. **Tests added** — every task's acceptance said "tests"; check the diff adds them, not just
   passes them.
3. **Docs the task touched** — the planner puts a docs/overlay task in each epic; that is the
   one most likely to drift from the code when two tasks conflict, so read it against the code.

Anything you want changed goes back through the factory as a new plan slice ("review notes:
…") rather than a hand edit on the branch — a hand commit is fine (the Integrator fetches and
fast-forwards before every landing), but the next session rebases onto it blind. Then stop the rig (`factory rig stop <rig>` — roles down, ledger up, so the
console still shows its history; `factory rig start <rig>` later); the ledger
and repo volumes survive) and gate the next phase.

## 8. The end-state sweep and teardown

A finished epic leaves the rig page's live list, but nothing is deleted: the ledger keeps every
closed bead (usage, notes, landed commits) and the ledger volume keeps the full event log, so
the console can list a rig's history (`ListTasks` with `history: true`) and replay any epic's
timeline (`GET /rigs/<rig>/epics/<id>/events`) — even for a stopped rig, because the console
reads the volumes directly. History lives exactly as long as the ledger volume: `factory rig
backup` before `factory rig destroy --volumes`.

The sweep, before calling the feature done:

1. **Branch review complete** (§7) on every repository, and the review notes either resolved
   or queued as a follow-up plan slice.
2. **Docs sweep** — the parent repo's exec plan updated to `completed`, its progress list
   checked off, and the per-repo docs the epics touched read against the code one last time.
3. **Metrics snapshot** — `factory --rig … metrics --json > runs/<feature>-metrics.json` per
   rig; the retry-tax and first-pass numbers are the input to the next run's plan quality.
4. **Backups** — `factory rig backup <rig> --to backups/` per rig while the ledgers are still up.
5. **Stop or destroy** — `factory rig stop <rig>` keeps history browsable in the console for
   the next phase; `factory rig destroy <rig> --volumes` (after the backup) forgets it.
6. **Rotate** anything that leaked scope during the run: fine-grained git tokens you widened,
   console tokens you shared.

```sh
for r in <rig> <rig> …; do factory rig backup "$r" --to backups/ && factory rig stop "$r"; done
factory rig doctor        # every rig: ledger=yes running=[ledger]
```

The feature branch is now the deliverable: one squash commit per task, verified twice (task
verify + the Integrator's project checks), `main` untouched, and the entire decision trail —
plans, incidents, guidance, timings — replayable from the console for as long as the ledger
volumes exist.
