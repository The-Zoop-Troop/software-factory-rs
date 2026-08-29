# Codex CLI 0.149.1 — `codex exec`
- **Status:** reference · **Verified:** 2026-08-28 live probes

- Invocation used: `codex exec --json --ephemeral --skip-git-repo-check --color never -C <dir> -o <last-message-file> [-s read-only | --dangerously-bypass-approvals-and-sandbox] [--output-schema <file>] [-m model] "<message>" < /dev/null`. **Stdin must be closed** or it waits for "additional input".
- No system-prompt flag; prepend instructions to the message.
- JSON lines: `thread.started`, `turn.started`, `item.completed {item:{type:"agent_message", text}}`, `turn.completed {usage:{input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens, reasoning_output_tokens}}`, `turn.failed {error}`, `error {message}`.
- With `--output-schema`, the schema must be OpenAI-strict (`additionalProperties: false` on every object, all properties required); the validated answer is written to the `-o` file.
- Providers: only the Responses API (`wire_api = "responses"`); chat-completions-only gateways are not usable. Custom provider via `-c 'model_providers.<id>.base_url=…' -c 'model_providers.<id>.env_key=VAR' -c 'model_provider="<id>"'`.
- Static musl binary: `codex-x86_64-unknown-linux-musl.tar.gz` from the `rust-v<ver>` GitHub release.
