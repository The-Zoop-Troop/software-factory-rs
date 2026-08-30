# Guide: your first project through the factory

- **Status:** draft · **Verified:** in progress (a multi-repo feature run, 2026-08-30)

This walks one real change through the factory end to end: several repositories, a feature
branch that must never touch `main`, tests as the definition of done, and an operator who only
uses the browser console. Every command is copy-pasteable; placeholders are in `<angle brackets>`.

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
them up one at a time. Each rig is its own compose project (`factory-<rig>`) with its own
`ledger`, `repo` and `cache` volumes; the shared console is generated into `~/.factory/console/`.
A rig whose repository needs a database gets `DATABASE_URL=postgres://factory:factory@postgres:5432/factory`
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

**Phase gating across rigs** (until cross-rig dependencies exist): submit the next phase's epic
only when the epic it depends on reads *done* on its rig page, and paste the landed contract (the
API shapes, error types, env vars the upstream epic produced) into the next epic's text. Keep the
order in your exec plan so anyone can see what is waiting on what.

## 5. Watch
## 6. Act on incidents
## 7. Review what landed
## 8. The end-state sweep and teardown