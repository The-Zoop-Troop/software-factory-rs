# The rig sandbox

- **Status:** accepted · **Verified:** 2026-11-02 against `compose.yaml`, `docker/entrypoint.sh` and the code; in-container acceptance last run 2026-08-28 (uid, CapEff, proxy allow/deny, no direct route)

- One rootless Docker container per project: `factory-rig:base` (git, make, `bd`, `claude`, `opencode`, `codex`, agent-ergonomics tools; uid 10001) plus a runtime layer per language (`docs/references/runtimes.md`); `cap_drop: ALL`; `no-new-privileges`; pids/mem/cpu limits. Runtimes never relax these; conformance asserts `CapEff` is zero in every image.
- Network `rig` is `internal: true`; only `egress` (tinyproxy, `FilterDefaultDeny`) bridges out, with a domain allowlist in `docker/egress/allowlist`.
- Volumes: `ledger` (`/work/rig`: `.beads`, `.factory`), `repo` (`/work/rig/repo`) and `cache` (`/work/cache`). Nothing from the host is mounted into the roles; the rig-local console bind-mounts `./docker/console`. Every role mounts all three read-write, so a Worker session can reach the ledger (`fac-8xl.3.1`).
- Build contexts are allowlists (`.dockerignore`, `docker/.dockerignore`): only `Cargo.*`, the toolchain/lint configs, `crates/` and the entrypoint reach the daemon — never `target/`, `.git`, ledgers or `docker/rig.env`.
- Credentials arrive as env from `docker/rig.env` (gitignored) at start; OpenCode's provider config is generated from env by the entrypoint and reads the key via `{env:…}`.
- Roles are compose services sharing the image: `steward`, `verifier`, `integrator`, `worker`, `worker-opencode`, `worker-codex` (profile), one-shots `plan`, `shell`.
- Threat model and what the rig does *not* protect: `docs/SECURITY.md`.

## Verification prepare step

The Verifier checks a task's branch out fresh, so a project's dependencies are not there. Before
the verify commands it runs `git submodule update --init --recursive` when `.gitmodules` exists
(the platform sweep's pointer checks ran in empty submodule dirs until this), then
`[verify] prepare = [...]` from `.factory/runtime.toml` in the candidate's worktree, or — when that
key is absent — one default chosen by the lockfile present: `npm ci`,
`pnpm install --frozen-lockfile`, `yarn install --frozen-lockfile`, `go mod download` (nothing for
Rust). `prepare = []` disables the declared and default commands, not the submodule update. Its
output lands in the task's note like any verify command.

## Services

`ledger` (the rig's Dolt SQL server, first up, health-checked), `egress` (the only route out),
then the roles: `steward`, `verifier`, `integrator`, `worker`, `planner`; optional profiles add
a rig-local `console`, `postgres`, `redis`, an OpenCode worker, and a chat bridge. Roles wait
for `ledger` to be healthy. `factory rig stop` takes `steward`, `verifier`, `integrator`, `worker`, `planner` and egress down
and leaves `ledger` up, so a stopped rig's history stays readable; only `rig destroy` takes it down.
It does not yet stop `worker-opencode` or `worker-codex` (`fac-8xl.2.1`). `RIG_WORKERS=N` (compose env) runs N worker replicas; the ledger's atomic claim keeps them
from taking the same task (the lease write alone is last-writer-wins and once raced two
workers onto one task), worktrees are per task branch, and the shared cache volume is
safe under cargo's own locking. The throughput report says whether a second worker pays:
wall-clock minus critical path is the most it can save. See `ledger.md` for why the server exists.
