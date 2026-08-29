# Exec plan: remote control

- **Status:** completed 2026-08-29 · **Owner:** human steers, agents execute · **Started:** 2026-08-29
- **Beads epic:** `fac-dk8`
- **Depends on:** nothing new in the rig; builds on `factory watch/inbox/doctor`, the ledger, and `.factory/events.jsonl`

## Goal

Operate one or many rigs from anywhere — a terminal, a chat client, a browser, another agent —
through one authenticated control plane, without ever exposing a rig or a provider credential.
The control plane speaks **A2A** (the interface the architecture already promises), so every
client, including other agents, is just an A2A client.

## Non-goals (this plan)

- Approving merges remotely. Verification decides what lands; humans plan, watch, answer, resolve.
- Cross-rig dependencies or a shared ledger (Beads federation is a later plan).
- A hosted/multi-tenant service. One operator, one host (or a few), their rigs.

## Architecture

```
clients:  factory CLI --rig URL   Telegram/Slack bot   web app (A2UI)   other agents (A2A)
                 │                       │                  │                 │
                 └───────────────────────┴────────┬─────────┴─────────────────┘
                                                  ▼  HTTPS + auth (per-client token, per-rig scope)
                                        console (crates/console, axum, A2A JSON-RPC + SSE)
                                          · rig registry   · audit log   · budgets   · notifications
                                                  │ same host: ledger volume + events.jsonl, `bd`
                                                  │ remote host: mTLS to a console peer
                               ┌──────────────────┼──────────────────┐
                               ▼                  ▼                  ▼
                          rig "toy"          rig "api"          rig "web"     (internal networks; unreachable from outside)
```

A2A mapping (from `ARCHITECTURE.md §5`): plan submission = `SendMessage` → returns a `Task` whose
`contextId` is the epic; progress = `GetTask` / `SubscribeToTask` (SSE) over the events log;
inbox = tasks in `INPUT_REQUIRED`, answered with `SendMessage { taskId }`; stop = `CancelTask`.
The Agent Card lists skills per rig (`plan`, `watch`, `inbox`, `resolve`, `doctor`).

## Epics (in order)

1. **console-api** — `crates/console`: axum server; rig registry (TOML: name, ledger path, repo
   path, harness, budget); A2A endpoints over the ledger (`bd … --json`) and `events.jsonl`
   tailing for SSE; Agent Card at `/.well-known/agent-card.json`; auth = bearer tokens with
   per-rig scopes (`watch`, `plan`, `resolve`, `admin`); every remote action appended to the
   rig's event log with the client identity; rate and USD budget caps per rig. Ports/fakes in
   `app`; the console never touches Dolt or provider credentials.
2. **cli-remote + telegram** — `factory --rig <url>` for `watch/inbox/plan/doctor` over the API;
   a Telegram bot (long polling, no inbound port) with `/plan`, `/watch`, `/inbox`,
   `/resolve <id> <note>`, `/stop <epic>`, and push notifications on `incident`, `question`,
   `epic_closed`, `budget` events. Slack/Discord as thin adapters of the same client.
3. **rig-registry + multi-project** — `factory rig create|list|destroy` materialising a compose
   project per rig (`COMPOSE_PROJECT_NAME`), registering it with the console, seeding `rig.env`
   from a per-rig secrets file; `factory rig doctor` across all rigs; host-level backup of
   `ledger`/`repo` volumes.
4. **ops-hardening** — reverse proxy with TLS (Caddy), systemd/compose service for the console,
   OpenTelemetry export (from the debt list) so remote operators can ask "why is rig X slow",
   alerting on incidents, restore drill documented.
5. **web-console (A2UI)** — epic board, task states and timeline, inbox with resolve, plan editor;
   rendered from A2UI messages so the same UI is agent-drivable. Last because everything before
   it is usable without a browser.

## Security posture (decided up front)

- Rigs stay on internal networks; only the console reaches them. The console is the sole exposed
  surface and terminates TLS behind a reverse proxy.
- Auth: per-client bearer tokens (or OIDC later), scoped per rig and per verb; tokens rotate;
  `admin` scope required for registry changes. No anonymous endpoints except the Agent Card.
- Audit: every remote action is an event in the rig's `events.jsonl` with `actor = client id`.
- Credentials: provider keys live only in each rig's `rig.env`; the console never reads or relays
  them; chat clients carry only their own bot token.
- Budgets: per-rig USD/tokens caps enforced by the console before a plan is accepted; hard stop
  (`CancelTask`) when exceeded.
- Human semantics: remote humans may plan, watch, answer questions, resolve incidents, stop.
  They may not force-merge; verification is not overridable from a chat window.

## Acceptance

- From a phone, via Telegram: submit a plan to rig `toy`, receive an `epic_closed` notification,
  resolve an incident — with the rig host unreachable from the internet.
- `factory --rig https://host/rigs/toy watch` and `inbox --resolve` work with a scoped token;
  an unscoped token is refused and the refusal is audited.
- Two rigs on one host run concurrently with separate ledgers, volumes, budgets, and tokens.
- The console's own crate meets the repo gates (lint-fp, coverage ≥ 85 %, docs).

## Decision log

- 2026-08-29 — Control plane speaks A2A; the web UI is A2UI. Clients are thin.
- 2026-08-29 — Order: API → CLI/Telegram → registry/multi-project → ops → web. Value per unit of
  work, and everything before the web app is usable without it.
- 2026-08-29 — Remote humans cannot approve merges. Done means verified, remotely too.

- 2026-08-29 — Plans reach the rig through a `plan_request` bead served by the rig's `planner` service, not by the console running a harness: the console never holds a provider credential and the design stays pull-based. `plan_cmd` remains as an opt-in for hosts that run the planner locally.
- 2026-08-29 — `serde_json::json!` is not used in the console (its expansion unwraps); values are built with explicit serde types and small helpers so the deny-tier lints hold.
- 2026-08-29 — `CancelTask` closes an epic's open tasks and labels the epic `fac:canceled`; a worker mid-session loses its task at the next persist rather than being killed.
- 2026-08-29 — Push notifications come from polling `ListTasks` and diffing states (`app::remote::chat::notifications`), not from SSE: one code path serves every chat transport and survives reconnects for free. SSE stays for interactive clients.
- 2026-08-29 — The bot serves only chat ids listed at start (`--chat`); everything else is logged and ignored. A bot token alone must not be enough to drive a rig.
- 2026-08-29 — A rig is a compose *project* over the one shared `compose.yaml` (env file per rig), not a copy of the file: fixes land in every rig on the next `up`. The console is a separate project that mounts the rigs' ledger volumes as `external`, so it can be restarted without touching a rig.
- 2026-08-29 — `rig restore` refuses while any service of the rig runs; restoring under a live steward would race the ledger.
- 2026-08-29 — Alerts are a console sweep over `ListTasks` (plus one fetch for each epic that vanished from the listing) posted to a generic JSON webhook; no per-integration code. Closed epics drop out of `ListTasks`, so watchers that saw them fetch them once (`list_tasks_with_vanished`).
- 2026-08-29 — OTLP over HTTP with the exporter's own client (opentelemetry-http pins reqwest 0.13 while the workspace is on 0.12); traces only — metrics stay in `events.jsonl` until a dashboard needs counters.
- 2026-08-29 — The web console is the A2UI surface plus a ~120-line renderer, not a separate front end: one read model, one action path, agent-drivable by construction. The board is re-sent whole on every refresh (idempotent `updateComponents`) instead of diffed; the payload is small and the protocol makes incrementality emergent.
- 2026-08-29 — Verified live: `docker compose up console` over the toy rig; card, 401, ListTasks, `/ui`, `/`, and `factory --rig doctor|watch` all answer. Two rig-level fixes came out of it: the generated single-rig registry is written to `/tmp` (the tokens mount is read-only for uid 10001), and the console joins `outside` because published ports need a non-internal network.

## Progress

- [x] console-api (`crates/console`: cards, SendMessage/GetTask/ListTasks/CancelTask, SSE SubscribeToTask, hashed scoped tokens, audit, budgets; compose `console` + `planner`; generated API doc) — 2026-08-29
- [x] cli-remote + telegram (`factory --rig/--token` for watch/inbox/plan/stop/doctor over `infra::A2aHttp`; `factory telegram` long-polling bot with chat allowlist and push notifications; compose `telegram` profile; shared `app::remote::chat` core) — 2026-08-29
- [x] rig-registry + multi-project (`factory rig create|list|destroy|doctor|backup|restore|console`; `app::rigs` registry + rendering + `HostDocker` port; `infra::DockerCli`; per-rig compose project and secrets; console over external ledger volumes) — 2026-08-29
- [x] ops-hardening (Caddy `tls` profile; systemd user units for rigs and the console; `infra::telemetry` OTLP/HTTP traces in every binary; console `--alert-url` webhook sweep; restore drill in RELIABILITY) — 2026-08-29
- [x] web-console (A2UI) (`app::remote::a2ui` surface + actions; console `/`, `/rigs/<rig>/ui`, `/rigs/<rig>/ui/action`; static renderer; A2UI extension on the Agent Card) — 2026-08-29
