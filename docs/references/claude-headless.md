# Claude Code headless (`claude -p`) 2.1.316
- **Status:** reference · **Verified:** 2026-11-02 live probes on 2.1.316 of the invocation, all three tool policies, `--json-schema`, `--effort` and the result envelope, with `claude-haiku-5-5`, `claude-sonnet-5-5` and `claude-opus-5-5`. `--mcp-config` and the auth notes were last probed on 2.1.250, 2026-08-28.

- Invocation used: `claude -p "<prompt>" --output-format json --no-session-persistence --max-turns N --system-prompt "…" [--json-schema '<schema>'] [--model m] [--max-budget-usd x]`.
- Tools: `--tools ""` (none), `--tools Read,Glob,Grep --permission-mode plan` (read-only), `--tools default --dangerously-skip-permissions` (full).
- Result envelope (`type: "result"`): `is_error`, `result` (text), `structured_output` (when a schema was given), `num_turns`, `total_cost_usd`, `usage.{input_tokens,output_tokens,cache_creation_input_tokens,cache_read_input_tokens}`.
- Auth: OAuth credentials file on the host, or `CLAUDE_CODE_OAUTH_TOKEN` (from `claude setup-token`) / `ANTHROPIC_API_KEY` env in the rig. `--bare` skips keychain reads and therefore breaks OAuth auth — don't use it headless. In `-p` mode an API key always wins over a plan login when both are present, so the rig chooses explicitly with `RIG_AUTH` (`docker/harness-auth.sh`).
- Re-running `claude setup-token` or re-logging-in **revokes** earlier tokens (`401 OAuth access token has been revoked`).
- Honors `HTTPS_PROXY`.

## At a limit

Captured 2026-11-02 on 2.1.316 against a local stub provider with dummy credentials, one run per status. No real limit has been hit, so the wording and headers a real one produces are unconfirmed.

Every refusal printed a `result` envelope on stdout with exit code 1, `is_error: true`, `subtype: "success"` and `terminal_reason: "api_error"`. `api_error_status` carries the HTTP status.

| Provider answer | Login | `api_error` | `result` | `ProviderStop` |
|---|---|---|---|---|
| 429 with `anthropic-ratelimit-unified-status: rejected` | plan | `usage_limit_reached` | `You've hit your 5-hour limit · resets 3:26pm (America/New_York)`; the same for `weekly` | `UsageLimit` |
| 429 | API key | none | `API Error: Request rejected (429) · …` | `RateLimit` |
| 400 credit balance | either | none | `Credit balance is too low` | `SpendLimit` |
| 400 spend limit | API key | none | `API Error: 400 You have reached your specified API usage limits. …` | `SpendLimit` |
| 402 | API key | none | `API Error: 402 …` | `SpendLimit` |
| 500, 529 | API key | none | `API Error: 500 …` | `Outage` |
| 401 | API key | none | `Invalid API key · Fix external API key` | none |

- `unattended_start_limit`, also in the CLI's schema, applies to cloud sessions, not to `claude -p` here.
- `usage_limit_reached` also covers a spend cap on usage credits. The CLI's schema says other 429s never carry it, including a temporary server throttle or an organization policy that turned the service off.
- The envelope gives the reset only as local-time prose, so the factory polls instead of parsing it. A refused request runs no model.
- `model_requires_usage_credits` and `long_context_credits_required` are in the schema but were not captured. The second refuses a request over 200K tokens of context when credits are off, so a long session on a plan can fail that way. Neither is treated as a limit: waiting would not clear them, so they charge attempts and reach an operator.
