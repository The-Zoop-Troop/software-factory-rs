#!/usr/bin/env bash
# Conformance for one runtime image. Runs INSIDE the image as uid 10001 (docker/build.sh
# --conformance mounts this script and the runtime's sample/). Verify commands run the way the
# Verifier runs them: one per line, /bin/sh, from the repo root, repo root first on PATH.
set -euo pipefail
SAMPLE=${1:-/sample}
WORK=$(mktemp -d)
cp -r "$SAMPLE"/. "$WORK"/
cd "$WORK"
echo "runtime=${RIG_RUNTIME:-?} uid=$(id -u)"
[ "$(id -u)" != "0" ] || { echo "FAIL: running as root"; exit 1; }
grep -qE "^CapEff:\s*0+$" /proc/self/status || { echo "FAIL: capabilities present"; exit 1; }
factory version >/dev/null || { echo "FAIL: factory binary"; exit 1; }
factory doctor --repo "$WORK" --workdir /work/rig 2>/dev/null | grep -E "^(ok|FAIL) +runtime" || true
fail=0
while IFS= read -r cmd || [ -n "$cmd" ]; do
  case "$cmd" in ''|'#'*) continue;; esac
  if PATH="$WORK:$PATH" /bin/sh -c "$cmd" >/tmp/out 2>&1; then echo "ok   $cmd"; else echo "FAIL $cmd"; tail -20 /tmp/out; fail=1; fi
done < verify.txt
[ "$fail" = 0 ] && echo "conformance: ok" || { echo "conformance: FAILED"; exit 1; }
