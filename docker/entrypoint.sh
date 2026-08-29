#!/usr/bin/env bash
# Rig entrypoint: bring up the ledger and clone if missing, then run one role.
#   rig-entrypoint steward|verify|integrate|work|plan <args>|planner|console|telegram|doctor|watch|inbox|shell|<any command>
set -euo pipefail

RIG_DIR=/work/rig
REPO_DIR=$RIG_DIR/repo
export BD_NON_INTERACTIVE=1

if [ ! -d "$RIG_DIR/.beads" ]; then
  echo "[rig] initialising ledger in $RIG_DIR (prefix ${RIG_PREFIX:-rig})"
  (cd "$RIG_DIR" && bd init --prefix "${RIG_PREFIX:-rig}" --non-interactive --skip-hooks >/dev/null 2>&1)
fi
bd metrics off >/dev/null 2>&1 || true

if [ ! -d "$REPO_DIR/.git" ]; then
  if [ -n "${RIG_REPO_URL:-}" ]; then
    echo "[rig] cloning $RIG_REPO_URL"
    git clone --quiet "$RIG_REPO_URL" "$REPO_DIR"
  else
    echo "[rig] no repo at $REPO_DIR and RIG_REPO_URL unset; creating an empty one"
    git init --quiet -b "${RIG_MAIN:-main}" "$REPO_DIR"
  fi
fi
git -C "$REPO_DIR" config user.name  "${RIG_GIT_NAME:-factory}"
git -C "$REPO_DIR" config user.email "${RIG_GIT_EMAIL:-factory@rig.local}"

mkdir -p "$RIG_DIR/.factory"
cd "$RIG_DIR"
echo "[rig] runtime=${RIG_RUNTIME:-base} harness=${RIG_HARNESS:-claude}"

# OpenCode provider config, generated from env so no credential is ever baked into the image.
if [ -n "${OPENCODE_PROVIDER_ID:-}" ]; then
  mkdir -p "$HOME/.config/opencode"
  jq -n --arg id "$OPENCODE_PROVIDER_ID" --arg name "${OPENCODE_PROVIDER_NAME:-$OPENCODE_PROVIDER_ID}" \
        --arg url "${OPENCODE_PROVIDER_BASE_URL:?OPENCODE_PROVIDER_BASE_URL required}" \
        --arg model "${OPENCODE_MODEL#*/}" '
    {"$schema":"https://opencode.ai/config.json",
     "permission":"allow",
     "provider": { ($id): { "npm":"@ai-sdk/openai-compatible", "name":$name,
                            "options": { "baseURL":$url, "apiKey":"{env:OPENCODE_API_KEY}" },
                            "models": { ($model): { "name":$model } } } } }' \
    > "$HOME/.config/opencode/opencode.json"
fi
# Claude Code: prefer CLAUDE_CODE_OAUTH_TOKEN (claude setup-token). CLAUDE_AUTH_JSON (base64 of a
# logged-in host's ~/.claude/.credentials.json, refresh token included) is the alternative when
# re-logins on the host keep revoking setup tokens.
if [ -n "${CLAUDE_AUTH_JSON:-}" ] && [ -z "${CLAUDE_CODE_OAUTH_TOKEN:-}" ]; then
  mkdir -p "$HOME/.claude" && printf '%s' "$CLAUDE_AUTH_JSON" | base64 -d > "$HOME/.claude/.credentials.json" && chmod 600 "$HOME/.claude/.credentials.json"
fi

# Codex CLI keeps its credential in $CODEX_HOME/auth.json; seed it from env at start so nothing
# is baked into the image. An access token (ChatGPT OAuth) expires; an API key does not.
if [ -n "${CODEX_AUTH_JSON:-}" ]; then
  # base64 of a logged-in host's ~/.codex/auth.json (ChatGPT OAuth tokens incl. refresh token).
  mkdir -p "$HOME/.codex" && printf '%s' "$CODEX_AUTH_JSON" | base64 -d > "$HOME/.codex/auth.json" && chmod 600 "$HOME/.codex/auth.json"
elif [ -n "${CODEX_OAUTH_TOKEN:-}" ]; then
  # An agent-identity JWT (not a ChatGPT access token).
  printf '%s' "$CODEX_OAUTH_TOKEN" | codex login --with-access-token >/dev/null 2>&1 || echo "[rig] codex login (access token) failed" >&2
elif [ -n "${OPENAI_API_KEY:-}" ]; then
  printf '%s' "$OPENAI_API_KEY" | codex login --with-api-key >/dev/null 2>&1 || echo "[rig] codex login (api key) failed" >&2
fi

HARNESS_ARGS=(--harness "${RIG_HARNESS:-claude}")
[ -n "${OPENCODE_MODEL:-}" ] && [ "${RIG_HARNESS:-claude}" = opencode ] && HARNESS_ARGS+=(--model "$OPENCODE_MODEL")
[ -n "${CODEX_MODEL:-}" ] && [ "${RIG_HARNESS:-claude}" = codex ] && HARNESS_ARGS+=(--model "$CODEX_MODEL")

role=${1:-shell}; shift || true
case "$role" in
  steward)   exec stewardd --workdir "$RIG_DIR" --events .factory/events.jsonl "$@" ;;
  verify)    exec factory --workdir "$RIG_DIR" verify    --repo "$REPO_DIR" --worktrees .factory/worktrees --events .factory/events.jsonl "$@" ;;
  integrate) exec factory --workdir "$RIG_DIR" integrate --repo "$REPO_DIR" --worktrees .factory/worktrees --events .factory/events.jsonl --main "${RIG_MAIN:-main}" "$@" ;;
  work)      exec factory --workdir "$RIG_DIR" work      --repo "$REPO_DIR" --worktrees .factory/worktrees --events .factory/events.jsonl --main "${RIG_MAIN:-main}" --agent "${RIG_AGENT:-worker-${HOSTNAME}}" "${HARNESS_ARGS[@]}" "$@" ;;
  plan)      exec factory --workdir "$RIG_DIR" plan      --repo "$REPO_DIR" --main "${RIG_MAIN:-main}" "${HARNESS_ARGS[@]}" "$@" ;;
  planner)   exec factory --workdir "$RIG_DIR" plan      --repo "$REPO_DIR" --main "${RIG_MAIN:-main}" "${HARNESS_ARGS[@]}" --queue --interval "${PLANNER_INTERVAL:-10}" "$@" ;;
  console)
    # Registry for this one rig, generated unless the operator mounted their own.
    mkdir -p /work/console
    [ -f /work/console/rigs.toml ] || printf '[[rig]]\nname = "%s"\nledger = "%s"\nevents = "%s/.factory/events.jsonl"\n' "${RIG_NAME:-toy}" "$RIG_DIR" "$RIG_DIR" > /work/console/rigs.toml
    [ -f /work/console/tokens.toml ] || { echo "[rig] console needs /work/console/tokens.toml (see docker/console/tokens.toml.example)"; exit 2; }
    exec console serve --registry /work/console/rigs.toml --tokens /work/console/tokens.toml --listen 0.0.0.0:7700 --public-url "${CONSOLE_URL:-http://127.0.0.1:7700}" "$@" ;;
  telegram)
    # Chat bot over the console; TELEGRAM_CHATS is a comma-separated allowlist of chat ids.
    chats=(); IFS=, read -ra ids <<< "${TELEGRAM_CHATS:-}"; for c in "${ids[@]}"; do [ -n "$c" ] && chats+=(--chat "$c"); done
    exec factory --rig "${FACTORY_RIG:-http://console:7700/rigs/${RIG_NAME:-toy}}" --token "${FACTORY_TOKEN:?set FACTORY_TOKEN to a console token}" telegram --bot-token "${TELEGRAM_BOT_TOKEN:?set TELEGRAM_BOT_TOKEN}" "${chats[@]}" "$@" ;;
  doctor|watch|inbox) exec factory --workdir "$RIG_DIR" "$role" "$@" ;;
  shell)     exec bash ;;
  *)         exec "$role" "$@" ;;
esac
