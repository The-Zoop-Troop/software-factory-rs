# Host setup and deployment (Docker)

Everything here is rootless Docker + Compose v2. Nothing runs on the host itself; the host
holds only the factory repository, `~/.factory`, and the Docker daemon.

## 1. Host prerequisites

```sh
docker info 2>/dev/null | grep -i rootless        # must say rootless
docker compose version                             # v2+
git --version && jq --version                      # used by the CLI and entrypoint
```

- One credential per harness you will run (`docs/SECURITY.md` for scope guidance):
  - **Claude Code**: `CLAUDE_CODE_OAUTH_TOKEN` (from `claude setup-token`), or
    `ANTHROPIC_API_KEY`, or `CLAUDE_AUTH_JSON=$(base64 -w0 ~/.claude/.credentials.json)`
    when host re-logins keep revoking setup tokens.
  - **Codex**: `OPENAI_API_KEY`, or `CODEX_AUTH_JSON=$(base64 -w0 ~/.codex/auth.json)`
    (full ChatGPT login incl. refresh token; the access token alone expires).
  - **OpenCode**: `OPENCODE_PROVIDER_ID/NAME/BASE_URL`, `OPENCODE_API_KEY`, `OPENCODE_MODEL`
    for any OpenAI-compatible provider.
- Outbound access from the host to the domains in `docker/egress/allowlist` (edit for your
  git host and registries).

## 2. Build the images

```sh
git clone --recurse-submodules https://github.com/The-Zoop-Troop/software-factory-rs
cd software-factory-rs
docker/build.sh <runtime>            # base → runtime chain → egress proxy
```

Facts that matter and are easy to get wrong:

- `build.sh` always rebuilds `factory-rig:base` first (factory binaries + the web console UI
  are **baked into base at build time**), then the runtime's parent chain (`web-e2e` sits on
  `node`, `polyglot` on `rust`), then assembles `docker/egress/allowlist` from
  base + runtime fragment + optional project fragment and builds the egress image.
- **The last `build.sh <runtime>` invocation decides the final egress allowlist file.** When
  building several runtimes in a row, end with the runtime whose rigs you are about to start
  (or rebuild per rig with `--project`).
- Every image is smoke-checked (harness CLIs + ledger tools present) — a zero exit is the gate.
- `--conformance` additionally runs the runtime's sample project exactly as the Verifier
  would. Use it when you changed a runtime Dockerfile.
- Runtime choice is about **verification**: browser tests need `web-e2e` (ships Chromium for
  one pinned Playwright — rigs cannot download browsers), monorepos may need `polyglot`.
  Full table: `docs/references/runtimes.md`. Published images exist at
  `ghcr.io/the-zoop-troop/rig-<runtime>` if you would rather pull than build.

## 3a. Single rig (the quick path)

For one project, the repo's compose file is enough — no registry:

```sh
cp docker/rig.env.example docker/rig.env    # fill in ONE harness credential + RIG_* (gitignored)
docker compose up -d                         # egress, ledger, steward, verifier, integrator, worker
docker compose run --rm shell doctor --probe
```

## 3b. Many rigs on one host (`factory rig`)

`factory rig` turns the shared `compose.yaml` into one compose project per rig
(`factory-<name>`: own `ledger`/`repo`/`cache` volumes, env, secrets) with one console over
all of them. Files live under `~/.factory` (`FACTORY_ROOT`):

```
~/.factory/
  rigs.toml                 # registry
  ledger.password           # one Dolt password per host (0600), written on first create
  secrets/<rig>.env         # your per-rig secrets files (0600; you create these)
  <rig>/compose.env         # generated: project name, image, ports, env-file path
  <rig>/rig.env             # copy of the secrets file the rig actually reads
  console/{compose.yaml,compose.env,rigs.toml,tokens.toml}
```

```sh
factory rig create <name> --repo-url https://…/<repo>.git \
  --runtime <rt> --harness <h> --main <feature-branch> \
  --secrets ~/.factory/secrets/<name>.env --no-start
factory rig list                        # name, repo, runtime, harness, console port
docker compose -p factory-<name> --env-file ~/.factory/<name>/compose.env -f compose.yaml up -d
factory rig doctor                      # ledger volume + running services per rig
```

`--no-start` registers without starting, so create everything first and bring rigs up one at
a time. Run these commands from the factory repository (or set `FACTORY_COMPOSE`).

## 4. Console

```sh
openssl rand -hex 32 > person.token
docker compose run --rm console console hash-token < person.token   # → sha256
# one [[token]] entry per person in ~/.factory/console/tokens.toml, scopes per rig:
#   watch | plan | resolve | admin
factory rig console                     # http://127.0.0.1:7700 over every registered rig
```

Gates:

```sh
curl -s localhost:7700/rigs/<rig>/.well-known/agent-card.json | jq .skills[].id
curl -s -H "Authorization: Bearer $(cat person.token)" \
  -d '{"jsonrpc":"2.0","id":1,"method":"ListTasks"}' localhost:7700/rigs/<rig>/a2a
```

Browser UI: `http://127.0.0.1:7700/`, paste the token. A stopped rig shows "unavailable";
running rigs show live counts. The console holds **no provider credential** — plans queue as
beads for the rig's own planner.

## 5. Production posture

- **TLS**: `CONSOLE_DOMAIN=console.example.com docker compose --profile tls up -d caddy`,
  then `CONSOLE_URL=https://console.example.com` so the Agent Card advertises it. Keep
  `CONSOLE_PORT` published on loopback only.
- **Systemd user units**: `cp docker/systemd/*.service ~/.config/systemd/user/ &&
  systemctl --user daemon-reload && systemctl --user enable --now factory-rig@<name>
  factory-console`; `loginctl enable-linger $USER` so they survive logout. Edit
  `FACTORY_COMPOSE` in the rig unit to this repo's `compose.yaml`. **Until the units are
  installed, nothing restarts on reboot** — containers only carry `restart: unless-stopped`.
- **Telemetry**: `FACTORY_LOG_FORMAT=json` for parseable logs;
  `OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318` exports spans (add the collector host
  to the egress allowlist).
- **Alerts**: `CONSOLE_ALERT_URL=https://hooks…` posts `{"rig","text"}` on every
  needs-a-human and epic-finished event; the Telegram bot (`--profile telegram`) pushes the
  same to allowlisted chats.

## 6. Verifier sidecars

A repo whose tests need a datastore: start the rig with `--profile postgres` (or `redis`) and
put `DATABASE_URL=postgres://factory:factory@postgres:5432/factory` in its rig env. Both are
throwaway (tmpfs), live only on the internal rig network, and are migrated by the tests.
