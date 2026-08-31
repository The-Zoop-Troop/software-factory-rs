#!/usr/bin/env bash
# Factory maintenance sweep: back up every registered rig, prune old archives, and warn on
# git tokens close to expiry. Run weekly (docker/systemd/factory-maintenance.timer) or by hand.
#   docker/maintenance.sh [--backups <dir>] [--retention-days <n>] [--warn-days <n>]
set -euo pipefail
cd "$(dirname "$0")/.."   # the factory repository: `factory rig` needs its compose.yaml

ROOT=${FACTORY_ROOT:-$HOME/.factory}
BACKUPS=${ROOT}/backups
RETENTION=30
WARN_DAYS=14
while [ $# -gt 0 ]; do
  case "$1" in
    --backups) BACKUPS=$2; shift ;;
    --retention-days) RETENTION=$2; shift ;;
    --warn-days) WARN_DAYS=$2; shift ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac; shift
done
mkdir -p "$BACKUPS"
warnings=0

rigs=$(factory rig list | awk 'NF {print $1}')
[ -n "$rigs" ] || { echo "maintenance: no rigs registered"; exit 0; }

echo "==> backups → $BACKUPS"
for rig in $rigs; do
  if factory rig backup "$rig" --to "$BACKUPS" >/dev/null; then
    echo "ok   $rig backed up"
  else
    echo "WARN $rig backup failed"; warnings=$((warnings + 1))
  fi
done

echo "==> retention: pruning archives older than ${RETENTION} days"
find "$BACKUPS" -name '*.tgz' -mtime +"$RETENTION" -print -delete | sed 's/^/pruned /'

echo "==> git token expiry (warn under ${WARN_DAYS} days)"
now=$(date +%s)
for rig in $rigs; do
  env_file="$ROOT/$rig/rig.env"
  [ -f "$env_file" ] || continue
  token=$(sed -n 's/^RIG_GIT_TOKEN=//p' "$env_file" | head -1)
  [ -n "$token" ] || continue
  headers=$(curl -sS -o /dev/null -D - -H "Authorization: Bearer $token" \
    https://api.github.com/rate_limit || true)
  status=$(printf '%s' "$headers" | head -1 | awk '{print $2}')
  expiry=$(printf '%s' "$headers" | tr -d '\r' | sed -n 's/^github-authentication-token-expiration: //Ip')
  if [ "$status" != 200 ]; then
    echo "WARN $rig git token: HTTP ${status:-none} (revoked or unreachable)"; warnings=$((warnings + 1))
  elif [ -n "$expiry" ]; then
    exp_s=$(date -d "$expiry" +%s 2>/dev/null || echo 0)
    days=$(( (exp_s - now) / 86400 ))
    if [ "$exp_s" = 0 ]; then
      echo "note $rig git token: unparseable expiry '$expiry'"
    elif [ "$days" -lt "$WARN_DAYS" ]; then
      echo "WARN $rig git token expires in ${days} day(s)"; warnings=$((warnings + 1))
    else
      echo "ok   $rig git token: ${days} days left"
    fi
  else
    echo "ok   $rig git token: no expiry (classic or non-expiring)"
  fi
done

if [ "$warnings" -gt 0 ]; then echo "maintenance: $warnings warning(s)"; exit 1; fi
echo "maintenance: clean"
