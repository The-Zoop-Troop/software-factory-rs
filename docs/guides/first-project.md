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
## 4. Write and submit the epics
## 5. Watch
## 6. Act on incidents
## 7. Review what landed
## 8. The end-state sweep and teardown