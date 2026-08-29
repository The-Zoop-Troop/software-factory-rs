- **Status:** reference · **Verified:** spec v1.0 copy, 2026-08-29

# A2A (Agent-to-Agent) Protocol — Technical Reference

> How autonomous agents built on diverse frameworks, by different organizations, on separate servers, discover each other and collaborate — **as agents, not just as tools.**
>
> Source: `A2A/` project. Canonical data model: `A2A/specification/a2a.proto` (`package lf.a2a.v1`). Normative spec: `A2A/docs/specification.md`. This is the **v1.0** specification.
>

---

## 1. Mental model & actors

The central design principle is **opacity**: an A2A agent is a black box. It collaborates by exposing *capabilities* and working on *tasks* — it never exposes its internal state, memory, proprietary logic, or tool implementations. This is what lets agents from competing vendors interoperate without sharing IP.

**Three actors** (`A2A/docs/topics/key-concepts.md`):

| Actor | Role |
|---|---|
| **User** | Human or service that defines a goal. |
| **A2A Client / Client Agent** | Acts on the user's behalf; *initiates* requests. |
| **A2A Server / Remote Agent** | Exposes an HTTP endpoint implementing A2A; treated as an opaque black box. |

An agent is frequently **both** a client and a server — orchestration produces chains of agents calling agents.

**Serialization note:** the data model is Protobuf-canonical. JSON **MUST** use **camelCase** field names and **`SCREAMING_SNAKE_CASE`** enum string values (ProtoJSON, per ADR-001). The v1.0 spec uses **PascalCase RPC method names** (`SendMessage`, `GetTask`) — the older slash-style `message/send`, `tasks/get` are pre-1.0 (v0.2.x) names.

---

## 2. Core data objects

All field references below are from `specification/a2a.proto`.

### AgentCard (`a2a.proto:361-398`, spec §8)
The self-describing manifest — the entry point for discovery.

| Field | Notes |
|---|---|
| `name`, `description` | **REQUIRED** identity |
| `supportedInterfaces` | **REQUIRED**, repeated `AgentInterface` — ordered, first = preferred |
| `provider` | `AgentProvider` (`organization`, `url`) |
| `version` | **REQUIRED** — the agent's own version, e.g. `"1.2.0"` |
| `documentationUrl`, `iconUrl` | optional |
| `capabilities` | **REQUIRED** `AgentCapabilities` |
| `securitySchemes` / `securityRequirements` | auth schemes + which are required |
| `defaultInputModes` / `defaultOutputModes` | **REQUIRED** media-type lists |
| `skills` | **REQUIRED**, repeated `AgentSkill` |
| `signatures` | optional JWS signatures over the card (RFC 7515) |

Sub-objects:
- **AgentInterface** — `url` (HTTPS), `protocolBinding` (`JSONRPC` / `GRPC` / `HTTP+JSON`), `protocolVersion`, `tenant` (opaque routing id for multi-tenant endpoints).
- **AgentCapabilities** — `streaming` (bool), `pushNotifications` (bool), `extensions` (repeated `AgentExtension`), `extendedAgentCard` (bool).
- **AgentExtension** — `uri`, `description`, `required`, `params` (Struct). *(This is the mechanism A2UI uses to ride on A2A.)*
- **AgentSkill** — `id`, `name`, `description`, `tags` (all required), plus `examples`, `inputModes`, `outputModes`, and per-skill `securityRequirements`.

### Task (`a2a.proto:167-184`, §4.1.1)
The core **stateful unit of work**.
- `id` (server-generated UUID), `contextId` (groups related tasks/messages), `status` (`TaskStatus`), `artifacts` (outputs), `history` (turn history, length via `historyLength`), `metadata`.
- **TaskStatus** — `state` (`TaskState` enum), optional `message` (e.g. the agent's question when `INPUT_REQUIRED`), `timestamp` (ISO 8601 UTC).

### Message (`a2a.proto:260-277`, §4.1.4)
One turn of communication.
- `messageId` (sender-created), `role` (`ROLE_USER` = client→server, `ROLE_AGENT` = server→client), `parts` (repeated `Part`).
- `contextId` / `taskId` (optional associations — server infers `contextId` from `taskId`), `metadata`, `extensions` (URIs contributing to this message), `referenceTaskIds` (links to prior tasks for refinements/follow-ups).

### Part (`a2a.proto:224-242`, §4.1.6)
The fundamental content container — a **`oneof`** making A2A modality-independent:
- `text` (string) · `raw` (bytes; base64 in JSON) · `url` (URI to external content) · `data` (`google.protobuf.Value` — arbitrary structured JSON).
- Plus on any part: `metadata`, `filename`, `mediaType` (MIME type).

> **A2UI hook:** an A2UI message is carried as a `Part` of kind `data`, tagged `mediaType: "application/a2ui+json"`.

### Artifact (`a2a.proto:280-293`, §4.1.7)
A tangible task output: `artifactId`, `name`, `description`, `parts` (≥1), `metadata`, `extensions`. Streamed incrementally via `TaskArtifactUpdateEvent`. Convention: a refined version keeps the same `name` but gets a **new `artifactId`**; the *client* tracks version lineage.

---

## 3. Transport, RPC methods & the three interaction modes

**One operation set, three functionally-equivalent bindings** (spec §5): **JSON-RPC 2.0** (§9), **gRPC** (§10), **HTTP+JSON/REST** (§11). Custom bindings allowed. Media type `application/a2a+json`.

### The 11 core operations

| Operation | Method (JSON-RPC/gRPC) | REST endpoint |
|---|---|---|
| Send message | `SendMessage` | `POST /message:send` |
| Send streaming message | `SendStreamingMessage` | `POST /message:stream` |
| Get task | `GetTask` | `GET /tasks/{id}` |
| List tasks | `ListTasks` | `GET /tasks` |
| Cancel task | `CancelTask` | `POST /tasks/{id}:cancel` |
| Subscribe to task | `SubscribeToTask` | `.../:subscribe` |
| Create push config | `CreateTaskPushNotificationConfig` | `POST /tasks/{id}/pushNotificationConfigs` |
| Get / List / Delete push config | `Get/List/DeleteTaskPushNotificationConfig` | `.../pushNotificationConfigs[/{id}]` |
| Get extended agent card | `GetExtendedAgentCard` | `GET /extendedAgentCard` |

- **`SendMessage`** returns `oneof { Task; Message }` — the agent chooses a stateful `Task` (long-running) or an immediate `Message`. Blocking by default; `configuration.returnImmediately: true` returns right after task creation.
- **Streaming methods** return a `stream StreamResponse` = `oneof { Task; Message; TaskStatusUpdateEvent; TaskArtifactUpdateEvent }`.
- **`ListTasks`** — cursor pagination (`pageToken`/`nextPageToken`), filters (`contextId`, `status`, `statusTimestampAfter`), `pageSize` default 50 / max 100, sorted by status timestamp desc.

### The three interaction modes

1. **Sync request/response (polling)** — `SendMessage`, then poll `GetTask` for long jobs. Blocking mode waits for a terminal or interrupted state.
2. **SSE streaming** — `SendStreamingMessage` (requires `capabilities.streaming: true`). Server replies `HTTP 200` + `Content-Type: text/event-stream`; each `data:` line is a JSON-RPC response wrapping a `StreamResponse`. A task stream begins with a `Task`, then 0+ status/artifact update events, and closes at a terminal state. Reconnect a dropped stream via `SubscribeToTask`. `TaskArtifactUpdateEvent` carries `append`/`lastChunk` for reassembling chunked artifacts.
3. **Push notifications (webhooks)** — for long-running/disconnected work (requires `capabilities.pushNotifications: true`). Client supplies a `PushNotificationConfig` (`url`, `token`, `authentication`); the server POSTs a `StreamResponse` payload to the webhook on significant state changes; the client then usually calls `GetTask` for full state.

---

## 4. Task lifecycle & state machine

**`TaskState` enum** (`a2a.proto:187-208`, §4.1.3):

| State | Kind | Meaning |
|---|---|---|
| `TASK_STATE_SUBMITTED` | active | submitted & acknowledged |
| `TASK_STATE_WORKING` | active | actively processing |
| `TASK_STATE_INPUT_REQUIRED` | **interrupted** | agent needs more user input |
| `TASK_STATE_AUTH_REQUIRED` | **interrupted** | agent needs authentication/credential |
| `TASK_STATE_COMPLETED` | **terminal** | finished successfully |
| `TASK_STATE_FAILED` | **terminal** | finished with error |
| `TASK_STATE_CANCELED` | **terminal** | canceled before completion |
| `TASK_STATE_REJECTED` | **terminal** | agent declined to perform |

Rules (prose; no formal diagram in the repo):
- Happy path `SUBMITTED → WORKING → COMPLETED`. Interrupted states occur mid-flight and resume to `WORKING` on input/credential.
- **Terminal states are immutable** — a task can't restart and accepts no further messages (`UnsupportedOperationError`). Any refinement/follow-up starts a **new task within the same `contextId`**.
- Blocking `SendMessage` returns on terminal *or* interrupted state. Streams close at terminal.
- `CancelTask` fails with `TaskNotCancelableError` if already terminal.
- **In-task auth** (§7.6): the agent sets `AUTH_REQUIRED` + an explanatory `TaskStatus.message`; the credential is delivered out-of-band; the agent may resume without a follow-up message.

**Context grouping:** the server-generated `contextId` logically groups multiple Tasks and standalone Messages into a session; `referenceTaskIds` on a Message hints which prior task a new one refines — supporting parallel dependent follow-ups.

---

## 5. Agent discovery

Servers **MUST** publish an AgentCard. Three strategies (`A2A/docs/topics/agent-discovery.md`):

1. **Well-Known URI** (recommended for public agents): `https://{domain}/.well-known/agent-card.json` (RFC 8615).
2. **Curated registries/catalogs** — an intermediary lets clients query by skills/tags/provider/capabilities. *(The registry API is not yet standardized.)*
3. **Direct configuration** — hardcoded URLs, config files, env vars.

**Extended Agent Card** (§3.1.11): if `capabilities.extendedAgentCard: true`, an **authenticated** client calls `GetExtendedAgentCard` for a fuller card (extra skills/config), possibly varying by client identity.

**Signing & caching:** cards MAY carry JWS `signatures` (RFC 7515). Use `Cache-Control`/`max-age`, `ETag` (from `version`), and conditional `If-None-Match`/`If-Modified-Since`.

### Sample AgentCard (abridged, from spec §8)

```json
{
  "name": "GeoSpatial Route Planner Agent",
  "description": "Provides advanced route planning, traffic analysis, and custom map generation services.",
  "supportedInterfaces": [
    {"url": "https://georoute-agent.example.com/a2a/v1", "protocolBinding": "JSONRPC", "protocolVersion": "1.0"},
    {"url": "https://georoute-agent.example.com/a2a/grpc", "protocolBinding": "GRPC", "protocolVersion": "1.0"}
  ],
  "provider": {"organization": "Example Geo Services Inc.", "url": "https://www.examplegeoservices.com"},
  "version": "1.2.0",
  "capabilities": {"streaming": true, "pushNotifications": true, "extendedAgentCard": true},
  "securitySchemes": {
    "google": {"openIdConnectSecurityScheme": {"openIdConnectUrl": "https://accounts.google.com/.well-known/openid-configuration"}}
  },
  "security": [{"google": ["openid", "profile", "email"]}],
  "defaultInputModes": ["application/json", "text/plain"],
  "defaultOutputModes": ["application/json", "image/png"],
  "skills": [
    {
      "id": "route-optimizer-traffic",
      "name": "Traffic-Aware Route Optimizer",
      "description": "Calculates the optimal driving route between two or more locations, considering real-time traffic.",
      "tags": ["maps", "routing", "navigation", "traffic"],
      "examples": ["Plan a route from '1600 Amphitheatre Parkway' to 'SFO' avoiding tolls."],
      "inputModes": ["application/json", "text/plain"],
      "outputModes": ["application/json", "application/vnd.geo+json", "text/html"]
    }
  ]
}
```

---

## 6. Security & authentication model

**Core principle:** A2A carries **no identity in the payload** — identity is established at the transport/HTTP layer with standard web mechanisms.

- **Transport:** HTTPS mandatory in production; TLS 1.3+ recommended; clients SHOULD validate the server cert.
- **Client auth flow** (§7.3): (1) read required schemes from the card; (2) acquire credentials **out-of-band**; (3) send them in binding-appropriate HTTP headers on **every** request (e.g. `Authorization: Bearer <TOKEN>`).
- **Server responsibilities:** authenticate every request → `401` (+`WWW-Authenticate`) for missing/invalid creds, `403` for authorized-but-forbidden. Authorization is per-skill/scope, least-privilege.

**SecurityScheme `oneof`** (`a2a.proto:503-564`, OpenAPI-3.2-aligned): `APIKey`, `HTTPAuth` (Bearer), `OAuth2` (flows: `authorizationCode` w/ `pkceRequired`, `clientCredentials`, `deviceCode`; `implicit`/`password` deprecated), `OpenIdConnect`, `MutualTls`. A `SecurityRequirement` maps scheme name → required scopes.

**Push-notification security** (§13.2): the server MUST authenticate to the client webhook and guard against **SSRF** (URL allowlisting / ownership verification / egress controls). The webhook receiver MUST verify sender authenticity (JWT via JWKS, HMAC, or API key), validate the optional `token`, and prevent replay (timestamps, `jti`/nonce).

**Data-access scoping** (§13.1): `GetTask`/`ListTasks` MUST be authorization-scoped; servers SHOULD NOT distinguish "not found" from "not authorized" (prevents info leakage).

---

## 7. A2A ↔ MCP: complementary, not competing

| | MCP | A2A |
|---|---|---|
| Connects | LLM/agent → **tools & resources** | **agent → agent** as peers |
| Nature | stateless, structured I/O, function-call-like | stateful, long-running, multi-turn, native modalities |
| Slogan | agents *using* capabilities | agents *partnering* on tasks |

**The stack:** A2A (inter-agent, cross-org) → MCP (model ↔ data/resources) → Frameworks (ADK, LangGraph, CrewAI…) → Models (any LLM).

**Canonical pattern:** an app uses **A2A between agents**, while **each agent internally uses MCP** to reach its own tools. Wrapping an agent as a mere MCP tool is "fundamentally limiting" — A2A exposes agents *as they are*, with discovery, negotiation, and shared stateful tasks.

---

## 8. Concrete request/response examples

**Basic task (REST binding):**
```http
POST /message:send HTTP/1.1
Content-Type: application/a2a+json
Authorization: Bearer token

{ "message": { "role": "ROLE_USER", "parts": [{"text": "What is the weather today?"}], "messageId": "msg-uuid" } }
```
```json
{ "task": { "id": "task-uuid", "contextId": "context-uuid",
  "status": {"state": "TASK_STATE_COMPLETED"},
  "artifacts": [{ "artifactId": "artifact-uuid", "name": "Weather Report",
    "parts": [{"text": "Today will be sunny with a high of 75°F"}] }] } }
```

**SSE stream:**
```
HTTP/1.1 200 OK
Content-Type: text/event-stream

data: {"task": {"id": "task-uuid", "status": {"state": "TASK_STATE_WORKING"}}}

data: {"artifactUpdate": {"taskId": "task-uuid", "artifact": {"parts": [{"text": "# Report\n\n"}]}}}

data: {"statusUpdate": {"taskId": "task-uuid", "status": {"state": "TASK_STATE_COMPLETED"}}}
```

**Multi-turn (agent asks a question):**
```json
{ "task": { "id": "task-uuid",
  "status": { "state": "TASK_STATE_INPUT_REQUIRED",
    "message": { "role": "ROLE_AGENT",
      "parts": [{"text": "I need more details. Where would you like to fly from and to?"}] } } } }
```

**JSON-RPC error object:**
```json
{ "jsonrpc": "2.0", "id": 1, "error": { "code": -32602, "message": "Invalid parameters",
  "data": [ { "@type": "type.googleapis.com/google.rpc.BadRequest",
    "fieldViolations": [ { "field": "message.parts", "description": "At least one part is required" } ] } ] } }
```

**Error model:** standard JSON-RPC codes (`-32700`…`-32603`) plus A2A codes `-32001`…`-32009`: `TaskNotFoundError`, `TaskNotCancelableError`, `PushNotificationNotSupportedError`, `UnsupportedOperationError`, `ContentTypeNotSupportedError`, `InvalidAgentResponseError`, `ExtendedAgentCardNotConfiguredError`, `ExtensionSupportRequiredError`, `VersionNotSupportedError`.

---

## 9. Key source files

| Concern | File |
|---|---|
| Canonical data model + `A2AService` RPCs | `A2A/specification/a2a.proto` |
| Full normative spec | `A2A/docs/specification.md` |
| Topic guides | `A2A/docs/topics/{key-concepts,what-is-a2a,a2a-and-mcp,agent-discovery,life-of-a-task,streaming-and-async,enterprise-ready}.md` |
| Serialization ADR | `A2A/adrs/adr-001-protojson-serialization.md` |

**SDKs:** Python (`pip install a2a-sdk`), Go, JS (`@a2a-js/sdk`), Java, .NET (`A2A`), Rust.

_Analysis date: 2026-07-07._
