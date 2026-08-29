# The rig sandbox

- **Status:** accepted · **Verified:** in-container acceptance 2026-08-28 (uid, CapEff, proxy allow/deny, no direct route)

- One rootless Docker container per project: `factory-rig` image (Rust toolchain, git, make, `bd`, `claude`, `opencode`, `codex`; uid 10001; `cap_drop: ALL`; `no-new-privileges`; pids/mem/cpu limits).
- Network `rig` is `internal: true`; only `egress` (tinyproxy, `FilterDefaultDeny`) bridges out, with a domain allowlist in `docker/egress/allowlist`.
- Volumes: `ledger` (`/work/rig`: `.beads`, `.factory`) and `repo` (`/work/rig/repo`). Nothing from the host is mounted.
- Build contexts are allowlists (`.dockerignore`, `docker/.dockerignore`): only `Cargo.*`, the toolchain/lint configs, `crates/` and the entrypoint reach the daemon — never `target/`, `.git`, ledgers or `docker/rig.env`.
- Credentials arrive as env from `docker/rig.env` (gitignored) at start; OpenCode's provider config is generated from env by the entrypoint and reads the key via `{env:…}`.
- Roles are compose services sharing the image: `steward`, `verifier`, `integrator`, `worker`, `worker-opencode`, `worker-codex` (profile), one-shots `plan`, `shell`.
- Threat model and what the rig does *not* protect: `docs/SECURITY.md`.
