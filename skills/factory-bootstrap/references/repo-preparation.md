# Preparing a project repository for its rig

Do this once per repository, **on the feature branch the factory will land on**, and push it.

## 1. `.factory/` — the project's contract with the rig

```sh
mkdir .factory
cat > .factory/runtime.toml <<'TOML'
[runtime]
name = "rust"        # rust | python | node | go | jvm | c-cpp | ruby | php | elixir | web-e2e | polyglot

[[tools]]
bin = "cargo"        # what tasks will call; `factory doctor` checks it exists in the image

# [verify]
# prepare = ["pnpm install --frozen-lockfile"]   # optional; see below
TOML
printf '# extra egress hosts this repo needs (registries come with the runtime)\n' > .factory/allowlist
git add .factory && git commit -m "chore: software-factory rig config" && git push origin <feature-branch>
```

- **`[verify] prepare`** — commands run in the fresh checkout before every verify. Omit it
  and the Verifier infers from the lockfile (`npm ci`, frozen pnpm/yarn, `go mod download`;
  nothing for Rust) and initialises submodules when `.gitmodules` exists. `prepare = []`
  disables it.
- **`.factory/allowlist`** — extra egress hosts; assembled into the proxy allowlist by
  `docker/build.sh <runtime> --project <dir>`.
- **`.factory/Dockerfile`** (optional) — `ARG BASE` / `FROM ${BASE}`; adds project tools on
  top of the runtime image. The sandbox is unchanged (uid 10001, zero capabilities).
- **`.factory/mcp.json`** (optional) — MCP servers, rendered per harness automatically.
  Remote hosts must be in the allowlist.
- **Harness skills** (optional) — `.claude/skills`, `.codex/skills`, `.opencode/skills`,
  `.agents/skills` in the project repo travel into every worker session's worktree. Ship the
  project's coding standard as a skill; `doctor` reports what it sees.

## 2. The verify command is the definition of done

Decide, per repository, the command every task must leave passing (`cargo test`,
`go test ./...`, `npm test`…). If the repo has no tests, **the first epic adds a test
runner** — the factory's "done" is a passing verify command; a build alone proves little.
Verify commands run via `/bin/sh` from the repo root with the repo root on `PATH`, in a fresh
worktree.

## 3. Branch discipline

- Create the feature branch (`RIG_MAIN`) on the remote before the rig starts; the rig clones
  that branch.
- `RIG_PROTECTED_BRANCHES` (default `main,master`): the Integrator refuses to run when
  `RIG_MAIN` is listed. Landing on a feature branch is configuration; landing on `main` is a
  deliberate override.
- Protect `main` on the remote as well, so no bug can reach it either.

## 4. The git credential

- One **fine-grained token per repository**: *Contents: read/write*, nothing else. The rig
  never sees SSH keys.
- A fine-grained token has one resource owner — repositories across two organisations need
  two tokens (and therefore separate rigs or `RIG_GIT_TOKEN` values).
- The rig applies the token as a git URL rewrite: it never appears in `RIG_REPO_URL`,
  `.gitmodules`, or a log line, and SSH-form submodule URLs authenticate through it too.
- Submodules: set `RIG_SUBMODULES=1` on the parent repo's rig; nothing else is needed.

## 5. Runtime selection traps

- Frontend repos whose tests run a real browser need `--runtime web-e2e`. It ships Chromium
  for **one pinned Playwright version** — say so in the epic ("pin `playwright` to the
  preinstalled version; never run `playwright install`"); rigs cannot download browsers.
- Go builds execute test binaries; the rig's `/tmp` is noexec — the go runtime image sets
  `GOTMPDIR` onto the cache volume. Anything else that must execute from `/tmp` has the same
  class of problem: put it on `/work/cache`.
- Language versions in apt-sourced images (jvm, ruby, php, elixir) follow Debian bookworm;
  need newer → `.factory/Dockerfile`.

## 6. Plans live in the parent repo

Write the execution plan into the (parent) repository's `docs/exec-plans/active/` — which
repos, phase order, verify command per repo, what external systems agents will NOT have, and
a progress list. It is the decision log for the run and part of the final doc sweep.

## Checklist (per repository)

- [ ] `.factory/runtime.toml` committed on the feature branch, runtime chosen by verification needs
- [ ] verify command exists and passes locally; repo with no tests → first epic adds them
- [ ] feature branch pushed; `main` branch-protected on the remote
- [ ] fine-grained token minted (this repo only, Contents r/w), stored in the rig's secrets file
- [ ] `.factory/allowlist` lists any extra egress hosts the build/tests genuinely need
- [ ] submodule parent: `RIG_SUBMODULES=1`
- [ ] datastore needed by tests → `postgres`/`redis` profile + `DATABASE_URL` in rig env
