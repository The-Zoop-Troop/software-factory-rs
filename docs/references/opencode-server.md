# OpenCode 1.18.18 — `opencode serve` HTTP API
- **Status:** reference · **Verified:** 2026-08-28 live probes (`/doc` OpenAPI)

Why the server, not `opencode run`: with stdout redirected to a file the CLI buffers everything and never exits headlessly.

- Start: `opencode serve --pure --hostname 127.0.0.1 --port <p>` in the project dir (cwd = project). Health: `GET /global/health` → `{"healthy":true,"version"}`. Logs `listening on http://…` to stderr.
- Config: `OPENCODE_CONFIG_CONTENT='{"permission":"allow", …}'` merges runtime config; user config `~/.config/opencode/opencode.json`; custom provider block: `provider.<id> = { npm: "@ai-sdk/openai-compatible", name, options: { baseURL, apiKey: "{env:VAR}" }, models: { "<model>": { name } } }`. The npm package is fetched at first use from registry.npmjs.org.
- `POST /session {title}` → `{id}`. `POST /session/{id}/message` body: `model {providerID, modelID}`, `system`, `tools {name: bool}`, `parts [{type:"text", text}]`, optional `format {type:"json_schema", schema, retryCount}`. Blocking; returns `{info:{tokens:{total,input,output,reasoning,cache}, cost, finish, error?, structured?}, parts:[{type:"text",text}…]}`.
- Tool ids: bash, read, glob, grep, edit, write, task, webfetch, todowrite, websearch, skill, apply_patch, question.
- `POST /session/{id}/abort` cancels. `GET /doc` is the OpenAPI spec.
- Pitfalls: never run two servers on one project dir (per-project DB lock → hang); a connection accepted while the server boots is never serviced, so probe with a per-request timeout and no connection pooling; clients inside the rig must ignore `HTTP(S)_PROXY` for loopback.
