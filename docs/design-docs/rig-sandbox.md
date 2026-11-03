# The rig sandbox

- **Status:** accepted · **Verified:** 2026-11-03 against `compose.yaml`, `docker/entrypoint.sh` and the code; in-container acceptance last run 2026-08-28 (uid, CapEff, proxy allow/deny, no direct route); untrusted children checked in a running rig by `docker/holes-check/run.sh` on 2026-11-03

- One rootless Docker container per project: `factory-rig:base` (git, make, `bd`, `claude`, `opencode`, `codex`, agent-ergonomics tools; uid 10001) plus a runtime layer per language (`docs/references/runtimes.md`); `cap_drop: ALL`; `no-new-privileges`; pids/mem/cpu limits. Runtimes never relax these; conformance asserts `CapEff` is zero in every image.
- Network `rig` is `internal: true`; only `egress` (tinyproxy, `FilterDefaultDeny`) bridges out, with a domain allowlist in `docker/egress/allowlist`.
- Volumes: `ledger` (`/work/rig`: `.beads`, `.factory`), `repo` (`/work/rig/repo`) and `cache` (`/work/cache`). Nothing from the host is mounted into the roles; the rig-local console bind-mounts `./docker/console`. Every role mounts all three read-write; the children that run untrusted work do not get that reach (below).
- Build contexts are allowlists (`.dockerignore`, `docker/.dockerignore`): only `Cargo.*`, the toolchain/lint configs, `crates/` and the entrypoint reach the daemon — never `target/`, `.git`, ledgers or `docker/rig.env`.
- Credentials arrive as env from `docker/rig.env` (gitignored) at start; OpenCode's provider config is generated from env by the entrypoint and reads the key via `{env:…}`. Every role but the Integrator drops `RIG_GIT_TOKEN` after the clone (`docker/git-auth.sh`).
- Roles are compose services sharing the image: `steward`, `verifier`, `integrator`, `worker`, `worker-opencode`, `worker-codex` (profile), one-shots `plan`, `shell`.
- Threat model and what the rig does *not* protect: `docs/SECURITY.md`.

## Verification prepare step

The Verifier checks a task's branch out fresh, so a project's dependencies are not there. Before
the verify commands it runs `git submodule update --init --recursive` when `.gitmodules` exists
(the platform sweep's pointer checks ran in empty submodule dirs until this), then
`[verify] prepare = [...]` from `.factory/runtime.toml` on the integration branch, never the
candidate's, or — when that key is absent — one default chosen by the lockfile in the checkout: `npm ci`,
`pnpm install --frozen-lockfile`, `yarn install --frozen-lockfile`, `go mod download` (nothing for
Rust). `prepare = []` disables the declared and default commands, not the submodule update. Its
output lands in the task's note like any verify command.

## Untrusted children

A harness session is an agent with a shell, and a verify command or an Integrator check is the
candidate's own code. Each starts as a child of its role, and none gets the role's reach:

- **Environment.** `crates/infra/src/child_env.rs` removes the factory's own secrets by name (the
  ledger password, the git token, the auth files, the console and bot tokens), every other
  harness's credential, and any name shaped like a secret (`…TOKEN`, `…_KEY`, `…PASSWORD`). A
  harness keeps its own credential. `RIG_SESSION_ENV` names variables a project needs anyway; it
  never passes the factory's own. git children get the same scrubbed environment.
- **`/proc`.** Every role process is non-dumpable, so a child cannot read its environment or
  memory, and `bd` is installed execute-only, which makes each `bd` process non-dumpable too.
- **Writes.** The role starts the child as `factory confine --write <dir>… -- <program>`, which
  restricts itself with Landlock and execs the program; descendants inherit the restriction. The
  child may write its worktree, its git metadata and object store, the ref directory of its own
  branch, `$HOME`, `$TMPDIR` or `/tmp`, `/dev` and `/work/cache`, plus `RIG_CONFINE_WRITE`. The
  ledger, the event log, other worktrees, the main checkout and the integration branch's ref stay
  read-only. Landlock needs Linux 6.2 or later; a role that cannot confine refuses to start, and
  `factory doctor` says so. `RIG_CONFINE=off` runs children unconfined.
- **`PATH`.** Verify commands run with the worktree last on `PATH`, so a committed `cargo` or `sh`
  cannot replace the real one; `. lib.sh` still resolves from the worktree.

What a child can still do: read the ledger's files, the event log and other worktrees; move other
task branches, which is harmless because the Verifier and Integrator check out the recorded commit;
and reach the ledger server's port, without a password to use it.

## Services

`ledger` (the rig's Dolt SQL server, first up, health-checked), `egress` (the only route out),
then the roles: `steward`, `verifier`, `integrator`, `worker`, `planner`; optional profiles add
a rig-local `console`, `postgres`, `redis`, an OpenCode worker, and a chat bridge. Roles wait
for `ledger` to be healthy. `factory rig stop` takes `steward`, `verifier`, `integrator`, `worker`, `planner` and egress down
and leaves `ledger` up, so a stopped rig's history stays readable; only `rig destroy` takes it down.
It stops `worker-opencode` and `worker-codex` too. `RIG_WORKERS=N` (compose env) runs N worker replicas; the ledger's atomic claim keeps them
from taking the same task (the lease write alone is last-writer-wins and once raced two
workers onto one task), worktrees are per task branch, and the shared cache volume is
safe under cargo's own locking. The throughput report says whether a second worker pays:
wall-clock minus critical path is the most it can save. See `ledger.md` for why the server exists.
