# OpenCode 1.18.34 — `opencode serve` HTTP API
- **Status:** reference · **Verified:** 2026-11-02 `/doc` OpenAPI on 1.18.34: the adapter's paths and message fields are present. Model calls were last probed on 1.18.18, 2026-08-28.

Why the server, not `opencode run`: with stdout redirected to a file the CLI buffers everything and never exits headlessly.

- Start: `opencode serve --pure --hostname 127.0.0.1 --port <p>` in the project dir (cwd = project). Health: `GET /global/health` → `{"healthy":true,"version"}`. Logs `listening on http://…` to stderr.
- Config: `OPENCODE_CONFIG_CONTENT='{"permission":"allow", …}'` merges runtime config; user config `~/.config/opencode/opencode.json`; custom provider block: `provider.<id> = { npm: "@ai-sdk/openai-compatible", name, options: { baseURL, apiKey: "{env:VAR}" }, models: { "<model>": { name } } }`. The npm package is fetched at first use from registry.npmjs.org.
- `POST /session {title}` → `{id}`. `POST /session/{id}/message` body: `model {providerID, modelID}`, `system`, `tools {name: bool}`, `parts [{type:"text", text}]`, optional `format {type:"json_schema", schema, retryCount}`. Blocking; returns `{info:{tokens:{total,input,output,reasoning,cache}, cost, finish, error?, structured?}, parts:[{type:"text",text}…]}`.
- Tool ids: bash, read, glob, grep, edit, write, task, webfetch, todowrite, websearch, skill, apply_patch, question.
- `POST /session/{id}/abort` cancels. `GET /doc` is the OpenAPI spec.
- Pitfalls: never run two servers on one project dir (per-project DB lock → hang); a connection accepted while the server boots is never serviced, so probe with a per-request timeout and no connection pooling; clients inside the rig must ignore `HTTP(S)_PROXY` for loopback.

## At a limit

Captured 2026-11-02 on 1.18.34 against a local stub OpenAI-compatible provider with a dummy key. No real limit has been hit.

`POST /session/{id}/message` answers HTTP 200 either way, with the failure in `info.error`:

```json
{"name":"APIError","data":{"message":"…","statusCode":429,"isRetryable":true,"responseHeaders":{},"responseBody":"…"}}
```

- `statusCode` 402, 400 and 401 returned within about a second, with `isRetryable: false`.
- 429 and 500 retried, about 9 requests in all, before answering. That took 12 seconds with `retry-after: 2` and 70 to 75 seconds without it, so a limit does not hang the session.
- The adapter maps 402 to `SpendLimit`, 429 to `RateLimit` and 5xx to `Outage`. A provider's `insufficient_quota` also arrives as 429, so it is reported as `RateLimit`; the handling is the same.

