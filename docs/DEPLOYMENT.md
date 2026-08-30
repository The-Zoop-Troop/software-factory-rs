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
docker/build.sh rust                        # base + runtime image + egress proxy (see docs/references/runtimes.md)
docker compose up -d                        # egress, steward, verifier, integrator, worker (claude)
docker compose run --rm shell doctor --probe   # tools, ledger, repo, credentials; --probe sends one token per harness
```
Bring your project in: set `RIG_REPO_URL` in `rig.env` (cloned on first start), or seed the `repo` volume via `docker compose run --rm shell`.

## Operate
```sh
docker compose run --rm -e RIG_HARNESS=opencode plan --text "…"      # submit a plan (or via the console, below)
docker compose up -d worker-opencode                                  # OpenCode worker
docker compose --profile codex up -d worker-codex                     # Codex worker
docker compose up -d --scale worker=3                                 # more Claude workers
docker compose exec steward factory watch                             # progress per epic
docker compose exec steward factory inbox [--resolve <id> --note …]   # incidents/questions
docker compose exec steward bd ready                                  # the ledger
docker compose exec steward tail -f .factory/events.jsonl             # the event log
```
Landing on a remote: set `RIG_MAIN` to the branch the factory may land on (`main`/`master` are refused by default — `RIG_PROTECTED_BRANCHES`) and pass `--remote origin` to `integrate` (edit the service command) and protect `main` on the remote so only the rig's deploy key can fast-forward it.

## Models and effort
`RIG_HARNESS` picks the harness (`claude` default, `opencode`, `codex`); `CLAUDE_MODEL` / `OPENCODE_MODEL` / `CODEX_MODEL` pick the model for it; `RIG_EFFORT=low|medium|high|max` sets thinking effort (`claude --effort`, OpenCode message `variant`, Codex `model_reasoning_effort`, `max` → `xhigh`). Per role: `RIG_PLANNER_MODEL`/`RIG_PLANNER_EFFORT` and `RIG_WORKER_MODEL`/`RIG_WORKER_EFFORT` override — plan with a strong model at high effort, work with a cheaper one. On the CLI the same are `--harness`, `--model`, `--effort` on `plan` and `work`.

## Remote control (console)
The `console` service exposes the rig over A2A on `127.0.0.1:7700` (`CONSOLE_PORT`); put TLS and a real hostname in front of it (`CONSOLE_URL` goes into the Agent Card). It shares only the `ledger` volume with the rig, mounts `docker/console` read-only for tokens, gets no `rig.env`, and sits on the `outside` network as well as `rig` only because Docker cannot publish a port on an internal-only network.
```sh
openssl rand -hex 32 > phone.token
docker compose run --rm console console hash-token < phone.token    # → sha256 for tokens.toml
cp docker/console/tokens.toml.example docker/console/tokens.toml   # paste the hash, set grants
PLANNER_HARNESS=opencode docker compose up -d planner console      # planner serves the plan queue (harness of your choice)
curl -s localhost:7700/rigs/toy/.well-known/agent-card.json | jq .skills[].id
curl -s -H "Authorization: Bearer $(cat phone.token)" -d '{"jsonrpc":"2.0","id":1,"method":"ListTasks"}' localhost:7700/rigs/toy/a2a
```
From anywhere with the token:
```sh
export FACTORY_RIG=https://host/rigs/toy FACTORY_TOKEN=$(cat phone.token)
factory doctor                      # console reachable, token accepted
factory watch [--interval 30]       # epics + inbox
factory inbox --resolve <id> --note "…"
factory plan --text "…"             # queued to the rig's planner
factory stop <epic>
```
Telegram: create a bot with `@BotFather`, find your chat id (`/start` the bot, then `curl https://api.telegram.org/bot<token>/getUpdates`), then
`TELEGRAM_BOT_TOKEN=… TELEGRAM_CHATS=<id>[,<id>] FACTORY_TOKEN=$(cat phone.token) docker compose --profile telegram up -d telegram`.
The bot answers `/plan /watch /inbox /resolve /stop /help` from listed chats only and pushes a message when a task needs you or finishes. No inbound port anywhere: it long-polls Telegram through the egress proxy.

Browser: open `http://127.0.0.1:7700/` (or the TLS hostname), paste a token — the operator console (Lit + Effect, embedded in the binary; source in `crates/console/ui`, built by `cargo xtask ui-build` or the image build) shows every rig, its epics and inbox. The A2UI projection for agent renderers stays at `/a2ui`. For UI work without a rig: `cargo run -p console --features fake -- serve --fake` (token `fake`, rig `toy`). Operations, scopes, and error codes: `docs/generated/console-api.md`. Multiple rigs on one host: mount your own `docker/console/rigs.toml` (one `[[rig]]` per ledger, optional `max_tokens`/`max_usd_micros`, optional `plan_cmd`).

## Many rigs on one host
`factory rig` turns the shared `compose.yaml` into one compose project per rig (`factory-<name>`: its own `ledger`/`repo`/`cache` volumes, env, secrets) and one console over all of them. Files live under `~/.factory` (`FACTORY_ROOT`).
```sh
factory rig create toy --repo-url git@github.com:me/toy.git --runtime rust --harness claude --secrets docker/rig.env
factory rig create api --repo-url … --runtime node --harness codex          # second rig, next console port
factory rig list                                                            # name, repo, runtime, harness, port
factory rig doctor                                                          # ledger volume + running services per rig
factory rig console                                                         # one console at 127.0.0.1:7700 over every rig
factory rig backup toy --to backups/                                        # toy-ledger-<ts>.tgz, toy-repo-<ts>.tgz
docker compose -p factory-toy down && factory rig restore toy --ledger backups/toy-ledger-<ts>.tgz
factory rig destroy toy [--volumes]
```
`~/.factory/console/tokens.toml` holds the console tokens; `~/.factory/console/rigs.toml` and `compose.yaml` are regenerated on every change (the console mounts each rig's ledger volume read-write, nothing else). Run the commands from this repository (or set `FACTORY_COMPOSE` to its `compose.yaml`).

## Production posture
- **TLS.** `CONSOLE_DOMAIN=console.example.com docker compose --profile tls up -d caddy` puts Caddy (automatic Let's Encrypt, or its internal CA for `localhost`) in front of the console; then set `CONSOLE_URL=https://console.example.com` so the Agent Card advertises the right address, and stop publishing `CONSOLE_PORT` on anything but loopback. The multi-rig console (`~/.factory/console/compose.yaml`) takes the same `caddy` service; copy `docker/caddy/Caddyfile` next to it.
- **Service units.** `docker/systemd/factory-rig@.service` and `factory-console.service` are user units: `mkdir -p ~/.config/systemd/user && cp docker/systemd/*.service ~/.config/systemd/user/ && systemctl --user daemon-reload && systemctl --user enable --now factory-rig@toy factory-console` (`loginctl enable-linger $USER` so they survive logout). Edit `FACTORY_COMPOSE` in the rig unit to this repository's `compose.yaml`.
- **Telemetry.** Every binary logs via `tracing` (`FACTORY_LOG_FORMAT=json` for one object per line). Set `OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318` on any service and its spans (worker sessions, verify runs, integrations, console requests) are exported over OTLP/HTTP with `service.name` = `factory` / `stewardd` / `console`; add the collector's host to the egress allowlist.
- **Alerts.** `console serve --alert-url https://hooks.example/… --alert-interval 30` (or `CONSOLE_ALERT_URL`) posts `{"rig","text"}` whenever a task needs a human or finishes — Slack incoming webhooks, ntfy, PagerDuty Events via a relay. The Telegram bot pushes the same events to its chats.

## Logs
Every role logs via `tracing` to stderr (`RUST_LOG=info` by default). Set `FACTORY_LOG_FORMAT=json` for one JSON object per line (`docker compose logs --no-log-prefix steward | jq`). State transitions are additionally appended to `.factory/events.jsonl` in the `ledger` volume.

## Runtimes
One image per language toolchain layered on `factory-rig:base`: `docker/build.sh python|node|go|rust`, then `RIG_IMAGE=factory-rig:<runtime> docker compose up -d`. A project can ship `.factory/Dockerfile` (`FROM` the runtime) and `.factory/allowlist`; `docker/build.sh <runtime> --project <dir>` builds it. `factory doctor` reads `.factory/runtime.toml` and says which runtime the project needs.

## Upgrade
`git pull && docker/build.sh <runtime> && docker compose up -d --force-recreate`. Ledger and repo volumes persist; the image is stateless.

## Backup / restore
`factory rig backup <name>` / `factory rig restore <name> --ledger <tgz> [--repo <tgz>]` (the rig must be stopped to restore). Without the rig registry: Volumes `ledger` and `repo` are the state: `docker run --rm -v <project>_ledger:/v -v $PWD:/b alpine tar czf /b/ledger.tgz -C /v .` (same for `repo`). Restore by extracting into a fresh volume before `up`.

## Troubleshooting
- A role logs `nothing ready` forever → `bd ready` in the rig; check `blocked` and incidents.
- Harness returns `401`/`revoked` → the credential in `rig.env` is stale; recreate the affected service after fixing it.
- A `plan` run hangs → `docker ps` for stale `plan-run-*` containers (a killed client does not kill the container) and remove them.
- Verify fails with `command not found` → the rig image lacks the tool; add it to `docker/Dockerfile.rig` (debt: per-project toolchain layer).
