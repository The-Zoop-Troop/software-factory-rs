#!/usr/bin/env bash
# Proves docker/git-auth.sh leaves the git token with the Integrator only. Runs in CI (job `rig`).
set -uo pipefail
cd "$(dirname "$0")" || exit 2
failures=0

# Prints "<token still set>:<rewrites left in git config>" after applying, then dropping for $1.
# shellcheck disable=SC2030  # each case runs in its own subshell on purpose
run() {
  (
    HOME=$(mktemp -d); export HOME
    export RIG_GIT_TOKEN=secret-token
    # shellcheck source=docker/git-auth.sh
    . ./git-auth.sh
    apply_git_token
    drop_git_token "$1"
    set_=no; [ -n "${RIG_GIT_TOKEN:-}" ] && set_=yes
    rewrites=$(git config --global --get-regexp '^url\.' | wc -l | tr -d ' ')
    rm -rf "$HOME"
    echo "$set_:$rewrites"
  )
}

expect() {
  local title=$1 want=$2 got
  got=$(run "$3")
  if [ "$got" = "$want" ]; then
    echo "ok    $title"
  else
    echo "FAIL  $title"
    echo "      want: $want"
    echo "      got:  $got"
    failures=$((failures + 1))
  fi
}

expect "the Integrator keeps the token and both rewrites" "yes:2" integrate
for role in work verify steward planner plan shell doctor; do
  expect "$role drops the token and its rewrites" "no:0" "$role"
done

# A rewrite left by an earlier start with another token is cleared too.
stale=$(
  HOME=$(mktemp -d); export HOME
  git config --global --add "url.https://x-access-token:old-token@github.com/.insteadOf" "https://github.com/"
  # shellcheck source=docker/git-auth.sh
  . ./git-auth.sh
  drop_git_token work
  git config --global --get-regexp '^url\.' | wc -l | tr -d ' '
  rm -rf "$HOME"
)
if [ "$stale" = 0 ]; then echo "ok    a stale rewrite from an earlier token is dropped"; else echo "FAIL  stale rewrite kept"; failures=$((failures + 1)); fi

if [ "$failures" -ne 0 ]; then
  echo "$failures failure(s)"
  exit 1
fi
echo "git-auth: ok"
