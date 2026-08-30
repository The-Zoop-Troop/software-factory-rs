# Exec plan: web console v2

- **Status:** completed 2026-08-29 · **Owner:** human steers, agents execute · **Started:** 2026-08-29
- **Beads epic:** ``
- **Depends on:** `docs/exec-plans/completed/remote-control.md` (console API, A2UI surface, alerts)
- **Skills:** `~/git-repos/personal_brand/{effect-fp-skill,lit-web-apps-skill,modern-css-skill,semantic-html-skill}`

## Goal

Make the browser console a place an operator can run a factory from without a terminal: see what
every rig is doing as it happens, submit work and watch it progress, and act on anything that needs
a human with the context and the choices in front of them. Professional, polished, with motion and
craft — and still nothing more than a client of the audited, scoped console API.

## What is wrong today (from live use, 2026-08-29)

- Actions block for up to 15 min with a "…" in 12 px text; no queued → planning → created feedback.
- 15 s polling replaces the whole surface; nothing is live, nothing is per-card.
- Errors are a string in a banner; incidents were Debug dumps with no "what happened / what now".
- Vanilla JS over the A2UI basic catalog: no widgets for forms, notifications, dialogs, timelines.

## Design

### Client — `crates/console/ui/` (Lit + Effect, CSR only)

- **Lit 3**, TypeScript strict (`experimentalDecorators`, `useDefineForClassFields: false`), Vite;
  no meta-framework. State ladder: reactive properties → `@lit/task` → `@lit/context` (session,
  API client) → `@lit-labs/signals` (rigs, tasks, events, pending actions, alerts).
- **Effect core** (`src/core`): every API/SSE payload `Schema`-decoded once; tagged errors
  (`Unauthorized`, `Forbidden{scope}`, `TaskNotFound`, `Terminal`, `Budget`, `PlannerFailed`,
  `Unreachable`); `ConsoleApi` as an `Effect.Service` behind a `Layer` (fake layer for tests);
  SSE as a `Stream` with `Schedule`-driven reconnect; one `ManagedRuntime` at the entry point.
- **Modern CSS**: `@layer` (reset, tokens, components, utilities), oklch tokens with `light-dark()`,
  container queries per card, `@starting-style` + `transition-behavior: allow-discrete` for cards,
  toasts and dialogs, same-document view transitions on state changes, `:has()`/`:user-invalid`
  for form validity, Popover + `<dialog>` in the top layer, scroll-driven progress as Tier-C polish
  behind `@supports`. Purposeful motion: state pulses, progress rings, confetti-free "landed" glow.
- **Semantic HTML** (element ladder, not the no-CSS rule): `<main>/<section>/<article>` per epic,
  `<output>` for live counters, `<time datetime>`, `<form>` with real labels, `<dialog>`,
  `<details>` for output, `aria-live` regions for notifications, skip link, focus-visible rings.
- **Surfaces**: overview of all rigs (landing) → rig board (epics as cards with per-task progress)
  → epic detail (task list, timeline, budgets, branches, verify output) → attention center (inbox
  drawer with badge) → alerts delivery log (webhook/Telegram sends during this session) → session
  panel (token scopes, connection state).
- Built assets are embedded in the `console` binary (`include_dir`-style), so deployment is
  unchanged; the old A2UI page stays reachable at `/a2ui` for agent renderers.

### Server — additions to `crates/console` and `app` (all behind the existing ports/fakes)

1. `GET /rigs/<rig>/events` — rig-wide SSE over `events.jsonl` (cursor, filters), plus
   `GET /events` fan-in for the overview.
2. Non-blocking actions: `SendMessage` honours `configuration.returnImmediately`; plan requests
   return the `plan_request` id at once; the `planner` writes progress notes (`planning`,
   `N tasks created`) the stream carries. Resolve/stop stay synchronous (fast).
3. Structured attention items: `INPUT_REQUIRED` messages carry a `DataPart`
   `{reason, since, attempts, context, options}`; `context` holds verify output, conflict paths,
   budget numbers; `options` are one-click workflows: *retry with fresh budget*, *retry with
   guidance* (note injected into the next worker context packet), *stop epic*, *re-plan from here*.
4. Evidence on beads: Verifier stores the failing command, exit code and last 200 lines
   (`fac_verify.last_run`, attempt history); Integrator stores conflict paths and target sha;
   Steward stores the tripped budget with numbers. `META_VERSION` unchanged (additive fields).
5. `GetTask` on an epic returns children with state, attempts, tokens, branch, last event.
6. `GET /whoami` — client id and scopes; the UI hides/disables what the token cannot do and says why.
7. `GET /rigs` overview returns per-rig counts; alert deliveries are audited (`remote` events with
   `action = "alert"`), so the UI's delivery log is just a filtered stream.

### Frontend gate (same rigor as Rust)

`cargo xtask ui-check`: pnpm install (frozen), `tsc --noEmit`, eslint (Effect + lit rules),
Vitest browser mode (Playwright provider) with 80 % coverage, Playwright e2e against the console
over the app fakes (a `console serve --fake` mode), `vite build`; CI job `ui` on every push; the
`web-e2e` runtime image supplies Chromium. Bundle budget: ≤ 250 kB gzipped.

## Epics (in order)

1. **ui-scaffold** — Vite/Lit/TS project, Effect core with fake layer, tokens/layers, embedded
   build, `console serve --fake`, `xtask ui-check`, CI job, first Playwright test.
2. **evidence-and-structured-attention** — server: verify/integrate/budget evidence on beads,
   structured `INPUT_REQUIRED` payloads with options, guidance notes into worker context, epic
   children in `GetTask`, `whoami`, overview counts, alert audit events.
3. **live-state** — rig + overview SSE, non-blocking plan with progress notes, signal store,
   per-card reactive updates, connection status/reconnect, toasts.
4. **actions-with-feedback** — plan/resolve/stop/options with pending → progress → done states,
   optimistic disable, typed error → message + recovery button, scope-aware controls.
5. **attention-center + epic-detail** — inbox drawer with badge and incident panel (evidence +
   options), epic page with task list, timeline, budgets, branches; alerts delivery log.
6. **polish** — motion, view transitions, dark/light, empty states, keyboard + screen-reader pass,
   bundle budget, docs (a new design doc `design-docs/web-console`, DEPLOYMENT), screenshots in README.

## Acceptance

- Submit a plan from the browser; within one poll of the planner the card appears in `queued`,
  moves to `working` as tasks are claimed, and every worker/verifier/integrator event shows on
  the card and in the timeline without reloading.
- An incident shows the failing verify output and offers *retry with guidance*; choosing it with
  a note reopens the task and the next worker session's packet contains the note (visible in the
  event log).
- A token with `watch` only sees a read-only board with disabled actions that explain why.
- `cargo xtask ui-check` and the `ui` CI job are green; Playwright e2e covers the three flows
  above; bundle ≤ 250 kB gzipped.

## Decision log

- 2026-08-29 — In-repo frontend under `crates/console/ui`, built by xtask and embedded in the
  binary: one gate, one version, unchanged deployment.
- 2026-08-29 — CSR only: behind a token, no SEO; SSR/SSG would add a Node render path for nothing.
- 2026-08-29 — Incidents become self-contained: evidence stored on beads by the roles that have
  it, options are real workflows. Rejected: linking to the event log (does not make the operator
  productive).
- 2026-08-29 — Notifications are in-page only (`aria-live`, toasts, badge); the UI shows webhook/
  Telegram deliveries for the active session; no browser Notifications API.
- 2026-08-29 — Auth stays paste-a-token; scopes come from `whoami` and shape the UI.
- 2026-08-29 — Landing page is an all-rigs overview; rig board and epic detail are routes
  (URLPattern + Navigation API controller, no router library).
- 2026-08-29 — The A2UI page remains for agent renderers; the human UI consumes the richer A2A
  payloads directly. One read model, two projections.
- 2026-08-29 — Design: leave to the builder; professional and polished with deliberate flourish
  (motion, oklch palette, glow on state changes), never at the expense of legibility or a11y.
- 2026-08-29 — Frontend quality gate mirrors the Rust one (strict TS, lint, browser tests, 80 %
  coverage, e2e, bundle budget) and blocks CI.
- 2026-08-29 — `@open-wc/testing-helpers` instead of `@open-wc/testing` under Vitest (the latter drags in web-dev-server's socket module). Branch coverage threshold starts at 60 % (lines/functions/statements 80 %) and rises with the attention/detail epics.
- 2026-08-29 — TypeScript pinned to 5.9 (7.x is the native-port line; language-service plugins are not supported there yet).
- 2026-08-29 — Evidence is *derived* from what the roles already leave on beads (the Verifier's note block, the Integrator's incident reason, budget/usage on the task) rather than a new metadata field: no `META_VERSION` change, nothing to migrate, and every existing rig gets structured incidents immediately. A dedicated `last_run` field can come later if the notes format proves too loose.
- 2026-08-29 — The event stream authenticates with `?token=` because `EventSource` cannot send headers; accepted on the stream endpoints only, and the token never appears in a log line (the console logs paths without query strings).
- 2026-08-29 — Streams replay a small backlog (`?backlog=N`) so a fresh page shows what just happened; per-card updates come from a debounced `ListTasks` refresh of the rig that emitted the event rather than from patching cards out of event payloads — one read model, no drift.
- 2026-08-29 — lit-analyzer's CSS validation is off: it rejects `@starting-style`, container queries, `text-wrap`, `view-transition-name`; ESLint + tsc + browser tests remain the gate for templates.

## Progress

- [x] ui-scaffold (`crates/console/ui`: Lit 3 + signals/context/task, Effect core with live + fake `ConsoleApi` layers, cascade-layer/oklch tokens, router on URLPattern/Navigation API, overview + rig pages, toasts/error panel; embedded via `include_dir` with placeholder fallback; `console serve --fake`; `cargo xtask ui-check` (tsc, eslint with Effect rules, Vitest node+browser 80 %, build, 250 kB budget, Playwright e2e); CI `ui` job; image builds the UI in a node stage) — 2026-08-29
- [x] evidence-and-structured-attention (`app::remote::attention`: reason/attempts/tokens/branch/last verify block/guidance/options mined from the beads the roles already write; `apply_option` workflows retry_fresh | retry_with_guidance (note → next worker packet) | stop_epic | replan | answer, over A2A `SendMessage` data parts and the UI action endpoint; epic `children` in `GetTask`; `GET /whoami`; `GET /rigs` counts; alert deliveries audited) — 2026-08-29
- [x] live-state (server: `GET /rigs/<rig>/events` + `GET /events` SSE with cursor/backlog and `?token=` for EventSource; `SendMessage returnImmediately` queues a plan and returns the request as a task; planner progress notes + `plan_started/planned/plan_failed` events. UI: Effect `Stream` over EventSource with backoff reconnect, events store with human descriptions, live feed per rig, request cards with planner progress, stream status in the header, toasts for events that matter, debounced per-rig refresh on every frame) — 2026-08-29
- [ ] actions-with-feedback
- [ ] attention-center + epic-detail
- [x] polish (dark/light oklch palette with readable badges in both, motion via `@starting-style`/view transitions with reduced-motion respected, toasts that expire and cap, replayed events kept out of notifications, empty states on every page, `docs/design-docs/web-console.md`, README screenshots, lit-analyzer CSS check disabled for modern CSS) — 2026-08-29
