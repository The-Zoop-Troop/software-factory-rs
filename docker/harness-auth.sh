# shellcheck shell=bash
# Chooses which kind of credential a rig's harnesses use. Sourced by the rig entrypoint before any
# harness runs, and by docker/harness-auth.test.sh.
#
#   RIG_AUTH=subscription   a plan login: CLAUDE_CODE_OAUTH_TOKEN, CLAUDE_AUTH_JSON, CODEX_AUTH_JSON, CODEX_OAUTH_TOKEN
#   RIG_AUTH=api-key        ANTHROPIC_API_KEY, OPENAI_API_KEY
#
# Without a choice the variables present would decide, and inconsistently: Claude Code prefers an
# API key when it has both, and the Codex login below prefers a subscription. One env file could
# then bill the wrong account without saying so, so a rig holding both kinds for a harness and no
# choice refuses to start. OpenCode always uses its provider key; this does not touch it.

select_harness_auth() {
  case "${RIG_AUTH:-}" in
    subscription)
      unset ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN OPENAI_API_KEY
      ;;
    api-key)
      unset CLAUDE_CODE_OAUTH_TOKEN CLAUDE_AUTH_JSON CODEX_AUTH_JSON CODEX_OAUTH_TOKEN
      ;;
    "")
      if [ -n "${ANTHROPIC_API_KEY:-}" ] && [ -n "${CLAUDE_CODE_OAUTH_TOKEN:-}${CLAUDE_AUTH_JSON:-}" ]; then
        echo "[rig] both a Claude plan login and ANTHROPIC_API_KEY are set; set RIG_AUTH=subscription or RIG_AUTH=api-key" >&2
        return 1
      fi
      if [ -n "${OPENAI_API_KEY:-}" ] && [ -n "${CODEX_AUTH_JSON:-}${CODEX_OAUTH_TOKEN:-}" ]; then
        echo "[rig] both a Codex plan login and OPENAI_API_KEY are set; set RIG_AUTH=subscription or RIG_AUTH=api-key" >&2
        return 1
      fi
      ;;
    *)
      echo "[rig] RIG_AUTH must be subscription or api-key, not '${RIG_AUTH}'" >&2
      return 1
      ;;
  esac
}
