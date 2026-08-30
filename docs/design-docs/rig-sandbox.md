# The rig sandbox

- **Status:** accepted · **Verified:** in-container acceptance 2026-08-28 (uid, CapEff, proxy allow/deny, no direct route)

- One rootless Docker container per project: `factory-rig:base` (git, make, `bd`, `claude`, `opencode`, `codex`, agent-ergonomics tools; uid 10001) plus a runtime layer per language (`docs/references/runtimes.md`); `cap_drop: ALL`; `no-new-privileges`; pids/mem/cpu limits. Runtimes never relax these; conformance asserts `CapEff` is zero in every image.
- Network `rig` is `internal: true`; only `egress` (tinyproxy, `FilterDefaultDeny`) bridges out, with a domain allowlist in `docker/egress/allowlist`.
- Volumes: `ledger` (`/work/rig`: `.beads`, `.factory`) and `repo` (`/work/rig/repo`). Nothing from the host is mounted.
- Build contexts are allowlists (`.dockerignore`, `docker/.dockerignore`): only `Cargo.*`, the toolchain/lint configs, `crates/` and the entrypoint reach the daemon — never `target/`, `.git`, ledgers or `docker/rig.env`.
- Credentials arrive as env from `docker/rig.env` (gitignored) at start; OpenCode's provider config is generated from env by the entrypoint and reads the key via `{env:…}`.
- Roles are compose services sharing the image: `steward`, `verifier`, `integrator`, `worker`, `worker-opencode`, `worker-codex` (profile), one-shots `plan`, `shell`.
- Threat model and what the rig does *not* protect: `docs/SECURITY.md`.

## Services

`ledger` (the rig's Dolt SQL server, first up, health-checked), `egress` (the only route out),
then the roles: `steward`, `verifier`, `integrator`, `worker`, `planner`; optional profiles add
a rig-local `console`, `postgres`, `redis`, an OpenCode worker, and a chat bridge. Roles wait
for `ledger` to be healthy. `factory rig stop` takes the roles and egress down and leaves
`ledger` up, so a stopped rig's history stays readable; only `rig destroy` takes it down. `RIG_WORKERS=N` (compose env) runs N worker replicas; leases keep them
from claiming the same task, worktrees are per task branch, and the shared cache volume is
safe under cargo's own locking. The throughput report says whether a second worker pays:
wall-clock minus critical path is the most it can save. See `ledger.md` for why the server exists.
