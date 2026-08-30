# The Harness port

- **Status:** accepted · **Verified:** live tests in `crates/infra` (`--ignored`) 2026-08-29

`app::ports::Harness::run(HarnessRequest) -> HarnessOutcome` is the only way the factory talks to an LLM agent. A request is: `cwd`, system prompt, prompt, optional JSON schema, `ToolPolicy` (None | ReadOnly | Full), turn cap, timeout. An outcome is: text, optional structured value, tokens, cost (micro-USD), turns, `is_error`.

| Adapter | Mechanism | Structured output | Tools | Notes |
|---|---|---|---|---|
| `ClaudeCli` | `claude -p --output-format json` | `--json-schema` | `--tools`, `--dangerously-skip-permissions` for Full | token from `CLAUDE_CODE_OAUTH_TOKEN`/`ANTHROPIC_API_KEY`; `--bare` skips keychain so not used |
| `OpencodeServer` | spawn `opencode serve` per request; `POST /session/{id}/message` | `format: json_schema` | `tools` map; permission via `OPENCODE_CONFIG_CONTENT` | any OpenAI-compatible provider; loopback client must ignore proxy env and not pool connections |
| `CodexCli` | `codex exec --json` | `--output-schema` + `-o` | `-s read-only` / `--dangerously-bypass-approvals-and-sandbox` | Responses API only; system prompt is prepended to the message; stdin must be `/dev/null` |

Choosing: `factory … --harness <kind> --model <m>`; `build_harness` in `crates/factory` is the single wiring site. Details of each CLI's contract: `docs/references/`.

## Effort

`HarnessRequest.effort: Option<Effort>` (`domain::Effort` = low | medium | high | max) travels with every session; the adapters translate it — Claude `--effort <level>`, OpenCode `variant: <level>` on the message, Codex `-c model_reasoning_effort="<level>"` with `max` spelled `xhigh`. `None` keeps the harness default. The Planner reads it from `PlanDefaults.effort`, the Worker from `WorkerConfig.effort`; the rig sets them per role from `RIG_EFFORT` and the `RIG_PLANNER_*` / `RIG_WORKER_*` overrides (2026-08-30).
