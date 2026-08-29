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
if [ "$RUNTIME" != base ]; then
  [ -f "docker/runtimes/$RUNTIME/Dockerfile" ] || { echo "unknown runtime: $RUNTIME (see docker/runtimes/)"; exit 2; }
  echo "==> runtime $RUNTIME"
  docker build -q -f "docker/runtimes/$RUNTIME/Dockerfile" --build-arg BASE=factory-rig:base -t "factory-rig:$RUNTIME" docker/runtimes >/dev/null
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
