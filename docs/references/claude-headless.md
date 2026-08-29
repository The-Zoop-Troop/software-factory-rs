# Claude Code headless (`claude -p`) 2.1.250
- **Status:** reference · **Verified:** 2026-08-28 live probes

- Invocation used: `claude -p "<prompt>" --output-format json --no-session-persistence --max-turns N --system-prompt "…" [--json-schema '<schema>'] [--model m] [--max-budget-usd x]`.
- Tools: `--tools ""` (none), `--tools Read,Glob,Grep --permission-mode plan` (read-only), `--tools default --dangerously-skip-permissions` (full).
- Result envelope (`type: "result"`): `is_error`, `result` (text), `structured_output` (when a schema was given), `num_turns`, `total_cost_usd`, `usage.{input_tokens,output_tokens,cache_creation_input_tokens,cache_read_input_tokens}`.
- Auth: OAuth credentials file on the host, or `CLAUDE_CODE_OAUTH_TOKEN` (from `claude setup-token`) / `ANTHROPIC_API_KEY` env in the rig. `--bare` skips keychain reads and therefore breaks OAuth auth — don't use it headless.
- Re-running `claude setup-token` or re-logging-in **revokes** earlier tokens (`401 OAuth access token has been revoked`).
- Honors `HTTPS_PROXY`.
