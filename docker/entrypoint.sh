#!/usr/bin/env bash
# Rig entrypoint: bring up the ledger and clone if missing, then run one role.
#   rig-entrypoint steward|verify|integrate|plan <args>|shell|<any command>
set -euo pipefail

RIG_DIR=/work/rig
REPO_DIR=$RIG_DIR/repo
export BD_NON_INTERACTIVE=1

if [ ! -d "$RIG_DIR/.beads" ]; then
  echo "[rig] initialising ledger in $RIG_DIR (prefix ${RIG_PREFIX:-rig})"
  (cd "$RIG_DIR" && bd init --prefix "${RIG_PREFIX:-rig}" --non-interactive --skip-hooks >/dev/null 2>&1)
  bd metrics off >/dev/null 2>&1 || true
fi

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

role=${1:-shell}; shift || true
case "$role" in
  steward)   exec stewardd --workdir "$RIG_DIR" --events .factory/events.jsonl "$@" ;;
  verify)    exec factory --workdir "$RIG_DIR" verify    --repo "$REPO_DIR" --worktrees .factory/worktrees --events .factory/events.jsonl "$@" ;;
  integrate) exec factory --workdir "$RIG_DIR" integrate --repo "$REPO_DIR" --worktrees .factory/worktrees --events .factory/events.jsonl --main "${RIG_MAIN:-main}" "$@" ;;
  plan)      exec factory --workdir "$RIG_DIR" plan      --repo "$REPO_DIR" --main "${RIG_MAIN:-main}" "$@" ;;
  shell)     exec bash ;;
  *)         exec "$role" "$@" ;;
esac
