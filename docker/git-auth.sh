# shellcheck shell=bash
# Hosted-git token for a rig role. Sourced by docker/entrypoint.sh.
#
# A fine-grained token scoped to the project repo is applied as a URL rewrite in the role's own
# git config, so it never appears in RIG_REPO_URL, .gitmodules or a log line: both SSH and HTTPS
# forms of the host resolve to token-authenticated HTTPS. Any role may need it for the first
# clone, but only the Integrator pushes, so every other role drops it before its process starts.
# A Worker's session or the candidate's tests then cannot push or read the token.

# Every rewrite this file has ever written, whatever token it held.
_git_token_rewrites() {
  git config --global --name-only --get-regexp '^url\.https://x-access-token:' 2>/dev/null || true
}

apply_git_token() {
  [ -n "${RIG_GIT_TOKEN:-}" ] || return 0
  local host=${RIG_GIT_HOST:-github.com} name
  local key="url.https://x-access-token:${RIG_GIT_TOKEN}@${host}/.insteadOf"
  # HOME persists across container restarts, so clear rewrites a previous start left.
  for name in $(_git_token_rewrites); do git config --global --unset-all "$name" 2>/dev/null || true; done
  git config --global --add "$key" "git@${host}:"
  git config --global --add "$key" "https://${host}/"
}

# $1 = the role about to run. Keeps the token for `integrate` only.
drop_git_token() {
  [ "${1:-}" = integrate ] && return 0
  local name
  for name in $(_git_token_rewrites); do git config --global --unset-all "$name" 2>/dev/null || true; done
  unset RIG_GIT_TOKEN
}
