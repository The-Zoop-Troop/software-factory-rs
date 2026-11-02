#!/usr/bin/env bash
# Proves docker/harness-auth.sh leaves only the chosen kind of credential. Runs in CI (job `rig`).
set -uo pipefail
cd "$(dirname "$0")" || exit 2

names=(CLAUDE_CODE_OAUTH_TOKEN CLAUDE_AUTH_JSON ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN
  CODEX_AUTH_JSON CODEX_OAUTH_TOKEN OPENAI_API_KEY OPENCODE_API_KEY)
failures=0

# Prints "<exit status>: <credential names still set>" for the given NAME=value assignments.
run() {
  (
    unset RIG_AUTH "${names[@]}"
    for assignment in "$@"; do export "${assignment?}"; done
    # shellcheck source=docker/harness-auth.sh
    . ./harness-auth.sh
    select_harness_auth 2>/dev/null
    status=$?
    kept=""
    for name in "${names[@]}"; do
      if [ -n "${!name:-}" ]; then kept="$kept $name"; fi
    done
    echo "$status:$kept"
  )
}

expect() {
  local title=$1 want=$2 got
  shift 2
  got=$(run "$@")
  if [ "$got" = "$want" ]; then
    echo "ok    $title"
  else
    echo "FAIL  $title"
    echo "      want: $want"
    echo "      got:  $got"
    failures=$((failures + 1))
  fi
}

# Exit status 0, both kinds present, each choice keeps only its own kind.
expect "subscription drops the Claude API key" "0: CLAUDE_CODE_OAUTH_TOKEN" \
  RIG_AUTH=subscription CLAUDE_CODE_OAUTH_TOKEN=t ANTHROPIC_API_KEY=k
expect "subscription drops the Claude auth token too" "0: CLAUDE_AUTH_JSON" \
  RIG_AUTH=subscription CLAUDE_AUTH_JSON=j ANTHROPIC_API_KEY=k ANTHROPIC_AUTH_TOKEN=a
expect "subscription drops the Codex API key" "0: CODEX_AUTH_JSON" \
  RIG_AUTH=subscription CODEX_AUTH_JSON=j OPENAI_API_KEY=k
expect "api-key drops the Claude plan logins" "0: ANTHROPIC_API_KEY" \
  RIG_AUTH=api-key CLAUDE_CODE_OAUTH_TOKEN=t CLAUDE_AUTH_JSON=j ANTHROPIC_API_KEY=k
expect "api-key drops the Codex plan logins" "0: OPENAI_API_KEY" \
  RIG_AUTH=api-key CODEX_AUTH_JSON=j CODEX_OAUTH_TOKEN=t OPENAI_API_KEY=k

# The OpenCode provider key is not part of the choice.
expect "subscription leaves the OpenCode key" "0: CLAUDE_CODE_OAUTH_TOKEN OPENCODE_API_KEY" \
  RIG_AUTH=subscription CLAUDE_CODE_OAUTH_TOKEN=t OPENCODE_API_KEY=o
expect "api-key leaves the OpenCode key" "0: ANTHROPIC_API_KEY OPENCODE_API_KEY" \
  RIG_AUTH=api-key ANTHROPIC_API_KEY=k OPENCODE_API_KEY=o

# No choice: one kind is unambiguous, both kinds refuse to start.
expect "no choice with only a plan login" "0: CLAUDE_CODE_OAUTH_TOKEN" CLAUDE_CODE_OAUTH_TOKEN=t
expect "no choice with only an API key" "0: ANTHROPIC_API_KEY" ANTHROPIC_API_KEY=k
expect "no choice with both Claude kinds refuses" "1: CLAUDE_CODE_OAUTH_TOKEN ANTHROPIC_API_KEY" \
  CLAUDE_CODE_OAUTH_TOKEN=t ANTHROPIC_API_KEY=k
expect "no choice with both Codex kinds refuses" "1: CODEX_AUTH_JSON OPENAI_API_KEY" \
  CODEX_AUTH_JSON=j OPENAI_API_KEY=k
expect "empty placeholders are not a conflict" "0: CLAUDE_CODE_OAUTH_TOKEN" \
  CLAUDE_CODE_OAUTH_TOKEN=t ANTHROPIC_API_KEY= CLAUDE_AUTH_JSON=

expect "an unknown choice refuses" "1: ANTHROPIC_API_KEY" RIG_AUTH=apikey ANTHROPIC_API_KEY=k

if [ "$failures" -ne 0 ]; then
  echo "$failures failure(s)"
  exit 1
fi
echo "harness-auth: ok"
