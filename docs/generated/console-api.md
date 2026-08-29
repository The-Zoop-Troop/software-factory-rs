# Console A2A API (generated)

- **Status:** generated · **Verified:** by `cargo xtask gen-docs --check` in CI. Do not edit by hand.

Source: `crates/console` via `console card` and `console --help`. Protocol: A2A v1 JSON-RPC binding (`docs/references/a2a.md`); every request carries `Authorization: Bearer <token>`.

## Agent Card (per rig, `GET /rigs/<rig>/.well-known/agent-card.json`)

```json
{
  "name": "factory rig toy",
  "description": "Autonomous software factory rig: plan in, verified code out. Humans plan, watch, answer, resolve, stop.",
  "version": "0.1.0",
  "supportedInterfaces": [
    {
      "url": "https://console.example/rigs/toy/a2a",
      "protocolBinding": "JSONRPC",
      "protocolVersion": "1.0"
    }
  ],
  "provider": {
    "organization": "The Zoop Troop",
    "url": "https://github.com/The-Zoop-Troop/software-factory-rs"
  },
  "capabilities": {
    "streaming": true,
    "pushNotifications": false,
    "extendedAgentCard": false
  },
  "securitySchemes": {
    "bearer": {
      "httpAuthSecurityScheme": {
        "scheme": "bearer"
      }
    }
  },
  "securityRequirements": [
    {
      "bearer": []
    }
  ],
  "defaultInputModes": [
    "text/plain"
  ],
  "defaultOutputModes": [
    "application/json"
  ],
  "skills": [
    {
      "id": "plan",
      "name": "Plan",
      "description": "SendMessage with plan text starts an epic; returns its Task.",
      "tags": [
        "factory",
        "scope:plan"
      ]
    },
    {
      "id": "watch",
      "name": "Watch",
      "description": "ListTasks / GetTask / SubscribeToTask over the ledger and event log.",
      "tags": [
        "factory",
        "scope:watch"
      ]
    },
    {
      "id": "inbox",
      "name": "Inbox",
      "description": "ListTasks filtered to INPUT_REQUIRED: incidents and questions.",
      "tags": [
        "factory",
        "scope:watch"
      ]
    },
    {
      "id": "resolve",
      "name": "Resolve",
      "description": "SendMessage with taskId of an inbox item closes it with the note.",
      "tags": [
        "factory",
        "scope:resolve"
      ]
    },
    {
      "id": "stop",
      "name": "Stop",
      "description": "CancelTask on an epic closes its open tasks.",
      "tags": [
        "factory",
        "scope:plan"
      ]
    }
  ]
}
```

## Operations (`POST /rigs/<rig>/a2a`)

| Method | Scope | Params | Result |
|---|---|---|---|
| `SendMessage` | `plan` | `{message: {parts: [{text}]}}` | `{task}` — the new epic |
| `SendMessage` | `resolve` | `{message: {taskId, parts: [{text}]}}` | `{task}` — the resolved inbox item |
| `GetTask` | `watch` | `{id}` | `Task` (epic or inbox item) |
| `ListTasks` | `watch` | `{status?: "TASK_STATE_INPUT_REQUIRED"}` | `{tasks: [Task]}` |
| `CancelTask` | `plan` | `{id}` | `Task` in `TASK_STATE_CANCELED` |
| `SubscribeToTask` | `watch` | `{id}` | SSE: `{task}` then `{statusUpdate}` per event, `final: true` at a terminal state |

Errors: HTTP 401 (no/unknown token), 403 + code -32040 (missing scope), 404 + -32001 (task/rig), 400 (-32601/-32602/-32004), -32041 (rig budget cap), -32002 (terminal task).

Task states: an epic is `SUBMITTED` until a task is claimed, `WORKING` while tasks move, `INPUT_REQUIRED` while any task is in incident, `COMPLETED` when closed, `CANCELED` after `CancelTask`. Incidents and questions are their own `INPUT_REQUIRED` tasks whose `contextId` is the epic.

## CLI

```text
A2A control plane over factory rigs

Usage: console <COMMAND>

Commands:
  serve       Serve the A2A API
  hash-token  Print the sha256 of a token read from stdin, for the token file
  card        Print the Agent Card a rig would publish (used to generate the API reference)
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

```text
Serve the A2A API

Usage: console serve [OPTIONS]

Options:
      --registry <REGISTRY>      Rig registry (TOML, see docs/DEPLOYMENT.md) [default: console/rigs.toml]
      --tokens <TOKENS>          Token file (TOML; sha256 of each bearer token and its grants) [default: console/tokens.toml]
      --listen <LISTEN>          Address to bind [default: 127.0.0.1:7700]
      --public-url <PUBLIC_URL>  URL clients reach this console at (goes into the Agent Card) [default: http://127.0.0.1:7700]
  -h, --help                     Print help
```
