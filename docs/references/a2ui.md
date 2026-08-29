- **Status:** reference · **Verified:** against the A2UI v0.9.1 specification, 2026-08-29

# A2UI — what the console emits

A2UI is a declarative UI format: an agent sends *intent* (a flat list of components from a
trusted catalog plus a data model) and the client renders it with its own widgets. No
executable code crosses the wire, which is why it fits a control plane whose clients may be
other agents.

## Messages

A stream of envelopes `{ "version": "v0.9.1", <one key> }`, delivered in order:

| Key | Payload | Console use |
|---|---|---|
| `createSurface` | `{surfaceId, catalogId, sendDataModel?}` | one surface `console` on the basic catalog |
| `updateComponents` | `{surfaceId, components: [{id, component, …props}]}` | the whole board every time (idempotent) |
| `updateDataModel` | `{surfaceId, path, value}` | `/plan/text`, `/notes/<inbox id>` |
| `deleteSurface` | `{surfaceId}` | unused |

Client → server: `action {name, surfaceId, sourceComponentId, timestamp, context}`; the client
resolves `{path}` bindings in the button's context against its data model before sending.

## Model

- Components form an **adjacency list**: containers (`Column`, `Row`, `Card`, `List`) hold child
  ids; exactly one component is `root`; re-sending an id replaces it in place.
- **Bindings** are JSON Pointers: `TextField.value = {path: "/plan/text"}` writes to the local
  model on input; nothing reaches the server until a `Button` action fires.
- **Catalogs** are the security seam: the client only instantiates components it knows. The
  console uses the basic catalog subset `Column, Row, Card, Text, TextField, Button, Divider`.
- Over A2A, A2UI is an extension (`https://a2ui.org/a2a-extension/a2ui/v0.9.1`) on the Agent
  Card; messages travel as `DataPart`s of media type `application/a2ui+json`.

## In this repository

- `app::remote::a2ui::console_surface(rig, tasks)` — pure: tasks → envelopes.
- `app::remote::a2ui::parse_action` — `plan`, `resolve`, `stop`, `refresh` with their context.
- `crates/console/src/ui.rs` — `GET /rigs/<rig>/ui`, `POST /rigs/<rig>/ui/action`, `GET /`.
- `crates/console/static/console.html` — a ~120-line renderer for the subset above; any other
  A2UI renderer (Lit, React, Flutter) can consume the same envelopes.
