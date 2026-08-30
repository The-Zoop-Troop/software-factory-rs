# Web console

- **Status:** accepted · **Verified:** 2026-08-29 (`cargo xtask ui-check --e2e`; screenshots in `docs/images/`)

The operator UI is a Lit application embedded in the `console` binary (`crates/console/ui`, served at `/`). It is a client of the audited, scoped console API and nothing more: everything it shows comes from `ListTasks`/`GetTask`/`whoami`/the event stream, and everything it does goes through `SendMessage`/`CancelTask`. Exec plan: `docs/exec-plans/completed/web-console-v2.md`.

![rig page, dark](../images/console-rig-dark.png)

## Shape

- **Core (`src/core`)**: Effect. Every reply is `Schema`-decoded once (`schema.ts`); failures are tagged errors with an explanation and a recovery (`errors.ts`); `ConsoleApi` is a service with a live HTTP layer and an in-memory fake layer (`api.ts`); the only `fetch` lives in `interop.ts`; one `ManagedRuntime` per session (`runtime.ts`); the SSE connection is a `Stream` with backoff reconnect (`events.ts`).
- **State (`src/state`)**: `@lit-labs/signals` modules — session (token, connection, identity/scopes), rigs (tasks per rig, summaries, attention items), events (ring buffer, human descriptions, alerts), notices (toasts with TTL and cap). Plain modules, tested in Node.
- **Actions (`src/actions.ts`)**: each operator action is an Effect that marks its target as pending, calls the API, updates the signals, and reports the outcome as a toast or an explained error. Components get a `Promise<boolean>`.
- **Live (`src/live.ts`)**: one `/events` stream per session; every frame lands in the feed, refreshes the rig it belongs to (debounced), and — unless it is a replayed backlog frame — becomes a toast when a human cares (`describe`).
- **Router (`src/router.ts`)**: URLPattern + Navigation API with a click/popstate fallback; routes `/`, `/rigs/:rig`, `/rigs/:rig/epics/:id`, lazily loaded pages, view transitions.
- **Components**: `epic-card`, `request-card`, `plan-form`, `inbox-item` + `attention-panel`, `attention-drawer`, `live-feed`, `alerts-log`, `toast-stack`, `error-panel`, `state-badge`. Styles are constructable stylesheets over cascade-layer tokens (oklch, `light-dark()`, container queries, `@starting-style`, top-layer `<dialog>`).

## Behaviour that matters

- **Scopes shape the UI.** `whoami` says what the token may do; Plan, Stop and Resolve are disabled with the missing scope named. The console enforces regardless.
- **Nothing blocks.** A plan is queued (`returnImmediately`) and shows as a request card with the planner's progress until the epic exists; Stop and options mark their own control pending.
- **Incidents are actionable.** The `Attention` data part gives the reason, attempts, tokens, branch, last verify output and prior guidance, plus options; *Retry with guidance* puts the operator's note into the next worker session.
- **Errors explain themselves** and offer a recovery (try again, enter a token).
- **Gate**: `cargo xtask ui-check [--e2e]` — strict TS, ESLint with the Effect rules, Vitest (Node + Chromium) with coverage thresholds, Vite build, 250 kB gzipped budget, Playwright e2e over `console serve --fake`. CI job `ui`.

## Decisions

- Embedded assets: the binary is self-contained; a missing build falls back to a placeholder page that says how to build it.
- `?token=` on the stream endpoints only, because `EventSource` cannot set headers.
- Per-card updates come from a debounced re-read of the rig's tasks, not from patching cards out of event payloads: one read model.
- Toasts expire (6 s) and cap at four; replayed backlog frames never toast.
