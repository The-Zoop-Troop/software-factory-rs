# Deployment

- **Status:** accepted · **Verified:** 2026-08-29 on Linux, rootless Docker 29.x, Compose v5

## Prerequisites
- Linux host with **rootless** Docker (`docker info | grep rootless`) and Compose v2+.
- One credential per harness you intend to run (see `docs/SECURITY.md` for scope):
  `CLAUDE_CODE_OAUTH_TOKEN` (from `claude setup-token`), `ANTHROPIC_API_KEY`, or `CLAUDE_AUTH_JSON=$(base64 -w0 ~/.claude/.credentials.json)` if host re-logins keep revoking setup tokens; `OPENCODE_*` for an OpenAI-compatible provider; `OPENAI_API_KEY` or `CODEX_AUTH_JSON=$(base64 -w0 ~/.codex/auth.json)` for Codex — the entrypoint seeds Codex's login from these.
- Outbound access from the host to the allowlisted domains in `docker/egress/allowlist` (edit it for your git remote and registries).

## First run
```sh
cp docker/rig.env.example docker/rig.env   # fill in; gitignored
docker compose build                        # rig image (~1.4 GB) + egress proxy
docker compose up -d                        # egress, steward, verifier, integrator, worker (claude)
docker compose run --rm shell doctor --probe   # tools, ledger, repo, credentials; --probe sends one token per harness
```
Bring your project in: set `RIG_REPO_URL` in `rig.env` (cloned on first start), or seed the `repo` volume via `docker compose run --rm shell`.

## Operate
```sh
docker compose run --rm -e RIG_HARNESS=opencode plan --text "…"      # submit a plan
docker compose up -d worker-opencode                                  # OpenCode worker
docker compose --profile codex up -d worker-codex                     # Codex worker
docker compose up -d --scale worker=3                                 # more Claude workers
docker compose exec steward factory watch                             # progress per epic
docker compose exec steward factory inbox [--resolve <id> --note …]   # incidents/questions
docker compose exec steward bd ready                                  # the ledger
docker compose exec steward tail -f .factory/events.jsonl             # the event log
```
Landing on a remote: pass `--remote origin` to `integrate` (edit the service command) and protect `main` on the remote so only the rig's deploy key can fast-forward it.

## One rig per worktree
Compose names volumes and containers after the project name, so an isolated factory per branch is one variable away:
```sh
COMPOSE_PROJECT_NAME=factory-$(git branch --show-current | tr '/' '-') docker compose up -d
```
Each project gets its own `ledger`/`repo` volumes and network; tear it down with the same variable and `down -v`.

## Logs
Every role logs via `tracing` to stderr (`RUST_LOG=info` by default). Set `FACTORY_LOG_FORMAT=json` for one JSON object per line (`docker compose logs --no-log-prefix steward | jq`). State transitions are additionally appended to `.factory/events.jsonl` in the `ledger` volume.

## Upgrade
`git pull && docker compose build && docker compose up -d --force-recreate`. Ledger and repo volumes persist; the image is stateless.

## Backup / restore
Volumes `ledger` and `repo` are the state: `docker run --rm -v <project>_ledger:/v -v $PWD:/b alpine tar czf /b/ledger.tgz -C /v .` (same for `repo`). Restore by extracting into a fresh volume before `up`.

## Troubleshooting
- A role logs `nothing ready` forever → `bd ready` in the rig; check `blocked` and incidents.
- Harness returns `401`/`revoked` → the credential in `rig.env` is stale; recreate the affected service after fixing it.
- A `plan` run hangs → `docker ps` for stale `plan-run-*` containers (a killed client does not kill the container) and remove them.
- Verify fails with `command not found` → the rig image lacks the tool; add it to `docker/Dockerfile.rig` (debt: per-project toolchain layer).
