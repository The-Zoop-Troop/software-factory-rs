#!/usr/bin/env bash
# Build the rig images: the base, then the selected runtime (default: rust), and assemble the
# egress allowlist from base + runtime + project fragments.
#   docker/build.sh [runtime] [--project <dir>] [--conformance]
# A project may carry .factory/Dockerfile (FROM the runtime image) and .factory/allowlist.
set -euo pipefail
cd "$(dirname "$0")/.."
RUNTIME=rust; PROJECT=""; CONFORMANCE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --project) PROJECT=$2; shift ;;
    --conformance) CONFORMANCE=1 ;;
    *) RUNTIME=$1 ;;
  esac; shift
done
echo "==> base"
docker build -q -f docker/base/Dockerfile -t factory-rig:base . >/dev/null
IMAGE=factory-rig:base
# A runtime layers on the image named by its Dockerfile's `ARG BASE=factory-rig:<x>` default;
# build that parent first (web-e2e sits on node, polyglot on rust).
build_runtime() {
  local rt=$1 parent
  [ -f "docker/runtimes/$rt/Dockerfile" ] || { echo "unknown runtime: $rt (see docker/runtimes/)"; exit 2; }
  parent=$(sed -nE 's/^ARG BASE=factory-rig:([a-z0-9-]+)$/\1/p' "docker/runtimes/$rt/Dockerfile" | head -1)
  [ "${parent:-base}" = base ] || build_runtime "$parent"
  echo "==> runtime $rt (from ${parent:-base})"
  docker build -q -f "docker/runtimes/$rt/Dockerfile" --build-arg BASE="factory-rig:${parent:-base}" -t "factory-rig:$rt" docker/runtimes >/dev/null
  # Smoke: every runtime image must still carry the harness CLIs and the ledger tools; a PATH
  # change that drops them fails here, not in a rig's first plan.
  docker run --rm --entrypoint sh "factory-rig:$rt" -c 'for b in bd git dolt codex claude opencode; do command -v "$b" >/dev/null 2>&1 || { echo "smoke: $b missing from PATH ($PATH)"; exit 1; }; done' \
    || { echo "runtime image factory-rig:$rt failed its smoke check"; exit 1; }
}
if [ "$RUNTIME" != base ]; then
  build_runtime "$RUNTIME"
  IMAGE="factory-rig:$RUNTIME"
fi
if [ -n "$PROJECT" ] && [ -f "$PROJECT/.factory/Dockerfile" ]; then
  echo "==> project image from $PROJECT/.factory/Dockerfile"
  docker build -q -f "$PROJECT/.factory/Dockerfile" --build-arg BASE="$IMAGE" -t factory-rig:project "$PROJECT" >/dev/null
  IMAGE=factory-rig:project
fi
{ cat docker/egress/allowlist.base
  [ -f "docker/runtimes/$RUNTIME/allowlist.fragment" ] && cat "docker/runtimes/$RUNTIME/allowlist.fragment"
  [ -n "$PROJECT" ] && [ -f "$PROJECT/.factory/allowlist" ] && cat "$PROJECT/.factory/allowlist"
  true; } > docker/egress/allowlist
echo "==> egress allowlist: $(grep -cvE '^\s*(#|$)' docker/egress/allowlist) hosts"
docker build -q -f docker/Dockerfile.egress -t factory-egress:dev docker >/dev/null
if [ "$CONFORMANCE" = 1 ] && [ -d "docker/runtimes/$RUNTIME/sample" ]; then
  echo "==> conformance $RUNTIME"
  docker run --rm --cap-drop ALL --security-opt no-new-privileges:true \
    -v "$PWD/docker/runtimes/$RUNTIME/sample:/sample:ro" -v "$PWD/docker/runtimes/conformance.sh:/conformance.sh:ro" \
    -e RIG_RUNTIME="$RUNTIME" "$IMAGE" bash /conformance.sh /sample
fi
echo "RIG_IMAGE=$IMAGE"
