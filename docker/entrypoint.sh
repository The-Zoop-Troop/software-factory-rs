#!/usr/bin/env bash
# Rig entrypoint: bring up the ledger and clone if missing, then run one role.
#   rig-entrypoint steward|verify|integrate|work|plan <args>|shell|<any command>
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
  shell)     exec bash ;;
  *)         exec "$role" "$@" ;;
esac
