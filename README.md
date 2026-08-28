# ai-software-factory

An autonomous software factory: give it a plan, get a verified, merged codebase.
Design: [`ARCHITECTURE.md`](ARCHITECTURE.md). Work tracking: `bd` (Beads) in `.beads/`.

## Shape

- **Ledger** — Beads (`bd`) is the only coordination surface; `bd ready` is the scheduler.
- **Roles** (all in `crates/app`, adapters in `crates/infra`):
  Planner → task/verify beads · Worker → branch + harness session · Verifier → runs the
  verify commands · Integrator → rebase/check/fast-forward/push · Steward → leases, budgets,
  epic close, event log. No orchestrator agent.
- **Harnesses** (`--harness`): `claude` (Claude Code headless), `opencode` (OpenCode server,
  any OpenAI-compatible provider), `codex` (Codex CLI). Swappable behind one `Harness` port.
- **Rig** — one rootless Docker container per project with default-deny egress; YOLO only inside.

## Run a rig

```sh
cp docker/rig.env.example docker/rig.env      # add credentials (gitignored)
docker compose build
docker compose up -d                          # egress, steward, verifier, integrator, worker (claude)
docker compose up -d worker-opencode          # or: OpenCode worker against OPENCODE_* provider
docker compose --profile codex up -d worker-codex
docker compose run --rm -e RIG_HARNESS=opencode plan --text "Add X, with tests and docs."
docker compose exec steward bd ready          # watch the ledger; events in .factory/events.jsonl
```

## Develop

```sh
cargo build && cargo test && cargo clippy --all-targets --all-features
cargo test -p infra -- --ignored   # live harness probes (need local claude/opencode/codex auth)
bd ready                           # what to work on next
```
